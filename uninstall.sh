#!/usr/bin/env bash
# Removes what install.sh installed, as listed in its manifest (SPEC §17). Your library is
# removed only when it is safely synced (or with --force); --keep-data always keeps it.
# Never touches the sync repository on GitHub.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/common.sh
. "$REPO/scripts/lib/common.sh"
# shellcheck source=scripts/lib/deps.sh
. "$REPO/scripts/lib/deps.sh"
# shellcheck source=scripts/lib/desktop.sh
. "$REPO/scripts/lib/desktop.sh"

KEEP_DATA=0
FORCE=0
PURGE_DEPS=0

usage() {
	cat <<USAGE
Usage: ./uninstall.sh [options]

  --keep-data    keep your library (cards, reviews, media, settings)
  --force        delete your library even if it is not synced
  --purge-deps   also remove the packages and tools the installer added (asks first)
  --yes          answer yes to every question
  --prefix DIR   the --prefix used when installing
  --help         show this help
USAGE
}

while [ $# -gt 0 ]; do
	case "$1" in
	--keep-data) KEEP_DATA=1 ;;
	--force) FORCE=1 ;;
	--purge-deps) PURGE_DEPS=1 ;;
	--yes | -y) ASSUME_YES=1 ;;
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
[ -d "$PREFIX" ] && PREFIX="$(cd "$PREFIX" && pwd)"
MANIFEST="$(manifest)"
[ -f "$MANIFEST" ] || die "no install manifest at $MANIFEST (was Indigo installed with --prefix?)"

# Decide about user data first, while the binary can still be asked about sync.
DATA="$(data_dir)"
REMOVE_DATA=0
if [ "$KEEP_DATA" -eq 0 ] && [ -d "$DATA" ]; then
	if [ "$FORCE" -eq 1 ]; then
		REMOVE_DATA=1
	else
		state=""
		[ -x "$PREFIX/bin/indigo" ] && state="$("$PREFIX/bin/indigo" --sync-state --data-dir "$DATA" 2>/dev/null || true)"
		if printf '%s' "$state" | grep -q '"configured":true' &&
			printf '%s' "$state" | grep -q '"failed":false' &&
			printf '%s' "$state" | grep -q '"pending":0' &&
			! printf '%s' "$state" | grep -q '"last_success_at":null'; then
			REMOVE_DATA=1
		else
			cat >&2 <<REFUSE
Your library in $DATA is not safely synced (sync is off, failed, or has changes not yet
uploaded), so deleting it would lose data. Choose one:
  ./uninstall.sh --keep-data     remove the app, keep the library
  open Indigo, export a backup or sync, then run this again
  ./uninstall.sh --force         remove the library anyway
REFUSE
			exit 1
		fi
	fi
	if [ "$REMOVE_DATA" -eq 1 ] && ! confirm "Delete your Indigo library in $DATA?"; then
		REMOVE_DATA=0
		say "Keeping your library"
	fi
fi

say "Removing installed files"
while read -r kind path; do
	if [ "$kind" = file ]; then rm -f -- "$path"; fi
done <"$MANIFEST"
refresh_caches

if [ "$PURGE_DEPS" -eq 1 ]; then
	declare -A by_manager=()
	tools=()
	while read -r kind a b; do
		case "$kind" in
		pkg) by_manager[$a]="${by_manager[$a]:-} $b" ;;
		tool) tools+=("$a:$b") ;;
		esac
	done <"$MANIFEST"
	if [ "${#by_manager[@]}" -gt 0 ] || [ "${#tools[@]}" -gt 0 ]; then
		echo "The installer added:"
		for m in "${!by_manager[@]}"; do echo "  packages ($m):${by_manager[$m]}"; done
		for t in "${tools[@]}"; do echo "  tool ${t%%:*} at ${t#*:}"; done
		if confirm "Remove all of these?"; then
			for m in "${!by_manager[@]}"; do
				# shellcheck disable=SC2086 # word splitting of the package list is intended
				remove_packages "$m" ${by_manager[$m]}
			done
			for t in "${tools[@]}"; do
				case "${t%%:*}" in
				rustup) command -v rustup >/dev/null 2>&1 && rustup self uninstall -y ;;
				*) rm -f -- "${t#*:}" ;;
				esac
			done
		fi
	else
		say "The installer added no packages or tools"
	fi
fi

if [ "$REMOVE_DATA" -eq 1 ]; then
	rm -rf -- "$DATA"
	say "Library deleted"
fi
rm -rf -- "$(install_dir)"
say "Indigo is uninstalled"
