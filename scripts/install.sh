#!/bin/sh
# devrig installer — downloads the latest release archive, verifies its
# SHA256, installs the `devrig` binary (dashboard included), and adds the
# install dir to your PATH.
#
#   curl --proto '=https' --tlsv1.2 -LsSf https://github.com/steveyackey/devrig/releases/latest/download/install.sh | sh
#
# Flags / env:
#   --help                  show this help
#   --no-modify-path        don't touch shell rc files (or DEVRIG_NO_MODIFY_PATH=1)
#   DEVRIG_INSTALL_DIR=DIR   install location (default: $HOME/.local/bin)
#   DEVRIG_VERSION=vX.Y.Z    version to install (default: latest)
set -eu

REPO="steveyackey/devrig"
BIN="devrig"
INSTALL_DIR="${DEVRIG_INSTALL_DIR:-$HOME/.local/bin}"
MODIFY_PATH="${DEVRIG_NO_MODIFY_PATH:+0}"
MODIFY_PATH="${MODIFY_PATH:-1}"

usage() {
	sed -n '2,15p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'
	exit 0
}
err() { echo "devrig-install: $*" >&2; exit 1; }

for arg in "$@"; do
	case "$arg" in
		--help | -h) usage ;;
		--no-modify-path) MODIFY_PATH=0 ;;
		*) err "unknown argument: $arg (try --help)" ;;
	esac
done

# --- platform detection ---
os="$(uname -s)"
case "$os" in
	Linux) OS="linux" ;;
	Darwin) OS="darwin" ;;
	*) err "unsupported OS: $os (use the prebuilt archive or 'go install')" ;;
esac
arch="$(uname -m)"
case "$arch" in
	x86_64 | amd64) ARCH="x86_64" ;;
	arm64 | aarch64) ARCH="arm64" ;;
	*) err "unsupported architecture: $arch" ;;
esac

# --- downloader (curl or wget) ---
if command -v curl >/dev/null 2>&1; then
	dl() { curl -fsSL "$1" -o "$2"; }
	dl_stdout() { curl -fsSL "$1"; }
elif command -v wget >/dev/null 2>&1; then
	dl() { wget -qO "$2" "$1"; }
	dl_stdout() { wget -qO - "$1"; }
else
	err "need curl or wget"
fi
command -v tar >/dev/null 2>&1 || err "need tar"

# --- resolve version ---
VERSION="${DEVRIG_VERSION:-}"
if [ -z "$VERSION" ]; then
	VERSION="$(dl_stdout "https://api.github.com/repos/$REPO/releases/latest" \
		| grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
	[ -n "$VERSION" ] || err "could not determine latest version"
fi
VER="${VERSION#v}"

ARCHIVE="${BIN}_${VER}_${OS}_${ARCH}.tar.gz"
BASE="https://github.com/$REPO/releases/download/$VERSION"

tmp="$(mktemp -d)"
staged=""
trap 'rm -rf "$tmp" ${staged:+"$staged"}' EXIT INT TERM

echo "Downloading $ARCHIVE ..."
dl "$BASE/$ARCHIVE" "$tmp/$ARCHIVE" || err "download failed: $BASE/$ARCHIVE"
dl "$BASE/SHA256SUMS" "$tmp/SHA256SUMS" || err "could not fetch SHA256SUMS"

echo "Verifying checksum ..."
(
	cd "$tmp"
	line="$(grep " $ARCHIVE\$" SHA256SUMS)" || err "no checksum entry for $ARCHIVE"
	if command -v sha256sum >/dev/null 2>&1; then
		printf '%s\n' "$line" | sha256sum -c - >/dev/null
	elif command -v shasum >/dev/null 2>&1; then
		printf '%s\n' "$line" | shasum -a 256 -c - >/dev/null
	else
		err "no sha256 tool (sha256sum/shasum) found"
	fi
) || err "checksum verification failed"

tar -xzf "$tmp/$ARCHIVE" -C "$tmp"
[ -f "$tmp/$BIN" ] || err "binary $BIN not found in archive"

mkdir -p "$INSTALL_DIR"
# Atomic install: stage the binary in the target dir, then rename over the
# destination. rename(2) swaps the directory entry without touching the old
# inode — so a currently-running `devrig` keeps working, there's no
# partial-write window, and no ETXTBSY ("text file busy") from overwriting a
# binary in use. (Same approach as `devrig update`.)
staged="$INSTALL_DIR/.$BIN.new.$$"
cp "$tmp/$BIN" "$staged" || err "could not write to $INSTALL_DIR (permission?)"
chmod 0755 "$staged"
mv -f "$staged" "$INSTALL_DIR/$BIN" || err "could not install to $INSTALL_DIR/$BIN"
staged=""
echo "Installed $BIN $VER to $INSTALL_DIR/$BIN"

# --- add install dir to PATH ---
on_path=0
case ":$PATH:" in *":$INSTALL_DIR:"*) on_path=1 ;; esac

if [ "$on_path" = "1" ]; then
	: # already on PATH, nothing to do
elif [ "$MODIFY_PATH" = "0" ]; then
	echo "NOTE: $INSTALL_DIR is not on your PATH. Add it: export PATH=\"$INSTALL_DIR:\$PATH\""
else
	line="export PATH=\"$INSTALL_DIR:\$PATH\""
	added=""
	for rc in "$HOME/.profile" "$HOME/.bashrc" "$HOME/.zshrc"; do
		# only touch rc files that already exist, plus always ensure .profile
		if [ -f "$rc" ] || [ "$rc" = "$HOME/.profile" ]; then
			if ! { [ -f "$rc" ] && grep -qF "$INSTALL_DIR" "$rc"; }; then
				printf '\n# added by devrig installer\n%s\n' "$line" >> "$rc"
				added="$added $rc"
			fi
		fi
	done
	[ -n "$added" ] && echo "Added $INSTALL_DIR to PATH in:$added (restart your shell or 'source' it)"
fi

echo "Run 'devrig update' to upgrade in place later."
