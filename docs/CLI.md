# indigo-cli

`indigo-cli` lets a script or an AI assistant (Claude Code, ChatGPT, Codex, any harness that can
run a shell command) manage an Indigo library: write cards, edit them, organise them, read
stats, change settings. It is Indigo's own command set as JSON in, JSON out. Nothing runs in the
background: no server, no network port, no AI inside the app. The assistant runs a command, reads
the JSON, and decides what to do next.

**Assistants: read this whole page before running anything else.** Get it any time with
`indigo-cli guide`.

## Rules for assistants

1. **You cannot delete cards, and you must not try.** There is no delete command. A backup import
   or a conflict restore that would remove a card is refused. If the user wants a card gone,
   tell them to delete it in the app (Library → select → Delete).
2. **You cannot sync.** The app syncs by itself (on launch, after each study session, on quit,
   and with Settings → Sync → Sync now). `settings_set` never changes the sync repository.
   Your changes reach other devices on the app's next sync.
3. **You cannot study or grade.** Grades schedule the user's reviews, so only the user gives
   them, in the app. You may read schedules, due dates and stats.
4. Write cards the user would write: one idea per card, a clear prompt, an answer they can check
   themselves against. Do not invent facts; if unsure, say so in the card's notes or ask.
5. Check your work: after saving, list what you made with `cards_query` and report titles and
   counts back to the user, along with any `warnings`.

## Running it

```
indigo-cli [--data-dir DIR] <command> ['<json args>' | -]
indigo-cli [--data-dir DIR] --each <command> < lines.jsonl
indigo-cli guide        # this page
indigo-cli commands     # the command names
```

- **Arguments** are one JSON object, given inline (quote it for the shell) or as `-` to read it
  from stdin. Commands with no arguments need none.
- **Output** is pretty JSON on stdout. Commands that return nothing print `{"ok": true}`.
- **Errors** go to stderr as `{"error": "..."}` with exit code 1. Read the message: it says
  what to fix (for example `Pick a category`, `Give the card a title`).
- **`--each`** reads one JSON arguments object per line from stdin and runs the command for
  each line, printing one compact result per line. A failed line prints
  `{"line": N, "error": "..."}` on stderr and the rest still run; the exit code is 1 if any line
  failed. Use it to add many cards in one call.
- **Data folder:** the installed app's by default (`~/.local/share/app.indigo.desktop` on Linux,
  `~/Library/Application Support/app.indigo.desktop` on macOS,
  `%APPDATA%\app.indigo.desktop` on Windows). `--data-dir` points elsewhere. Indigo must have
  been started once on this machine; the CLI never creates a library.
- The app can stay open. It shows your changes the next time a screen opens (switch to another
  screen and back).

## Shapes

### Category

```json
{ "id": "…", "name": "Python", "syntax": "python", "color": null, "position": 3,
  "modified_at": "…", "deleted_at": null }
```

`syntax` is the code language used to highlight typed answers and code. A new category whose
name is a known language (Python, C, Rust, GDScript, JavaScript, Bash, …) gets it filled in.
To create one, `category_save` needs only `{"name": "..."}`.

### Card

The smallest card you can save:

```json
{ "title": "List comprehension",
  "category_id": "<id from categories_list>",
  "steps": [ { "prompt_md": "Build a list of the squares of 0–9 in one expression.",
               "solution_md": "`[x * x for x in range(10)]`" } ] }
```

All fields, with what they mean. Leave out what you do not need; defaults are shown.

| Field | Default | Meaning |
|---|---|---|
| `id` | `""` | Empty for a new card. Keep the id from `card_get` to edit. |
| `title` | (required) | Short name shown in lists and while studying. |
| `category_id` | (required) | A live category's id. |
| `difficulty` | `"beginner"` | `beginner`, `intermediate`, `advanced`, `expert` or `master`. |
| `tags` | `[]` | Free-form strings, e.g. `["fundamentals", "loops"]`. |
| `est_minutes` | `null` | Expected minutes, for a long card; otherwise measured. |
| `source` | `null` | Where it comes from: a URL, a book and chapter. |
| `steps` | (required) | One step for a normal card; several for a multi-step task. |
| `state` | `"shelved"` | Ignored for a new card (always shelved). See `card_set_state`. |

Fields `pack_id`, `pack_card_id`, `created_at`, `modified_at`, `modified_by` and `deleted_at` are
managed by Indigo; leave them out or pass back what `card_get` returned.

**Step**

| Field | Default | Meaning |
|---|---|---|
| `prompt_md` | (required) | The question, in Markdown. |
| `response` | `"recall"` | `recall`: the user answers in their head (or does the task), then reveals. `typed`: the user types an answer into a code editor and compares it with `solution_text`. |
| `solution_md` | `""` | The answer (recall) or notes shown with the answer (typed), in Markdown. |
| `solution_text` | `null` | For `typed` steps: the reference answer, plain text, highlighted with the category's `syntax`. Keep it under ~15 lines. |
| `criteria` | `[]` | A checklist shown after the reveal, e.g. `[{"text": "handles the empty list"}]`. Reminders only; they never grade. |
| `media` | `[]` | Images, audio or video; see `media_import_path`. |

