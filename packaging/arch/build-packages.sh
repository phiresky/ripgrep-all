#!/usr/bin/env bash
# Build the Arch/CachyOS pacman packages for a release, the same way they are
# attached to the GitHub release (generic x86-64 + Zen 4 / znver4 variant).
#
# Usage (on Arch/CachyOS or in an archlinux container):
#   ./build-packages.sh [pkgver]
#
# The script copies the matching PKGBUILD for each variant into a scratch
# dir, runs makepkg, and leaves the resulting .pkg.tar.zst files in
# packaging/arch/dist/.
set -euo pipefail

PKGVER="${1:-0.10.10.3}"
HERE="$(cd "$(dirname "$0")" && pwd)"
DIST="$HERE/dist"
mkdir -p "$DIST"

build_variant() {
  local variant="$1" pkgbuild="$2"
  local scratch="/tmp/rga-pkgbuild-$variant"
  rm -rf "$scratch"
  mkdir -p "$scratch"
  cp "$HERE/$pkgbuild" "$scratch/PKGBUILD"
  # Sync pkgver with the version being built (the PKGBUILD in-tree always
  # lags one release behind).
  sed -i "s/^pkgver=.*/pkgver=$PKGVER/" "$scratch/PKGBUILD"
  # shellcheck disable=SC2164
  cd "$scratch"
  # The sha256 of the release tarball only exists once the tag does, so
  # refresh the checksums here (pacman-contrib) instead of failing on a
  # stale value. Skipped silently when updpkgsums is not installed.
  if command -v updpkgsums >/dev/null 2>&1; then
    echo "==> Refreshing source checksums for $variant"
    updpkgsums
  fi
  echo "==> Building $variant (pkgver $PKGVER)"
  makepkg -s --noconfirm
  cp ./*.pkg.tar.zst "$DIST/"
  cd /
  rm -rf "$scratch"
}

build_variant generic PKGBUILD
build_variant znver4 PKGBUILD-znver4

echo "==> Done. Packages in $DIST:"
ls -1 "$DIST"/*.pkg.tar.zst
