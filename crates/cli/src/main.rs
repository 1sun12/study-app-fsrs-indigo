//! `indigo-cli`: Indigo's own commands as JSON in, JSON out, for scripts and AI harnesses
//! (`docs/CLI.md`, printed by `indigo-cli guide`). It works on the app's database directly, as
//! this device, so its edits sync on the app's next sync. By design it cannot delete a card,
//! make one disappear, run a sync, change the sync repository, or study and grade.

use std::error::Error;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chrono::{DateTime, Utc};
use clap::Parser;
use indigo_core::db::Db;
use indigo_core::domain::{Card, CardFilter};
use indigo_core::sched::day::Clock;
use indigo_core::settings::{KEY_SETTINGS, Settings};
use indigo_core::zipsafe::SafeZip;
use indigo_core::{authoring, backup, media, pack, sim, stats, sync, vault, warnings};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const GUIDE: &str = include_str!("../../../docs/CLI.md");
/// The app's identifier (tauri.conf.json): its data folder has this name on every OS.
const APP_ID: &str = "app.indigo.desktop";

/// Everything the app can do except deleting cards, syncing, and studying.
const COMMANDS: &[&str] = &[
    "categories_list",
    "category_save",
    "category_delete",
    "cards_query",
    "card_get",
    "card_save",
    "card_warnings",
    "card_set_state",
    "tag_rename",
    "tag_delete",
    "stats_overview",
    "media_import_path",
    "media_cleanup",
    "backup_export",
    "backup_import",
    "pack_export",
    "pack_inspect",
    "pack_import",
    "settings_get",
    "settings_set",
    "sched_config_get",
    "sched_config_set",
    "sched_preview",
    "sync_status",
    "conflicts_list",
    "conflict_restore",
    "conflict_dismiss",
];

type Res<T> = Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(
    version,
    about = "Indigo's commands as JSON in, JSON out. Start with `indigo-cli guide`."
)]
struct Cli {
    /// The app's data folder; default: the installed app's.
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Run the command once per line of JSON read from stdin (one result per line).
    #[arg(long)]
    each: bool,
    /// `guide`, `commands`, or a command from `indigo-cli commands`.
    command: String,
    /// The command's arguments as one JSON object, or `-` to read it from stdin.
    args: Option<String>,
}

struct Ctx {
    db: Db,
    data_dir: PathBuf,
    media_dir: PathBuf,
    device_id: String,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command.as_str() {
        "guide" => print!("{GUIDE}"),
        "commands" => println!("{}", COMMANDS.join("\n")),
        _ => match run(cli) {
            Ok(true) => {}
            Ok(false) => return ExitCode::FAILURE,
            Err(e) => {
                eprintln!("{}", json!({ "error": e.to_string() }));
                return ExitCode::FAILURE;
            }
        },
    }
    ExitCode::SUCCESS
}

/// `Ok(false)`: some `--each` lines failed (each reported on stderr).
fn run(cli: Cli) -> Res<bool> {
    if !COMMANDS.contains(&cli.command.as_str()) {
        return Err(format!(
            "unknown command `{}`; see `indigo-cli commands` (deleting cards, syncing and \
             studying are not available here)",
            cli.command
        )
        .into());
    }
    let data_dir = match cli.data_dir {
        Some(d) => d,
        None => dirs::data_dir()
            .ok_or("this system has no data folder; pass --data-dir")?
            .join(APP_ID),
    };
    let db_path = data_dir.join("indigo.sqlite");
    if !db_path.is_file() {
        return Err(format!(
            "no Indigo library in {}; start Indigo once first",
            data_dir.display()
        )
        .into());
    }
    let db = Db::open(&db_path)?;
    let ctx = Ctx {
        device_id: db.device_id()?,
        media_dir: vault::media_dir(&data_dir)?,
        db,
        data_dir,
    };

    if cli.each {
        let mut all_ok = true;
        for (i, line) in std::io::stdin().lock().lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str(&line)
                .map_err(Into::into)
                .and_then(|args| dispatch(&ctx, &cli.command, args))
            {
                Ok(v) => println!("{v}"),
                Err(e) => {
                    all_ok = false;
                    eprintln!("{}", json!({ "line": i + 1, "error": e.to_string() }));
                }
            }
        }
        return Ok(all_ok);
    }

    let args = match cli.args.as_deref() {
        None => json!({}),
        Some("-") => {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            serde_json::from_str(&s)?
        }
        Some(s) => serde_json::from_str(s)?,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&dispatch(&ctx, &cli.command, args)?)?
    );
    Ok(true)
}

