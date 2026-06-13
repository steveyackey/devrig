#!/bin/sh
# devrig installer — downloads the latest release archive, verifies its
# SHA256, and installs the `devrig` binary (dashboard included).
#
#   curl --proto '=https' --tlsv1.2 -LsSf https://github.com/steveyackey/devrig/releases/latest/download/install.sh | sh
#
# Env overrides:
#   DEVRIG_INSTALL_DIR   target dir (default: $HOME/.local/bin)
#   DEVRIG_VERSION       version tag to install (default: latest)
set -eu

REPO="steveyackey/devrig"
BIN="devrig"
INSTALL_DIR="${DEVRIG_INSTALL_DIR:-$HOME/.local/bin}"

err() { echo "devrig-install: $*" >&2; exit 1; }

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

need() { command -v "$1" >/dev/null 2>&1 || err "required tool not found: $1"; }
need curl
need tar

# --- resolve version ---
VERSION="${DEVRIG_VERSION:-}"
if [ -z "$VERSION" ]; then
	VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
		| grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
	[ -n "$VERSION" ] || err "could not determine latest version"
fi
VER="${VERSION#v}"

ARCHIVE="${BIN}_${VER}_${OS}_${ARCH}.tar.gz"
BASE="https://github.com/$REPO/releases/download/$VERSION"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

echo "Downloading $ARCHIVE ..."
curl -fsSL "$BASE/$ARCHIVE" -o "$tmp/$ARCHIVE" || err "download failed: $BASE/$ARCHIVE"
curl -fsSL "$BASE/SHA256SUMS" -o "$tmp/SHA256SUMS" || err "could not fetch SHA256SUMS"

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
install -m 0755 "$tmp/$BIN" "$INSTALL_DIR/$BIN" 2>/dev/null \
	|| { cp "$tmp/$BIN" "$INSTALL_DIR/$BIN" && chmod 0755 "$INSTALL_DIR/$BIN"; }

echo "Installed $BIN $VER to $INSTALL_DIR/$BIN"
case ":$PATH:" in
	*":$INSTALL_DIR:"*) ;;
	*) echo "NOTE: $INSTALL_DIR is not on your PATH. Add it, e.g.:"
	   echo "      export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
esac
echo "Run 'devrig update' to upgrade in place later."
