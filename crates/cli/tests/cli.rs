//! Runs the real binary against throwaway profiles: the harness flow, and every guard that keeps
//! cards from disappearing.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use chrono::Utc;
use indigo_core::db::Db;
use indigo_core::settings::KEY_SETTINGS;
use serde_json::{Value, json};

fn cli(dir: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_indigo-cli"))
        .arg("--data-dir")
        .arg(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.unwrap_or("").as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// Runs a command that must succeed and returns its JSON.
fn ok(dir: &Path, cmd: &str, args: Value) -> Value {
    let o = cli(dir, &[cmd, &args.to_string()], None);
    assert!(
        o.status.success(),
        "{cmd}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}

fn refused(dir: &Path, cmd: &str, args: Value) -> String {
    let o = cli(dir, &[cmd, &args.to_string()], None);
    assert!(!o.status.success(), "{cmd} should be refused");
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// A profile as the app leaves it after its first launch, with one category and one card.
fn profile() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    Db::open(&dir.path().join("indigo.sqlite"))
        .unwrap()
        .device_id()
        .unwrap();
    let cat = ok(
        dir.path(),
        "category_save",
        json!({ "category": { "name": "Python" } }),
    );
    assert_eq!(cat["syntax"], "python", "known language filled in");
    let saved = ok(
        dir.path(),
        "card_save",
        json!({ "card": { "title": "Squares", "category_id": cat["id"],
                          "steps": [{ "prompt_md": "Squares of 0..9", "solution_md": "`[x*x for x in range(10)]`" }] } }),
    );
    (dir, saved["card"]["id"].as_str().unwrap().to_string())
}

fn live(dir: &Path, id: &str) -> bool {
    !ok(dir, "card_get", json!({ "id": id })).is_null()
}

#[test]
fn harness_flow_adds_cards_to_the_shelf() {
    let (dir, id) = profile();
    let card = ok(dir.path(), "card_get", json!({ "id": id }));
    assert_eq!(card["state"], "shelved");
    assert_eq!(card["steps"][0]["response"], "recall");

    let cat = card["category_id"].as_str().unwrap();
    let lines = format!(
        "{}\n{}\n",
        json!({ "card": { "title": "Dict get", "category_id": cat, "steps": [{ "prompt_md": "Lookup with a default",
                "response": "typed", "solution_text": "d.get(k, 0)", "criteria": [{ "text": "uses get" }] }] } }),
        json!({ "card": { "title": "", "category_id": cat, "steps": [{ "prompt_md": "no title" }] } })
    );
    let o = cli(dir.path(), &["--each", "card_save"], Some(&lines));
    assert!(!o.status.success(), "one line failed");
    assert_eq!(String::from_utf8_lossy(&o.stdout).lines().count(), 1);
    assert!(String::from_utf8_lossy(&o.stderr).contains("\"line\":2"));

    let found = ok(
        dir.path(),
        "cards_query",
        json!({ "filter": { "text": "dict" } }),
    );
    assert_eq!(found.as_array().unwrap().len(), 1);
}

#[test]
fn deleting_syncing_and_studying_are_not_commands() {
    let (dir, id) = profile();
    for cmd in [
        "card_delete",
        "sync_now",
        "attempt_start",
        "attempt_finish",
        "session_build",
    ] {
        assert!(refused(dir.path(), cmd, json!({ "id": id })).contains("unknown command"));
    }
    assert!(live(dir.path(), &id));
}

#[test]
fn a_category_with_cards_cannot_be_deleted() {
    let (dir, id) = profile();
    let card = ok(dir.path(), "card_get", json!({ "id": id }));
    refused(
        dir.path(),
        "category_delete",
        json!({ "id": card["category_id"] }),
    );
    assert!(live(dir.path(), &id));
}

#[test]
fn a_backup_that_deletes_a_card_is_refused() {
    let (a, id) = profile();
    let b = tempfile::tempdir().unwrap();
    Db::open(&b.path().join("indigo.sqlite"))
        .unwrap()
        .device_id()
        .unwrap();
    let zip = a.path().join("one.zip");
    ok(a.path(), "backup_export", json!({ "path": zip }));
    ok(b.path(), "backup_import", json!({ "path": zip }));
    assert!(live(b.path(), &id));

    // The card is deleted on A; A's next backup carries the tombstone.
    Db::open(&a.path().join("indigo.sqlite"))
        .unwrap()
        .delete_card(&id, Utc::now())
        .unwrap();
    let zip2 = a.path().join("two.zip");
    ok(a.path(), "backup_export", json!({ "path": zip2 }));
    assert!(refused(b.path(), "backup_import", json!({ "path": zip2 })).contains("never deletes"));
    assert!(live(b.path(), &id));
}

#[test]
fn restoring_a_deleted_version_is_refused() {
    let (dir, id) = profile();
    let mut card = ok(dir.path(), "card_get", json!({ "id": id }));
    card["deleted_at"] = json!(Utc::now());
    Db::open(&dir.path().join("indigo.sqlite"))
        .unwrap()
        .insert_conflict("card", &id, &card.to_string(), "other-device", Utc::now())
        .unwrap();
    let conflicts = ok(dir.path(), "conflicts_list", json!({}));
    let cid = conflicts[0]["id"].clone();
    assert!(
        refused(dir.path(), "conflict_restore", json!({ "id": cid })).contains("never deletes")
    );
    assert!(live(dir.path(), &id));
}

#[test]
fn settings_set_keeps_the_sync_repository() {
    let (dir, _) = profile();
    let db = Db::open(&dir.path().join("indigo.sqlite")).unwrap();
    let mut s = db.settings().unwrap();
    s.sync.repo_url = "https://example.com/vault.git".into();
    db.setting_set(KEY_SETTINGS, &s).unwrap();

    let mut s = ok(dir.path(), "settings_get", json!({}));
    s["daily_minutes"] = json!(45);
    s["sync"]["repo_url"] = json!("");
    ok(dir.path(), "settings_set", json!({ "settings": s }));
    let s = ok(dir.path(), "settings_get", json!({}));
    assert_eq!(s["daily_minutes"], 45);
    assert_eq!(s["sync"]["repo_url"], "https://example.com/vault.git");
}

#[test]
fn no_library_no_profile_created() {
    let dir = tempfile::tempdir().unwrap();
    assert!(refused(dir.path(), "settings_get", json!({})).contains("start Indigo once first"));
    assert!(!dir.path().join("indigo.sqlite").exists());
}
