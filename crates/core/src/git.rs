//! Git transport for sync (owner ruling 2026-11-12, as in Hearth/Bonfire): the vault directory
//! is a git repository whose `origin` is the user's private GitHub repo.
//!
//! The system `git` does the work, so the user's SSH agent or credential helper (for example
//! `gh auth setup-git`) authenticates and Indigo never sees a credential. Prompts are switched
//! off: a background sync fails with a message instead of waiting for a password.
//!
//! Git is never asked to merge file contents. `fetch` copies the remote tree out of the object
//! store; core merges records; `push` commits a tree built as "remote tree + this device's
//! vault on top" with both histories as parents, so the push is always a fast-forward and a
//! textual conflict cannot happen.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::Result;
use crate::error::invalid;
use crate::media;
use crate::sync::Transport;
use crate::vault;

/// Longest a network call (fetch, push) may run before it is stopped.
const NETWORK_WALL_CLOCK: Duration = Duration::from_secs(600);
/// GitHub refuses files of 100 MB or more; stop before pushing one.
pub const MAX_FILE_BYTES: u64 = 95 * 1024 * 1024;
const BRANCH: &str = "main";
const REMOTE_MAIN: &str = "refs/remotes/origin/main";

pub struct GitTransport {
    /// The vault directory, which is the repository's working tree.
    pub dir: PathBuf,
    pub url: String,
    pub device_name: String,
}

/// A repository URL Indigo will hand to git: no leading `-` (it would read as an option) and
/// no whitespace or control characters.
pub fn valid_url(url: &str) -> bool {
    !url.is_empty()
        && !url.starts_with('-')
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

pub fn is_available() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn command(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .stdin(Stdio::null());
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        c.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    c
}

fn stderr_text(bytes: &[u8], args: &[&str]) -> String {
    let s = String::from_utf8_lossy(bytes).trim().to_string();
    if s.is_empty() {
        format!("git {} failed", args.join(" "))
    } else {
        s
    }
}

/// Runs git and returns trimmed stdout, or stderr as the error.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    git_env(dir, args, &[])
}

fn git_env(dir: &Path, args: &[&str], env: &[(&str, &Path)]) -> Result<String> {
    let mut c = command(dir);
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c.args(args).output().map_err(|e| {
        invalid(match e.kind() {
            std::io::ErrorKind::NotFound => "git is not installed; Indigo needs it to sync".into(),
            _ => format!("could not run git: {e}"),
        })
    })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        Err(invalid(stderr_text(&out.stderr, args)))
    }
}

