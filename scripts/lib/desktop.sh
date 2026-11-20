# shellcheck shell=bash
# Binary, icons, desktop entry, optional Desktop shortcut (SPEC §17 step 5).

ICON_SIZES="32x32 64x64 128x128 256x256"

icon_source() {
	case "$1" in
	32x32) echo 32x32.png ;;
	64x64) echo 64x64.png ;;
	128x128) echo 128x128.png ;;
	256x256) echo 128x128@2x.png ;;
	esac
}

install_file() {
	# install_file MODE SRC DEST: copies and records DEST.
	mkdir -p "$(dirname "$3")"
	install -m "$1" "$2" "$3"
	manifest_add "file $3"
}

desktop_dir() {
	if command -v xdg-user-dir >/dev/null 2>&1; then
		xdg-user-dir DESKTOP
	else
		printf '%s/Desktop' "$HOME"
	fi
}

install_desktop() {
	local repo="$1" shortcut="$2" size apps entry
	install_file 755 "$repo/target/release/indigo" "$PREFIX/bin/indigo"
	for size in $ICON_SIZES; do
		install_file 644 "$repo/src-tauri/icons/$(icon_source "$size")" \
			"$PREFIX/share/icons/hicolor/$size/apps/indigo.png"
	done
	apps="$PREFIX/share/applications"
	entry="$(mktemp)"
	cat >"$entry" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Indigo
Comment=Spaced-repetition practice for skills you use in real tools
Exec=$PREFIX/bin/indigo
Icon=indigo
Terminal=false
Categories=Education;Development;
StartupWMClass=indigo
DESKTOP
	install_file 644 "$entry" "$apps/indigo.desktop"
	if [ "$shortcut" -eq 1 ]; then
		local desk
		desk="$(desktop_dir)"
		if [ -d "$desk" ]; then
			install_file 755 "$entry" "$desk/indigo.desktop"
			if command -v gio >/dev/null 2>&1; then
				gio set "$desk/indigo.desktop" metadata::trusted true 2>/dev/null || true
			fi
		fi
	fi
	rm -f "$entry"
	refresh_caches
	case ":$PATH:" in
	*":$PREFIX/bin:"*) ;;
	*) warn "$PREFIX/bin is not on your PATH; the menu entry works, but to run 'indigo' from a terminal add it to PATH" ;;
	esac
}

refresh_caches() {
	if command -v update-desktop-database >/dev/null 2>&1; then
		update-desktop-database -q "$PREFIX/share/applications" 2>/dev/null || true
	fi
	if command -v gtk-update-icon-cache >/dev/null 2>&1; then
		gtk-update-icon-cache -q -f -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
	fi
}

# PIDs of this user's running installed Indigo (matched by executable, so a dev build from
# the source folder is left alone).
running_pids() {
	local p exe
	for p in $(pgrep -u "$(id -u)" -x indigo 2>/dev/null); do
		exe="$(readlink "/proc/$p/exe" 2>/dev/null || true)"
		[ "${exe% (deleted)}" = "$PREFIX/bin/indigo" ] && printf '%s\n' "$p"
	done
	return 0
}

# Asks the running Indigo to sync and quit (`indigo --quit`), waits, and offers to force it.
close_running() {
	local waited=0
	say "Closing Indigo (it syncs first)"
	"$PREFIX/bin/indigo" --quit >/dev/null 2>&1 || true
	while [ -n "$(running_pids)" ] && [ "$waited" -lt "${QUIT_TIMEOUT:-60}" ]; do
		sleep 1
		waited=$((waited + 1))
	done
	[ -z "$(running_pids)" ] && return 0
	confirm "Indigo has not closed after ${waited}s. Force it to quit? (an unfinished sync runs again next start)" ||
		die "close Indigo and run ./install.sh again"
	# shellcheck disable=SC2046 # one PID per word
	kill $(running_pids) 2>/dev/null || true
	sleep 2
	[ -z "$(running_pids)" ] || die "Indigo is still running; close it and run ./install.sh again"
}

# Creates (with the GitHub CLI) or asks for the private sync repository, checks git can reach
# it, and leaves its URL in sync-setup.json for Indigo to adopt on its next start.
setup_github_sync() {
	local url="" name
	if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
		read -r -p "Name of the private repository for Indigo [indigo-vault]: " name
		name="${name:-indigo-vault}"
		if gh repo view "$name" >/dev/null 2>&1; then
			say "Using your existing repository $name"
		elif gh repo create "$name" --private --description "Indigo sync" >/dev/null; then
			say "Created the private repository $name"
		else
			warn "could not create $name; create it at https://github.com/new and set it in Indigo → Settings → Sync"
			return 0
		fi
		url="$(gh repo view "$name" --json url --jq .url 2>/dev/null || true)"
		[ -z "$url" ] || url="$url.git"
		if confirm "Let git use your GitHub CLI sign-in (runs 'gh auth setup-git')?"; then
			gh auth setup-git || warn "gh auth setup-git failed; run it yourself before syncing"
		fi
	else
		cat <<GITHUB

Create an empty private repository at https://github.com/new (for example indigo-vault),
then sign git in to GitHub once: either 'gh auth login' and 'gh auth setup-git', or an SSH
key added to your GitHub account. docs/SYNC.md has the details.

GITHUB
		read -r -p "Repository URL (blank to skip): " url
	fi
	[ -n "$url" ] || return 0
	case "$url" in
	-* | *[[:space:]\\\"]*)
		warn "that URL cannot be used; set it later in Indigo → Settings → Sync"
		return 0
		;;
	esac
	if GIT_TERMINAL_PROMPT=0 git ls-remote "$url" >/dev/null 2>&1; then
		local data
		data="$(data_dir)"
		mkdir -p "$data"
		printf '{"repo_url":"%s"}\n' "$url" >"$data/sync-setup.json"
		manifest_add "file $data/sync-setup.json"
		say "Sync set to $url; Indigo picks it up on its next start"
	else
		warn "git could not reach $url (sign-in missing?); set it later in Indigo → Settings → Sync"
	fi
}