/// One field of the arguments object; a missing field reads as `null`, so optional ones may be
/// left out.
fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Res<T> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| format!("`{key}`: {e}").into())
}

fn out(v: impl serde::Serialize) -> Res<Value> {
    Ok(serde_json::to_value(v)?)
}

fn done() -> Res<Value> {
    Ok(json!({ "ok": true }))
}

fn dispatch(c: &Ctx, cmd: &str, a: Value) -> Res<Value> {
    let db = &c.db;
    let cfg = db.sched_config()?;
    let clock = Clock::now(cfg.rollover_hour);
    let now = clock.now;
    let dev = c.device_id.as_str();
    let path = || arg::<String>(&a, "path").map(PathBuf::from);
    match cmd {
        "categories_list" => out(db.categories_list()?),
        "category_save" => out(authoring::save_category(
            db,
            filled(category_defaults(now), arg(&a, "category")?)?,
            now,
        )?),
        "category_delete" => {
            // Core refuses while live cards use the category, so no card goes with it.
            db.delete_category(&arg::<String>(&a, "id")?, now)?;
            done()
        }
        "cards_query" => {
            let mut filter = arg::<Option<CardFilter>>(&a, "filter")?.unwrap_or_default();
            filter.today.get_or_insert(clock.today);
            out(db.cards_query(&filter)?)
        }
        "card_get" => out(db
            .get_card(&arg::<String>(&a, "id")?)?
            .filter(|c| c.deleted_at.is_none())),
        // save_card always clears `deleted_at`: saving never hides a card.
        "card_save" => out(authoring::save_card(
            db,
            card_from(arg(&a, "card")?, now)?,
            now,
            dev,
            &cfg,
            false,
        )?),
        "card_warnings" => out(warnings::card_warnings(&card_from(arg(&a, "card")?, now)?)),
        "card_set_state" => {
            db.set_card_state(&arg::<String>(&a, "id")?, arg(&a, "card_state")?, now, dev)?;
            done()
        }
        "tag_rename" => out(authoring::rename_tag(
            db,
            &arg::<String>(&a, "from")?,
            &arg::<String>(&a, "to")?,
            now,
            dev,
        )?),
        "tag_delete" => out(authoring::delete_tag(
            db,
            &arg::<String>(&a, "tag")?,
            now,
            dev,
        )?),
        "stats_overview" => out(stats::overview(db, clock.today, &cfg)?),
        "media_import_path" => out(media::import_file(
            db,
            &c.media_dir,
            &path()?,
            &c.data_dir.join("tmp"),
            now,
        )?),
        "media_cleanup" => out(media::cleanup(db, &c.media_dir)?),
        "backup_export" => out(backup::export(db, &c.media_dir, &path()?, now)?),
        "backup_import" => {
            let path = path()?;
            refuse_tombstones(db, &path)?;
            out(backup::import(db, &c.media_dir, &path, &c.data_dir, now)?)
        }
        "pack_export" => out(pack::export(
            db,
            &c.media_dir,
            &arg::<Vec<String>>(&a, "card_ids")?,
            &arg::<String>(&a, "name")?,
            &path()?,
            now,
        )?),
        "pack_inspect" => out(pack::inspect(db, &path()?)?),
        // Packs carry no tombstones (PackCard has no deleted_at), so an import never hides a card.
        "pack_import" => out(pack::import(
            db,
            &c.media_dir,
            &path()?,
            &arg::<Option<Vec<String>>>(&a, "update")?.unwrap_or_default(),
            now,
            dev,
            &cfg,
        )?),
        "settings_get" => out(db.settings()?),
        "settings_set" => {
            let mut s: Settings = arg(&a, "settings")?;
            s.sync = db.settings()?.sync; // the sync repository is the owner's to change
            db.setting_set(KEY_SETTINGS, &s)?;
            done()
        }
        "sched_config_get" => out(cfg),
        "sched_config_set" => {
            vault::set_config(db, &arg(&a, "config")?, now, dev)?;
            done()
        }
        "sched_preview" => out(sim::retention_preview(&cfg, arg(&a, "retention")?)?),
        "sync_status" => Ok(json!({
            "configured": db.settings()?.sync.remote().is_some(),
            "status": sync::status(db)?,
        })),
        "conflicts_list" => out(sync::conflicts(db)?),
        "conflict_restore" => {
            let id: String = arg(&a, "id")?;
            refuse_restoring_a_tombstone(db, &id)?;
            sync::restore_conflict(db, &id, now, dev, &cfg)?;
            done()
        }
        "conflict_dismiss" => {
            sync::dismiss_conflict(db, &arg::<String>(&a, "id")?)?;
            done()
        }
        _ => unreachable!("checked against COMMANDS"),
    }
}

