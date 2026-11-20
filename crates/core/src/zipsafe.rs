//! Zip reading and writing for backups and packs (SPEC §12, §13). Archives are untrusted:
//! every entry name is checked before anything is read, symlinks and encrypted entries are
//! refused, and sizes are enforced on the decompressed stream, not on the headers.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Result;
use crate::error::invalid;

pub const MAX_ENTRIES: usize = 200_000;
/// Any single entry, decompressed (above every media size limit).
pub const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;
/// Any JSON / JSONL entry.
pub const MAX_JSON_BYTES: u64 = 50 * 1024 * 1024;
/// The whole archive, decompressed.
pub const MAX_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn zip_err(e: zip::result::ZipError) -> crate::CoreError {
    invalid(format!("Not a readable zip archive: {e}"))
}

/// Relative, forward-slash, no `..`, `.`, empty segments, drive letters or NUL.
fn check_name(name: &str) -> Result<()> {
    let body = name.strip_suffix('/').unwrap_or(name);
    let bad = body.is_empty()
        || body.starts_with('/')
        || body.contains(['\\', ':', '\0'])
        || body
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..");
    if bad {
        return Err(invalid(format!(
            "Archive rejected: unsafe entry name {name:?}"
        )));
    }
    Ok(())
}

pub struct SafeZip {
    archive: ZipArchive<BufReader<File>>,
    /// File entries (directories dropped), in archive order.
    names: Vec<String>,
    read_total: u64,
}

impl SafeZip {
    pub fn open(path: &Path) -> Result<SafeZip> {
        let mut archive = ZipArchive::new(BufReader::new(File::open(path)?)).map_err(zip_err)?;
        if archive.len() > MAX_ENTRIES {
            return Err(invalid("Archive rejected: too many entries"));
        }
        let mut names = Vec::with_capacity(archive.len());
        let mut declared = 0u64;
        for i in 0..archive.len() {
            let f = archive.by_index(i).map_err(zip_err)?;
            let name = f.name().map_err(zip_err)?.to_string();
            check_name(&name)?;
            if f.is_symlink() {
                return Err(invalid(format!("Archive rejected: {name} is a symlink")));
            }
            if f.encrypted() {
                return Err(invalid(format!("Archive rejected: {name} is encrypted")));
            }
            declared = declared.saturating_add(f.size());
            if declared > MAX_TOTAL_BYTES || f.size() > MAX_FILE_BYTES {
                return Err(invalid("Archive rejected: too large"));
            }
            if !f.is_dir() {
                names.push(name);
            }
        }
        Ok(SafeZip {
            archive,
            names,
            read_total: 0,
        })
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    /// Streams one entry into `out`, refusing more than `cap` bytes (headers can lie).
    fn copy(&mut self, name: &str, cap: u64, out: &mut impl Write) -> Result<u64> {
        let f = self.archive.by_name(name).map_err(zip_err)?;
        let n = std::io::copy(&mut f.take(cap + 1), out)?;
        self.read_total += n;
        if n > cap || self.read_total > MAX_TOTAL_BYTES {
            return Err(invalid(format!("Archive rejected: {name} is too large")));
        }
        Ok(n)
    }

    pub fn read(&mut self, name: &str, cap: u64) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.copy(name, cap, &mut buf)?;
        Ok(buf)
    }

    pub fn read_json<T: DeserializeOwned>(&mut self, name: &str) -> Result<T> {
        let bytes = self.read(name, MAX_JSON_BYTES)?;
        serde_json::from_slice(&bytes).map_err(|e| invalid(format!("{name}: {e}")))
    }

    /// Extracts every file entry below `dir` (which must not exist yet).
    pub fn extract_to(&mut self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        for name in self.names.clone() {
            let target = dir.join(&name);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let cap = if is_json(&name) {
                MAX_JSON_BYTES
            } else {
                MAX_FILE_BYTES
            };
            let mut out = BufWriter::new(File::create(&target)?);
            self.copy(&name, cap, &mut out)?;
            out.flush()?;
        }
        Ok(())
    }
}

pub fn is_json(name: &str) -> bool {
    name.ends_with(".json") || name.ends_with(".jsonl")
}

/// Writes to `<out>.part` and renames on `finish`, so a failed export leaves no half archive.
pub struct ZipOut {
    zip: ZipWriter<BufWriter<File>>,
    part: PathBuf,
    out: PathBuf,
}

impl ZipOut {
    pub fn create(out: &Path) -> Result<ZipOut> {
        let mut part = out.as_os_str().to_owned();
        part.push(".part");
        let part = PathBuf::from(part);
        Ok(ZipOut {
            zip: ZipWriter::new(BufWriter::new(File::create(&part)?)),
            part,
            out: out.to_path_buf(),
        })
    }

    fn start(&mut self, name: &str, compress: bool) -> Result<()> {
        let method = if compress {
            CompressionMethod::Deflated
        } else {
            CompressionMethod::Stored
        };
        self.zip
            .start_file(
                name,
                SimpleFileOptions::default()
                    .compression_method(method)
                    .large_file(true),
            )
            .map_err(|e| invalid(format!("zip: {e}")))
    }

    pub fn bytes(&mut self, name: &str, data: &[u8]) -> Result<()> {
        self.start(name, true)?;
        self.zip.write_all(data)?;
        Ok(())
    }

    pub fn json<T: Serialize + ?Sized>(&mut self, name: &str, value: &T) -> Result<()> {
        self.bytes(name, &serde_json::to_vec_pretty(value)?)
    }

    /// Media is already compressed: stored as is, streamed from disk.
    pub fn file(&mut self, name: &str, path: &Path) -> Result<()> {
        self.start(name, false)?;
        std::io::copy(&mut BufReader::new(File::open(path)?), &mut self.zip)?;
        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        let mut w = self
            .zip
            .finish()
            .map_err(|e| invalid(format!("zip: {e}")))?;
        w.flush()?;
        drop(w);
        std::fs::rename(&self.part, &self.out)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_checked() {
        for ok in ["cards/a.json", "media/ab/abc.png", "cards/"] {
            assert!(check_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "../x",
            "a/../../x",
            "/etc/passwd",
            "a\\b",
            "C:/x",
            "a//b",
            "./a",
            "",
            "a\0b",
        ] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
    }
}
