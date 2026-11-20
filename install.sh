#!/usr/bin/env bash
# Builds and installs Indigo for the current user (SPEC §17). Re-run to update.
# Never touches your library: that lives in ~/.local/share/app.indigo.desktop.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/common.sh
. "$REPO/scripts/lib/common.sh"
# shellcheck source=scripts/lib/deps.sh
. "$REPO/scripts/lib/deps.sh"
# shellcheck source=scripts/lib/desktop.sh
. "$REPO/scripts/lib/desktop.sh"

SKIP_DEPS=0
NO_SYNC=0
NO_PULL=0
ARGS=("$@")

usage() {
	cat <<USAGE
Usage: ./install.sh [options]

Builds Indigo from this folder and installs it for your user.

  --yes          answer yes to every question (skips sync setup and the Desktop shortcut)
  --skip-deps    do not install system packages (you installed them yourself)
  --no-sync      skip the optional sync setup
  --no-pull      do not update this folder with git pull first
  --prefix DIR   install under DIR instead of ~/.local
  --help         show this help
USAGE
}

while [ $# -gt 0 ]; do
	case "$1" in
	--yes | -y) ASSUME_YES=1 ;;
	--skip-deps) SKIP_DEPS=1 ;;
	--no-sync) NO_SYNC=1 ;;
	--no-pull) NO_PULL=1 ;;
	--prefix)
		[ $# -ge 2 ] || die "--prefix needs a directory"
		PREFIX="$2"
		shift
		;;
	--help | -h)
		usage
		exit 0
		;;
	*) die "unknown option $1 (see --help)" ;;
	esac
	shift
done
mkdir -p "$PREFIX"
PREFIX="$(cd "$PREFIX" && pwd)"

[ "$(uname -s)" = Linux ] || die "install.sh supports Linux only"

# Update the source first. A pull that changed anything restarts this script, so the new
# install.sh runs rather than a half-read old one.
if [ "$NO_PULL" -eq 0 ]; then
	update_source && exec "$REPO/install.sh" "${ARGS[@]}" --no-pull
fi
OLD_COMMIT="$(cat "$(install_dir)/installed-commit" 2>/dev/null || true)"
NEW_COMMIT="$(git -C "$REPO" rev-parse --short HEAD 2>/dev/null || echo unknown)"

if [ "$SKIP_DEPS" -eq 0 ]; then
	install_packages
else
	say "Skipping system packages (--skip-deps)"
fi
need curl
ensure_rust
ensure_node

say "Building Indigo (this takes a few minutes the first time)"
(
	cd "$REPO"
	npm ci --no-audit --no-fund
	npm run tauri build -- --no-bundle
	cargo build --release -p indigo-cli
)
[ -x "$REPO/target/release/indigo" ] || die "build finished without target/release/indigo"
[ -x "$REPO/target/release/indigo-cli" ] || die "build finished without target/release/indigo-cli"

SHORTCUT=0
if [ "$ASSUME_YES" -eq 0 ] && confirm "Add an Indigo shortcut to your Desktop?"; then
	SHORTCUT=1
fi
WAS_RUNNING=0
if [ -n "$(running_pids)" ]; then
	WAS_RUNNING=1
	confirm "Indigo is running. Close it now (it syncs first) to finish the update?" ||
		die "close Indigo and run ./install.sh again"
	close_running
fi
say "Installing into $PREFIX"
install_desktop "$REPO" "$SHORTCUT"
printf '%s\n' "$NEW_COMMIT" >"$(install_dir)/installed-commit"

# Optional sync (SPEC §11.4): a private GitHub repository. git signs in with the GitHub CLI
# or an SSH key; Indigo gets only the repository URL.
if [ "$NO_SYNC" -eq 0 ] && [ "$ASSUME_YES" -eq 0 ] && [ -t 0 ] &&
	confirm "Set up sync through a private GitHub repository now?"; then
	setup_github_sync
fi

manifest_add "dir $(install_dir)"
if [ -n "$OLD_COMMIT" ] && [ "$OLD_COMMIT" != "$NEW_COMMIT" ]; then
	say "Updated Indigo: $OLD_COMMIT → $NEW_COMMIT"
elif [ -n "$OLD_COMMIT" ]; then
	say "Reinstalled Indigo $NEW_COMMIT (no new commits)"
else
	say "Installed Indigo $NEW_COMMIT"
fi
if [ "$WAS_RUNNING" -eq 1 ] && confirm "Start Indigo again?"; then
	setsid "$PREFIX/bin/indigo" >/dev/null 2>&1 </dev/null &
fi
say "Done. Start Indigo from your applications menu or with: $PREFIX/bin/indigo"
