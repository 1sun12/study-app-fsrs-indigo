# shellcheck shell=bash
# Shared helpers for install.sh and uninstall.sh: output, prompts, root, manifest, versions.

INDIGO_ID="app.indigo.desktop"
PREFIX="${PREFIX:-$HOME/.local}"
ASSUME_YES=0

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die() {
	printf '\033[1;31merror:\033[0m %s\n' "$*" >&2
	exit 1
}

# confirm "Question?" → 0 for yes. --yes answers yes; no terminal answers no.
confirm() {
	if [ "$ASSUME_YES" -eq 1 ]; then return 0; fi
	if [ ! -t 0 ]; then return 1; fi
	local reply
	read -r -p "$1 [y/N] " reply
	[[ "$reply" =~ ^[Yy] ]]
}

# Runs a command as root: directly when already root, else through sudo.
as_root() {
	if [ "$(id -u)" -eq 0 ]; then
		"$@"
	elif command -v sudo >/dev/null 2>&1; then
		sudo "$@"
	else
		die "need root for: $* (install sudo or run as root)"
	fi
}

need() { command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"; }

# version_ge 1.76.0 1.70 → true when $1 >= $2.
version_ge() {
	[ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" = "$2" ]
}

data_dir() { printf '%s/%s' "${XDG_DATA_HOME:-$HOME/.local/share}" "$INDIGO_ID"; }
install_dir() { printf '%s/share/indigo-install' "$PREFIX"; }
manifest() { printf '%s/manifest' "$(install_dir)"; }

# Manifest lines: "file PATH", "dir PATH", "pkg MANAGER NAME", "tool NAME PATH".
manifest_add() {
	mkdir -p "$(install_dir)"
	local m
	m="$(manifest)"
	touch "$m"
	grep -qxF -- "$*" "$m" || printf '%s\n' "$*" >>"$m"
}

# Verifies FILE against a SHA-256 list that names it (sha256sum format).
check_sha256() {
	local file="$1" sums="$2" name expected actual
	name="$(basename "$file")"
	expected="$(awk -v n="$name" '{ f = $2; sub(/^\*/, "", f); sub(/^.*\//, "", f) } f == n { print $1; exit }' "$sums")"
	[ -n "$expected" ] || die "no checksum published for $name"
	actual="$(sha256sum "$file" | awk '{print $1}')"
	[ "$expected" = "$actual" ] || die "checksum mismatch for $name"
}

# git pull --ff-only in the source folder when it tracks an upstream and has no local
# changes. Returns 0 only when new commits arrived.
update_source() {
	local before after
	git -C "$REPO" rev-parse --is-inside-work-tree >/dev/null 2>&1 || return 1
	if ! git -C "$REPO" rev-parse --abbrev-ref '@{upstream}' >/dev/null 2>&1; then
		say "Not updating the source: this folder tracks no remote branch"
		return 1
	fi
	if [ -n "$(git -C "$REPO" status --porcelain --untracked-files=no)" ]; then
		warn "Not updating the source: this folder has local changes"
		return 1
	fi
	before="$(git -C "$REPO" rev-parse HEAD)"
	say "Updating the source (git pull --ff-only)"
	git -C "$REPO" pull --ff-only || {
		warn "git pull failed; installing what is here"
		return 1
	}
	after="$(git -C "$REPO" rev-parse HEAD)"
	[ "$before" != "$after" ]
}