/// A network command with a wall clock; Err carries git's stderr.
fn git_network(dir: &Path, args: &[&str]) -> Result<()> {
    let mut child = command(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| invalid(format!("could not run git: {e}")))?;
    let mut stderr = child.stderr.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = stderr.read_to_end(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > NETWORK_WALL_CLOCK {
            let _ = child.kill();
            let _ = child.wait();
            return Err(invalid("git took too long and was stopped"));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let err = reader.join().unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        Err(invalid(explain(&stderr_text(&err, args))))
    }
}

/// Adds the likely fix to the auth failures people actually hit.
fn explain(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("could not read username")
        || lower.contains("authentication failed")
        || lower.contains("permission denied (publickey)")
    {
        format!("{err}\nGitHub sign-in failed. Run `gh auth login` then `gh auth setup-git` once.")
    } else if lower.contains("repository not found") {
        format!("{err}\nCheck the repository URL in Settings → Sync and that you can access it.")
    } else {
        err.to_string()
    }
}

/// True when git failed because the network is unreachable: a sync then simply waits (SPEC
/// §11.3, "offline is silent") instead of asking for attention.
pub fn is_offline(err: &str) -> bool {
    let e = err.to_lowercase();
    [
        "could not resolve host",
        "could not resolve hostname",
        "temporary failure in name resolution",
        "network is unreachable",
        "connection timed out",
        "connection refused",
        "failed to connect to",
        "operation timed out",
    ]
    .iter()
    .any(|p| e.contains(p))
}

fn rev(dir: &Path, name: &str) -> Option<String> {
    git(dir, &["rev-parse", "--verify", "-q", name]).ok()
}

fn is_ancestor(dir: &Path, a: &str, b: &str) -> bool {
    command(dir)
        .args(["merge-base", "--is-ancestor", a, b])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A path from the remote tree that is safe to write under a local directory.
fn safe_rel(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != ".." && !c.contains(':'))
}

/// `media/<hh>/<sha256>.<ext>` with a valid hash and an allowed extension.
fn media_rel(path: &str) -> Option<&str> {
    let rel = path.strip_prefix("media/")?;
    let (hh, file) = rel.split_once('/')?;
    let (hash, ext) = file.rsplit_once('.')?;
    (vault::valid_hash(hash) && hash.starts_with(hh) && hh.len() == 2 && media::is_allowed_ext(ext))
        .then_some(rel)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

impl GitTransport {
    /// `git init` in place (the vault may already hold files), origin set to `url`, and a
    /// repo-local identity when none is configured so a commit can never fail for that.
    fn ensure_repo(&self) -> Result<()> {
        if !valid_url(&self.url) {
            return Err(invalid("The sync repository URL is not valid"));
        }
        std::fs::create_dir_all(&self.dir)?;
        if !self.dir.join(".git").exists() {
            git(&self.dir, &["init", "-q", "-b", BRANCH])?;
        }
        for (key, fallback) in [("user.email", "indigo@localhost"), ("user.name", "Indigo")] {
            if git(&self.dir, &["config", key]).is_err() {
                git(&self.dir, &["config", key, fallback])?;
            }
        }
        match git(&self.dir, &["remote", "get-url", "origin"]) {
            Ok(current) if current == self.url => {}
            Ok(_) => {
                git(&self.dir, &["remote", "set-url", "origin", &self.url])?;
            }
            Err(_) => {
                git(&self.dir, &["remote", "add", "origin", &self.url])?;
            }
        }
        std::fs::write(self.dir.join(".git/info/exclude"), "*.part\n")?;
        Ok(())
    }

    /// Streams the named blobs out of the object store: `(object id, destination)`.
    fn write_blobs(&self, blobs: &[(String, PathBuf)]) -> Result<()> {
        if blobs.is_empty() {
            return Ok(());
        }
        let mut child = command(&self.dir)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdin = child.stdin.take().expect("piped");
        let ids: Vec<String> = blobs.iter().map(|(id, _)| id.clone()).collect();
        let feeder = std::thread::spawn(move || -> std::io::Result<()> {
            for id in ids {
                writeln!(stdin, "{id}")?;
            }
            Ok(())
        });
        let mut out = BufReader::new(child.stdout.take().expect("piped"));
        for (id, dest) in blobs {
            let mut header = String::new();
            out.read_line(&mut header)?;
            let mut parts = header.split_whitespace();
            let (Some(got), Some("blob"), Some(size)) = (parts.next(), parts.next(), parts.next())
            else {
                return Err(invalid(format!("git cat-file: unexpected {header:?}")));
            };
            if got != id {
                return Err(invalid("git cat-file returned objects out of order"));
            }
            let size: u64 = size
                .parse()
                .map_err(|_| invalid("git cat-file: bad size"))?;
            let mut bytes = Vec::with_capacity(size as usize);
            (&mut out).take(size).read_to_end(&mut bytes)?;
            let mut newline = [0u8; 1];
            out.read_exact(&mut newline)?;
            write_atomic(dest, &bytes)?;
        }
        let _ = feeder.join();
        child.wait()?;
        Ok(())
    }
}

impl Transport for GitTransport {
    fn fetch(&self, mirror: &Path, media_dir: &Path) -> Result<()> {
        self.ensure_repo()?;
        git_network(&self.dir, &["fetch", "--quiet", "--no-tags", "origin"])?;
        if rev(&self.dir, REMOTE_MAIN).is_none() {
            return Ok(()); // a new, empty repository
        }
        let listing = git(&self.dir, &["ls-tree", "-r", "-z", REMOTE_MAIN])?;
        let mut blobs = Vec::new();
        for entry in listing.split('\0').filter(|e| !e.is_empty()) {
            let Some((meta, path)) = entry.split_once('\t') else {
                continue;
            };
            let mut m = meta.split(' ');
            let (Some(mode), Some("blob"), Some(id)) = (m.next(), m.next(), m.next()) else {
                continue; // submodules and anything else that is not a file
            };
            if !matches!(mode, "100644" | "100755") || !safe_rel(path) {
                continue; // symlinks and unsafe paths are never written
            }
            if path.starts_with("media/") {
                if let Some(rel) = media_rel(path)
                    && !media_dir.join(rel).exists()
                {
                    blobs.push((id.to_string(), media_dir.join(rel)));
                }
            } else {
                blobs.push((id.to_string(), mirror.join(path)));
            }
        }
        self.write_blobs(&blobs)
    }

    fn push(&self, vault_dir: &Path) -> Result<bool> {
        debug_assert_eq!(vault_dir, self.dir);
        self.ensure_repo()?;
        let remote = rev(&self.dir, REMOTE_MAIN);
        let head = rev(&self.dir, "HEAD");

        // Tree = the remote's tree with this device's vault laid over it. Files only the
        // remote has (other devices' reviews) stay; nothing is ever removed.
        let index = self.dir.join(".git").join("indigo-index");
        let _ = std::fs::remove_file(&index);
        let env = [("GIT_INDEX_FILE", index.as_path())];
        match &remote {
            Some(r) => git_env(&self.dir, &["read-tree", r], &env)?,
            None => git_env(&self.dir, &["read-tree", "--empty"], &env)?,
        };
        git_env(&self.dir, &["add", "--ignore-removal", "--", "."], &env)?;
        let tree = git_env(&self.dir, &["write-tree"], &env)?;
        let _ = std::fs::remove_file(&index);

        for line in git(&self.dir, &["ls-tree", "-r", "-l", &tree])?.lines() {
            let mut parts = line.split_whitespace();
            let size = parts
                .nth(3)
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            if size >= MAX_FILE_BYTES {
                let name = line.split('\t').nth(1).unwrap_or(line);
                return Err(invalid(format!(
                    "{name} is too large for GitHub ({} MB); remove it from its card",
                    size / 1_048_576
                )));
            }
        }

        let remote_tree = remote
            .as_ref()
            .and_then(|r| git(&self.dir, &["rev-parse", &format!("{r}^{{tree}}")]).ok());
        if remote_tree.as_deref() == Some(tree.as_str()) {
            // Nothing new here: just catch the local branch up.
            let r = remote.expect("remote tree implies remote");
            git(
                &self.dir,
                &["update-ref", &format!("refs/heads/{BRANCH}"), &r],
            )?;
            return Ok(true);
        }

        let mut parents: Vec<String> = Vec::new();
        match (&head, &remote) {
            (Some(h), Some(r)) if is_ancestor(&self.dir, h, r) => parents.push(r.clone()),
            (Some(h), Some(r)) if is_ancestor(&self.dir, r, h) => parents.push(h.clone()),
            (Some(h), Some(r)) => parents.extend([h.clone(), r.clone()]),
            (Some(h), None) => parents.push(h.clone()),
            (None, Some(r)) => parents.push(r.clone()),
            (None, None) => {}
        }
        let message = format!("Sync from {}", self.device_name);
        let mut args: Vec<&str> = vec!["commit-tree", &tree, "-m", &message];
        for p in &parents {
            args.extend(["-p", p]);
        }
        let commit = git(&self.dir, &args)?;
        git(
            &self.dir,
            &["update-ref", &format!("refs/heads/{BRANCH}"), &commit],
        )?;
        if head.is_none() {
            git(
                &self.dir,
                &["symbolic-ref", "HEAD", &format!("refs/heads/{BRANCH}")],
            )?;
        }

        match git_network(&self.dir, &["push", "--quiet", "origin", BRANCH]) {
            Ok(()) => Ok(true),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("[rejected]")
                    || msg.contains("fetch first")
                    || msg.contains("non-fast-forward")
                {
                    Ok(false) // someone pushed since our fetch; the caller syncs again
                } else {
                    Err(e)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_paths_are_checked() {
        assert!(valid_url("https://github.com/me/indigo-vault.git"));
        assert!(valid_url("git@github.com:me/indigo-vault.git"));
        assert!(!valid_url("--upload-pack=x"));
        assert!(!valid_url("https://x y"));
        assert!(safe_rel("cards/ab.json"));
        for bad in ["", "/etc/x", "a/../b", "a//b", "./a", "c:\\x", "a\\b"] {
            assert!(!safe_rel(bad), "{bad}");
        }
        let h = "ab".repeat(32);
        assert_eq!(
            media_rel(&format!("media/ab/{h}.webm")),
            Some(format!("ab/{h}.webm").as_str())
        );
        assert_eq!(media_rel(&format!("media/cd/{h}.webm")), None);
        assert_eq!(media_rel(&format!("media/ab/{h}.exe")), None);
    }

    #[test]
    fn offline_errors_are_recognised() {
        assert!(is_offline(
            "fatal: unable to access 'https://github.com/a/b.git/': Could not resolve host: github.com"
        ));
        assert!(is_offline(
            "ssh: connect to host github.com port 22: Network is unreachable"
        ));
        assert!(!is_offline("remote: Repository not found."));
        assert!(!is_offline(
            "fatal: Authentication failed for 'https://github.com/a/b.git/'"
        ));
    }
}
