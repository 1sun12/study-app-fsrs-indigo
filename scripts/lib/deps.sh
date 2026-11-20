# shellcheck shell=bash
# System packages and build tools (SPEC §17 steps 1–3). Only what was missing is installed,
# and only that is recorded, so --purge-deps never removes something the user already had.

MIN_NODE="22.17.0"
MIN_RUST="1.90.0"
DISTRO_FAMILY=""

detect_distro() {
	local id="" like=""
	if [ -r /etc/os-release ]; then
		# shellcheck disable=SC1091
		id="$(. /etc/os-release && printf '%s' "${ID:-}")"
		# shellcheck disable=SC1091
		like="$(. /etc/os-release && printf '%s' "${ID_LIKE:-}")"
	fi
	case " $id $like " in
	*" debian "* | *" ubuntu "*) DISTRO_FAMILY=apt ;;
	*" arch "*) DISTRO_FAMILY=pacman ;;
	*" fedora "* | *" rhel "*) DISTRO_FAMILY=dnf ;;
	*" void "*) DISTRO_FAMILY=xbps ;;
	*) DISTRO_FAMILY="" ;;
	esac
}

# Tauri 2 prerequisites (v2.tauri.app/start/prerequisites, checked 2026-11-12), GStreamer
# for media (libav = H.264/AAC), ffmpeg (compresses attached video/audio), git (sync), and
# the tools this script needs.
packages() {
	case "$DISTRO_FAMILY" in
	apt) echo libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev \
		libayatana-appindicator3-dev librsvg2-dev gstreamer1.0-plugins-base \
		gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav ffmpeg git unzip \
		xz-utils ca-certificates desktop-file-utils ;;
	pacman) echo webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module \
		libappindicator-gtk3 librsvg xdotool gst-plugins-base gst-plugins-good gst-plugins-bad \
		gst-libav ffmpeg git unzip xz desktop-file-utils ;;
	dnf) echo webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel \
		librsvg2-devel libxdo-devel gcc gcc-c++ make gstreamer1-plugins-base \
		gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-plugin-libav ffmpeg-free git unzip \
		xz desktop-file-utils ;;
	# Void is not on the Tauri page; names checked in void-packages, proven by the CI container.
	xbps) echo libwebkit2gtk41-devel base-devel curl wget file openssl-devel \
		libayatana-appindicator-devel librsvg-devel xdotool-devel gst-plugins-base1 \
		gst-plugins-good1 gst-plugins-bad1 gst-libav ffmpeg git unzip xz desktop-file-utils ;;
	esac
}

pkg_installed() {
	case "$DISTRO_FAMILY" in
	apt) dpkg-query -W -f='${Status}' "$1" 2>/dev/null | grep -q "install ok installed" ;;
	pacman) pacman -Q "$1" >/dev/null 2>&1 || pacman -Qg "$1" >/dev/null 2>&1 ;;
	dnf) rpm -q "$1" >/dev/null 2>&1 ;;
	xbps) xbps-query "$1" >/dev/null 2>&1 ;;
	esac
}

install_packages() {
	detect_distro
	if [ -z "$DISTRO_FAMILY" ]; then
		say "Unknown distribution. Install these (or their equivalents), then re-run with --skip-deps:"
		echo "  Tauri 2 prerequisites (v2.tauri.app/start/prerequisites), GStreamer base/good/bad/libav, ffmpeg, git, curl, unzip, xz"
		exit 1
	fi
	local missing=() p
	for p in $(packages); do
		pkg_installed "$p" || missing+=("$p")
	done
	if [ "${#missing[@]}" -eq 0 ]; then
		say "System packages already present"
		return
	fi
	say "Installing system packages: ${missing[*]}"
	case "$DISTRO_FAMILY" in
	apt)
		as_root apt-get update
		as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y "${missing[@]}"
		;;
	pacman) as_root pacman -Sy --needed --noconfirm "${missing[@]}" ;;
	dnf) as_root dnf install -y "${missing[@]}" ;;
	xbps) as_root xbps-install -Sy "${missing[@]}" ;;
	esac
	for p in "${missing[@]}"; do manifest_add "pkg $DISTRO_FAMILY $p"; done
}

remove_packages() {
	local family="$1"
	shift
	case "$family" in
	apt) as_root apt-get remove -y "$@" ;;
	pacman) as_root pacman -R --noconfirm "$@" ;;
	dnf) as_root dnf remove -y "$@" ;;
	xbps) as_root xbps-remove -y "$@" ;;
	*) warn "unknown package manager $family; remove by hand: $*" ;;
	esac
}

arch_suffix() {
	case "$(uname -m)" in
	x86_64) echo amd64 ;;
	aarch64 | arm64) echo arm64 ;;
	*) die "unsupported CPU $(uname -m)" ;;
	esac
}

ensure_rust() {
	# shellcheck disable=SC1091
	[ -r "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
	if command -v cargo >/dev/null 2>&1; then
		local v
		v="$(rustc --version | awk '{print $2}')"
		if version_ge "$v" "$MIN_RUST"; then return; fi
		command -v rustup >/dev/null 2>&1 || die "Rust $v is older than $MIN_RUST; update it"
		say "Updating Rust ($v → stable)"
		rustup update stable
		return
	fi
	say "Installing Rust (rustup, minimal profile; your shell profile is not modified)"
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
		sh -s -- -y --profile minimal --no-modify-path
	# shellcheck disable=SC1091
	. "$HOME/.cargo/env"
	manifest_add "tool rustup $HOME/.cargo"
}

# Node for the build only, private to the install directory.
ensure_node() {
	if command -v node >/dev/null 2>&1 && version_ge "$(node --version | tr -d v)" "$MIN_NODE"; then
		return
	fi
	local dir arch tmp version tarball
	dir="$(install_dir)/node"
	if [ -x "$dir/bin/node" ] && version_ge "$("$dir/bin/node" --version | tr -d v)" "$MIN_NODE"; then
		PATH="$dir/bin:$PATH"
		return
	fi
	arch="$(arch_suffix)"
	[ "$arch" = amd64 ] && arch=x64
	tmp="$(mktemp -d)"
	# index.tab: first row whose "lts" column is not "-" is the newest LTS.
	version="$(curl -fsSL https://nodejs.org/dist/index.tab | awk -F'\t' 'NR > 1 && $10 != "-" { print $1; exit }')"
	[ -n "$version" ] || die "could not find the current Node LTS"
	tarball="node-$version-linux-$arch.tar.xz"
	say "Installing Node $version for the build (private copy)"
	curl -fsSL -o "$tmp/$tarball" "https://nodejs.org/dist/$version/$tarball"
	curl -fsSL -o "$tmp/SHASUMS256.txt" "https://nodejs.org/dist/$version/SHASUMS256.txt"
	check_sha256 "$tmp/$tarball" "$tmp/SHASUMS256.txt"
	rm -rf "$dir"
	mkdir -p "$dir"
	tar -xJf "$tmp/$tarball" -C "$dir" --strip-components=1
	rm -rf "$tmp"
	PATH="$dir/bin:$PATH"
}