/// `given` laid over `defaults` (top-level fields; given ones win).
fn filled<T: DeserializeOwned>(mut defaults: Value, given: Value) -> Res<T> {
    match (defaults.as_object_mut(), given) {
        (Some(d), Value::Object(g)) => d.extend(g),
        (_, other) => return Err(format!("expected a JSON object, got {other}").into()),
    }
    Ok(serde_json::from_value(defaults)?)
}

fn category_defaults(now: DateTime<Utc>) -> Value {
    json!({ "id": "", "syntax": null, "color": null, "position": 0, "modified_at": now,
            "deleted_at": null })
}

/// A card with everything a new one may leave out filled in, so the smallest card is
/// `{title, category_id, steps: [{prompt_md, solution_md}]}`. Core assigns ids and timestamps.
fn card_from(mut given: Value, now: DateTime<Utc>) -> Res<Card> {
    if let Some(steps) = given.get_mut("steps").and_then(Value::as_array_mut) {
        for step in steps {
            if let Some(criteria) = step.get_mut("criteria").and_then(Value::as_array_mut) {
                for c in criteria {
                    *c = filled(json!({ "id": "", "weight": 1.0 }), c.take())?;
                }
            }
            *step = filled(
                json!({ "id": "", "response": "recall", "solution_md": "", "solution_text": null,
                        "criteria": [], "points": 1.0, "media": [] }),
                step.take(),
            )?;
        }
    }
    filled(
        json!({ "id": "", "difficulty": "beginner", "tags": [], "est_minutes": null,
                "source": null, "pack_id": null, "pack_card_id": null, "state": "shelved",
                "created_at": now, "modified_at": now, "modified_by": "", "deleted_at": null }),
        given,
    )
}

/// A backup merge applies its tombstones; refuse one that would hide a card this library has.
fn refuse_tombstones(db: &Db, archive: &Path) -> Res<()> {
    let mut z = SafeZip::open(archive)?;
    let names: Vec<String> = z
        .names()
        .iter()
        .filter(|n| n.starts_with("cards/") && n.ends_with(".json"))
        .cloned()
        .collect();
    for name in names {
        let card: Card = z.read_json(&name)?;
        let live_here = db
            .get_card(&card.id)?
            .is_some_and(|c| c.deleted_at.is_none());
        if card.deleted_at.is_some() && live_here {
            return Err(format!(
                "this backup deletes \"{}\", and indigo-cli never deletes cards; import it in the \
                 app instead",
                card.title
            )
            .into());
        }
    }
    Ok(())
}

/// Restoring a conflict's losing version that is a deletion would hide a live card.
fn refuse_restoring_a_tombstone(db: &Db, id: &str) -> Res<()> {
    let Some((.., json, _)) = db.conflicts_all()?.into_iter().find(|c| c.0 == id) else {
        return Ok(()); // core reports the missing conflict
    };
    let card: Card = serde_json::from_str(&json)?;
    if card.deleted_at.is_some() {
        return Err(
            "that version is a deletion, and indigo-cli never deletes cards; dismiss it or \
             restore it in the app"
                .into(),
        );
    }
    Ok(())
}
