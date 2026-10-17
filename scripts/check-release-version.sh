#!/usr/bin/env bash
# Guard a release: the tag, the workspace Cargo.toml version and the PKGBUILD's
# pkgver must agree, because they are the three places the version is written by
# hand. nfpm and the AppImage take VERSION from the environment, so they cannot
# drift.
#
#   scripts/check-release-version.sh v0.1.0
#   scripts/check-release-version.sh          # no tag argument: check the files only
set -euo pipefail

cd "$(dirname "$0")/.."

tag="${1:-}"
tag="${tag#v}"
# A bare `v` (which the workflows' `v*` filter would match) must not pass as a
# version, and neither must anything else that is not vX.Y.Z.
if [ "$#" -ge 1 ]; then
    case "$tag" in
        [0-9]*.[0-9]*.[0-9]*) ;;
        *)
            echo "error: tag $1 is not vX.Y.Z" >&2
            exit 1
            ;;
    esac
fi

cargo_version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
pkgbuild_version="$(sed -n 's/^pkgver=\(.*\)/\1/p' packaging/arch/PKGBUILD | head -1)"

if [ -z "$cargo_version" ] || [ -z "$pkgbuild_version" ]; then
    echo "could not read a version (cargo='$cargo_version' pkgbuild='$pkgbuild_version')" >&2
    exit 1
fi

status=0
echo "Cargo.toml (workspace) : $cargo_version"
echo "packaging/arch/PKGBUILD: $pkgbuild_version"
if [ -n "$tag" ]; then echo "tag                    : $tag"; fi

if [ "$cargo_version" != "$pkgbuild_version" ]; then
    echo "error: PKGBUILD pkgver does not match the workspace version" >&2
    status=1
fi

if [ -n "$tag" ] && [ "$tag" != "$cargo_version" ]; then
    echo "error: tag v$tag does not match the workspace version" >&2
    status=1
fi

# The crates inherit the workspace version; a stray literal would silently
# package a different number than the one on the tag.
if grep -rn '^version = "' crates/*/Cargo.toml >/dev/null 2>&1; then
    echo "error: a crate sets its own version; it must inherit the workspace one" >&2
    grep -rn '^version = "' crates/*/Cargo.toml >&2
    status=1
fi

exit "$status"
