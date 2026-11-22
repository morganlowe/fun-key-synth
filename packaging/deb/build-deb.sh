#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 3 ]; then
  echo "Usage: $0 <version> <binary-path> <out-dir>" >&2
  exit 1
fi

RAW_VERSION="$1"
VERSION="${RAW_VERSION#v}"
VERSION="${VERSION#V}"
BINARY_PATH="$2"
OUT_DIR="$3"

if [ ! -f "$BINARY_PATH" ]; then
  echo "Binary not found: $BINARY_PATH" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGING_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="${SOURCE_DIR:-$PWD}"

DESKTOP_FILE="$REPO_ROOT/packaging/fun-key-synth.desktop"
if [ ! -f "$DESKTOP_FILE" ]; then
  DESKTOP_FILE="$PACKAGING_DIR/fun-key-synth.desktop"
fi

ICON_FILE="$REPO_ROOT/packaging/fun-key-synth.png"
if [ ! -f "$ICON_FILE" ]; then
  ICON_FILE="$PACKAGING_DIR/fun-key-synth.png"
fi

STAGING="$(mktemp -d)"
cleanup() {
  rm -rf "$STAGING"
}
trap cleanup EXIT

install -Dm755 "$BINARY_PATH" "$STAGING/usr/bin/fun-key-synth"
if command -v strip >/dev/null 2>&1; then
  strip --strip-unneeded "$STAGING/usr/bin/fun-key-synth"
fi

install -Dm644 "$DESKTOP_FILE" "$STAGING/usr/share/applications/fun-key-synth.desktop"
install -Dm644 "$ICON_FILE" "$STAGING/usr/share/icons/hicolor/256x256/apps/fun-key-synth.png"

if [ -f "$REPO_ROOT/README.md" ]; then
  install -Dm644 "$REPO_ROOT/README.md" "$STAGING/usr/share/doc/fun-key-synth/README.md"
fi

if [ -f "$REPO_ROOT/LICENSE" ]; then
  install -Dm644 "$REPO_ROOT/LICENSE" "$STAGING/usr/share/doc/fun-key-synth/copyright"
fi

INSTALLED_SIZE="$(du -sk "$STAGING/usr" | cut -f1)"

mkdir -p "$STAGING/DEBIAN"
chmod 0755 "$STAGING/DEBIAN"

cat > "$STAGING/DEBIAN/control" <<EOF
Package: fun-key-synth
Version: ${VERSION}
Section: sound
Priority: optional
Architecture: amd64
Installed-Size: ${INSTALLED_SIZE}
Maintainer: Morgan Lowe <space-bear-taur@proton.me>
Homepage: https://github.com/morganlowe/fun-key-synth
Depends: libc6 (>= 2.34), libasound2t64 | libasound2, libxkbcommon0, libgl1, libegl1, libwayland-client0 | libx11-6
Description: Fun Key Synth — Analog Vibe Studio & 8-Part Performance Sampler
 A beginner-friendly analog vibe studio, Auto-Chord / Auto-Beat groovebox,
 local Party Mode song generator, and Volca-style 8-part performance sampler
 built in Rust.
EOF
chmod 0644 "$STAGING/DEBIAN/control"

mkdir -p "$OUT_DIR"
DEB_PATH="$OUT_DIR/fun-key-synth_${VERSION}_amd64.deb"
dpkg-deb --root-owner-group --build "$STAGING" "$DEB_PATH"

dpkg-deb --info "$DEB_PATH"
dpkg-deb --contents "$DEB_PATH"