Markdown supports GitHub-style formatting and fenced code blocks with a language
(```` ```python ````), which are highlighted. Links open in the browser after the user confirms.

A good recall card: a prompt that asks one thing, a `solution_md` that answers it in a few lines.
A good typed card: `response: "typed"`, a prompt that says exactly what to write, the canonical
answer in `solution_text`, optional notes in `solution_md`.

### Where new cards go

New cards land on the **shelf**. Each day the app moves up to "New cards per day" (Settings →
Study, default 20) shelf cards into study, oldest first. To make a card due today, set it
`active` with `card_set_state`. Only do that when the user asks.

## Commands

Arguments are listed as JSON fields. `→` is what comes back.

### Categories

| Command | Arguments | Returns |
|---|---|---|
| `categories_list` | — | The live categories. Saving a new one with a deleted one's name brings that one back. |
| `category_save` | `category`: a Category (only `name` needed for a new one; include `id` to rename or recolour) | The saved category. |
| `category_delete` | `id` | Refused while any card uses the category. Move the cards first. |

### Cards

| Command | Arguments | Returns |
|---|---|---|
| `cards_query` | `filter` (optional): `text` (in titles), `category_id`, `state` (`shelved`/`active`/`suspended`), `due_only`, `flagged_only` | Summaries: `id`, `title`, `category_name`, `difficulty`, `state`, `step_count`, `due_day`, `flagged`, `tags`. |
| `card_get` | `id` | The full card, or `null`. |
| `card_save` | `card` | `{card, warnings}`. `warnings` are hints (e.g. no answer to compare against); fix them when they make sense. |
| `card_warnings` | `card` | The same hints without saving. |
| `card_set_state` | `id`, `card_state`: `shelved`, `active` or `suspended` | `{ok}`. Suspended cards leave daily study until resumed. |
| `tag_rename` | `from`, `to` (onto an existing tag merges them) | Number of cards changed. |
| `tag_delete` | `tag` (removes the tag from cards; the cards stay) | Number of cards changed. |

To **edit** a card: `card_get`, change fields, `card_save` the whole object. To **move** cards
to another category or change their difficulty, do the same per card.

### Media

| Command | Arguments | Returns |
|---|---|---|
| `media_import_path` | `path`: an image (png, jpg, webp, gif), audio (mp3, ogg, wav, flac) or video (mp4, webm) file | `{hash, ext, mime, bytes, …}`. Video and audio are compressed first. |
| `media_cleanup` | — | Removes stored files no card uses. |

To attach: import the file, then add to a step's `media`:
`{"hash": "<hash>", "ext": "<ext>", "mime": "<mime>", "role": "prompt", "caption": null}`
(`role`: `prompt` shows with the question, `solution` with the answer). Then `card_save`.

### Backups and packs

| Command | Arguments | Returns |
|---|---|---|
| `backup_export` | `path` (a `.zip` to write) | Counts. Everything: cards, reviews, media, settings. |
| `backup_import` | `path` | Merge report. Refused if the backup would delete a card here. |
| `pack_export` | `path` (`.indigopack`), `card_ids`, `name` | Cards written. Packs share cards without history. |
| `pack_inspect` | `path` | What an import would add or update. |
| `pack_import` | `path`, `update` (optional: pack card ids to overwrite) | Report. New cards go to the shelf. |

### Settings and scheduling

| Command | Arguments | Returns |
|---|---|---|
| `settings_get` | — | Device settings: `daily_minutes`, `new_per_day`, `theme`, `editor`, `media_limits`, `tags`, `saved_filters`, … |
| `settings_set` | `settings`: the whole object from `settings_get`, changed | `{ok}`. The sync repository is always kept as it is. |
| `sched_config_get` | — | Scheduling constants, e.g. `desired_retention` (0.40–0.99, default 0.90), `max_interval_days`. |
| `sched_config_set` | `config`: the whole object, changed | `{ok}`. Recomputes every card's due date from its history. Only when the user asks. |
| `sched_preview` | `retention` | Intervals in days after successive good reviews at that retention. |

### Stats and sync status (read only)

| Command | Arguments | Returns |
|---|---|---|
| `stats_overview` | — | Study time, streaks, review counts, forecast, weak steps, per-category figures. |
| `sync_status` | — | Whether sync is set up, and the last result. |
| `conflicts_list` | — | Card versions that lost a sync merge. |
| `conflict_restore` | `id` | Brings that version back. Refused when the version is a deletion. |
| `conflict_dismiss` | `id` | Forgets the losing version. |

## Recipe: "make 20 Python cards on the fundamentals"

1. `indigo-cli categories_list`. Use the live `Python` category, or create it:
   `indigo-cli category_save '{"category": {"name": "Python"}}'`.
2. `indigo-cli cards_query '{"filter": {"category_id": "<id>"}}'` to see what exists, so you do
   not duplicate cards.
3. Write one JSON line per card to a file, each `{"card": {...}}`, for example:

   ```
   {"card": {"title": "Squares in one line", "category_id": "<id>", "tags": ["fundamentals"], "steps": [{"prompt_md": "Build a list of the squares of 0–9 in one expression.", "response": "typed", "solution_text": "[x * x for x in range(10)]"}]}}
   {"card": {"title": "Default in dict lookup", "category_id": "<id>", "tags": ["fundamentals"], "steps": [{"prompt_md": "Read key `k` from dict `d`, falling back to 0 when it is missing.", "solution_md": "`d.get(k, 0)`"}]}}
   ```

4. `indigo-cli --each card_save < cards.jsonl`. Fix and re-run any line reported on stderr (only
   those lines; the others are saved).
5. `indigo-cli cards_query '{"filter": {"category_id": "<id>", "state": "shelved"}}'` and tell
   the user what was added, and that the cards enter study through the shelf.

## Exit codes

`0` success; `1` an error (stderr has `{"error": …}`), an unknown or excluded command, or a
failed `--each` line. `2` the command line itself was malformed.
