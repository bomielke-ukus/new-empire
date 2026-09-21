#!/usr/bin/env bash
# Assembles "New Empire.app": the release game binary, its sprite sets and
# a manifest, so the game runs from a double-click on a Mac that has no
# toolchain. The Mac build workflow runs this and attaches the zip; it also
# works from a checkout on a Mac.
#
#   scripts/bundle-mac.sh [OUT_DIR]      # default: target/bundle
#
# The bundle is ad-hoc signed where `codesign` exists — Apple Silicon will
# not run an unsigned binary at all — but not notarised, so the first launch
# of a downloaded copy goes through Gatekeeper (README, "Testing a build on
# a Mac"). `ditto` makes the zip because it keeps the bundle's permissions,
# which `zip` in an Actions runner has not always done. On a machine without
# those tools (Linux, for the dry run) the layout is still assembled, which
# is what the tests need.
set -euo pipefail
cd "$(dirname "$0")/.."

out="${1:-target/bundle}"
app="$out/New Empire.app"
version="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
build="${GITHUB_RUN_NUMBER:-0}"

echo "== building the game in release =="
cargo build --release -p new-empire

echo "== laying out $app =="
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/assets"
cp target/release/new-empire "$app/Contents/MacOS/new-empire"
cp -R assets/sprites "$app/Contents/Resources/assets/sprites"
# Recordings, once there are any; the placeholders are in the binary.
if [ -d assets/sounds ]; then
  cp -R assets/sounds "$app/Contents/Resources/assets/sounds"
fi
sed -e "s/@VERSION@/$version/" -e "s/@BUILD@/$build/" \
  packaging/macos/Info.plist > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"

# The game looks for its art beside the binary and in Contents/Resources
# (view::sheets::candidates), so a set that did not copy is a silent
# fallback to placeholders. Check it copied.
test -f "$app/Contents/Resources/assets/sprites/villager/"*.ron

if command -v plutil >/dev/null 2>&1; then
  plutil -lint "$app/Contents/Info.plist"
fi
if command -v codesign >/dev/null 2>&1; then
  echo "== ad-hoc signing =="
  codesign --force --deep --sign - "$app"
  codesign --verify --deep --strict "$app"
fi
if command -v ditto >/dev/null 2>&1; then
  echo "== zipping =="
  rm -f "$out/New Empire.zip"
  ditto -c -k --keepParent "$app" "$out/New Empire.zip"
fi

echo "bundled: $app (version $version, build $build)"
