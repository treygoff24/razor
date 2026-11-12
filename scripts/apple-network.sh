#!/bin/sh
# Builds razor-network as a static library for the phone, the Simulator, Mac Catalyst and the Mac,
# bundles the four with the C header as RazorNetworkFFI.xcframework, and assembles the Swift package
# RazorNetwork at OUT, which an app adds as a local package (docs/design/network.md).
#
#   scripts/apple-network.sh [OUT]      OUT defaults to target/apple/RazorNetwork
#
# Cargo's output goes to CARGO_TARGET_DIR, or the workspace's target/. It builds with the toolchain
# in use, which cargo refuses when older than the workspace's rust-version, and with Cargo.lock as
# it is; it prints the toolchain and each slice's size.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
apple=$root/network/apple
target=${CARGO_TARGET_DIR:-$root/target}
out=${1:-$target/apple/RazorNetwork}
targets="aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-ios-macabi aarch64-apple-darwin"

# The apps' deployment targets, so the linker matches the objects to the app.
export IPHONEOS_DEPLOYMENT_TARGET=27.0 MACOSX_DEPLOYMENT_TARGET=27.0
rustup target add $targets >/dev/null
for t in $targets; do
    (cd "$root" && CARGO_TARGET_DIR=$target cargo rustc -p razor-network --release --locked --target "$t" --crate-type staticlib)
done

# The header and module map sit in a folder of their own, so Xcode copying every xcframework's
# headers into one include folder never puts two module maps in the same place.
headers=$target/apple-headers
rm -rf "$headers" "$out"
mkdir -p "$headers/RazorNetworkFFI" "$out"
cp "$root/network/include/razor_network.h" "$headers/RazorNetworkFFI/"
cat >"$headers/RazorNetworkFFI/module.modulemap" <<'MAP'
module RazorNetworkFFI {
    header "razor_network.h"
    export *
}
MAP
set --
for t in $targets; do set -- "$@" -library "$target/$t/release/librazor_network.a" -headers "$headers"; done
xcodebuild -create-xcframework "$@" -output "$out/RazorNetworkFFI.xcframework" >/dev/null
cp -R "$apple/Package.swift" "$apple/Sources" "$out/"
rustc --version
for t in $targets; do
    printf '%-26s %s\n' "$t" "$(du -h "$target/$t/release/librazor_network.a" | cut -f1)"
done
echo "$out"
