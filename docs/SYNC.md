# Sync

Indigo keeps two computers in step through a **private GitHub repository**. Your database never leaves the computer: Indigo writes a small folder of files (the *vault*: one file per card, your reviews, media) and pushes it with `git`. Indigo never sees a password or token. git signs in with the GitHub CLI or your SSH key, and Indigo stores only the repository's URL.

## What syncs

Cards, categories, scheduling settings, every review, and media. Device-only: Settings, half-finished study sessions, backups, and the conflict list.

Sync runs when Indigo starts, after each study summary, when you click **Sync now**, and when you close the window (the window hides, the sync finishes, then Indigo exits). Studying never waits for it. The dot next to **+ Add card** shows the state: green synced, blue syncing, grey not yet, amber needs attention (hover for the reason).

## One-time setup

The installer offers to do steps 1–2 for you (`./install.sh`, answer yes to sync setup).

1. **Create an empty private repository** on GitHub, for example `indigo-vault`: at https://github.com/new, or with the GitHub CLI:

   ```bash
   gh repo create indigo-vault --private
   ```

2. **Sign git in to GitHub once.** With the GitHub CLI (simplest):

   ```bash
   gh auth login
   gh auth setup-git
   ```

   `gh auth setup-git` tells git to use the GitHub CLI's stored sign-in, so syncs never ask for a password. An SSH key added to your GitHub account works as well; then use the `git@github.com:…` form of the URL.

3. In Indigo, open **Settings → Sync**, paste the repository URL (`https://github.com/<you>/indigo-vault.git`), and click **Sync now**.

4. On the second computer, install Indigo, sign git in the same way, and paste the same URL. A computer that has never synced joins the library already in the repository; its own cards are merged in, nothing is deleted.

## Video and audio

A git repository keeps every file forever, so Indigo makes video and audio small **when you attach them**: video becomes WebM (VP9, at most 1280 px on the long side, which is 720p for widescreen) and audio becomes Opus. The smaller file is the one Indigo keeps and syncs; your original file on disk is untouched. Videos you added before this existed are converted once, before the next sync. Compressing a long video takes a while; the Attach button shows "Adding…" until it is done. This needs `ffmpeg` (the installer adds it); without it files are stored as they are.

GitHub refuses single files of 100 MB or more, so Indigo refuses to store anything over 95 MB even after compression. Trim very long recordings first.

## When both computers change the same card

The newer edit wins on both computers. The other version is kept under **Settings → Sync → Conflicts**: **Restore** brings it back (it then wins everywhere on the next sync), **Dismiss** forgets it. Edit times come from each computer's clock, so keep clocks set automatically.

Git itself never merges file contents: Indigo merges cards as records, then commits the result with both computers' histories as parents. So there are never git merge conflicts to resolve by hand, and every past version stays in the repository's history.

## If something goes wrong

- **"GitHub sign-in failed"**: run `gh auth login` and then `gh auth setup-git` (or check your SSH key), then **Sync now**.
- **"Repository not found"**: check the URL in Settings → Sync, and that the account git signs in with can see it.
- **"The repository holds a different Indigo library"**: this computer already syncs with another repository. Use the right URL.
- **"The repository kept changing"**: the other computer was syncing at the same moment; Indigo tries again at the next trigger.
- The vault is an ordinary git repository in Indigo's data folder (`~/.local/share/app.indigo.desktop/vault` on Linux). Indigo is its only writer; do not commit to it by hand.
- The real safety net is local: Indigo backs up its database before every merge (`backups/`, newest ten kept), and **Settings → Data → Export backup** writes a full copy you can keep anywhere.
