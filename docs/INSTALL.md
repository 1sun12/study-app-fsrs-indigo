# Installing Indigo

Indigo is built from source on your machine by `install.sh`. Linux only for now (Debian, Ubuntu, Pop!_OS, Mint, Arch and its relatives, Fedora, Void).

## Install or update

```bash
git clone https://github.com/1sun12/study-app-fsrs-indigo.git indigo && cd indigo
./install.sh
```

The script:

1. installs the system packages Indigo needs (Tauri's WebKitGTK prerequisites, GStreamer with the H.264/AAC decoder for video, ffmpeg (compresses attached video and audio), git, curl, unzip), asking for your password through `sudo`. Only packages you did not already have are recorded.
2. installs Rust (rustup, without editing your shell profile) and a private copy of Node.js if yours is missing or older than 22.17, Every download is checked against its published SHA-256.
3. builds Indigo (a few minutes the first time),
4. installs `~/.local/bin/indigo`, the icon and the applications-menu entry, and offers a Desktop shortcut,
5. optionally sets up sync: with the GitHub CLI signed in it creates (or reuses) a private repository and offers `gh auth setup-git`; otherwise it asks for the repository URL (see [SYNC.md](SYNC.md)).

Run it again at any time to update; it never touches your library. An update:

- runs `git pull --ff-only` first when this folder tracks a remote branch and has no local changes (otherwise it says why and installs what is there; `--no-pull` skips this),
- closes a running Indigo after asking (Indigo syncs, then exits; after a minute it offers to force it),
- prints the installed commit before → after, and offers to start Indigo again.

Options: `--yes` (no questions; skips sync setup and the shortcut), `--skip-deps` (you installed the system packages yourself), `--no-sync`, `--no-pull`, `--prefix DIR` (instead of `~/.local`), `--help`.

Everything the installer creates is listed in `~/.local/share/indigo-install/manifest`.

## Your data

Your library (cards, reviews, media, settings, automatic database backups) lives in `~/.local/share/app.indigo.desktop/`. **Settings → Data → Export backup** writes all of it to one zip.

## Uninstall

```bash
./uninstall.sh
```

This removes every file in the manifest. It deletes your library only if sync is on, the last sync succeeded, and nothing is waiting to upload; otherwise it stops and explains. Options: `--keep-data`, `--force` (delete the library anyway), `--purge-deps` (also remove the packages and tools the installer added, after listing them and asking), `--yes`, `--prefix DIR`. It never touches your GitHub repository.
