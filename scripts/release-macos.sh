#!/bin/sh
set -eu

: "${MUSIC_CHAPTER_EDITOR_CONTACT:?Set MUSIC_CHAPTER_EDITOR_CONTACT to the public project or contact URL}"

target="aarch64-apple-darwin"
bundle_dir="src-tauri/target/$target/release/bundle"
product="Music Chapter Editor"

pnpm check
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build --target "$target" --bundles app,dmg

ditto -c -k --sequesterRsrc --keepParent \
  "$bundle_dir/macos/$product.app" \
  "$bundle_dir/macos/Music-Chapter-Editor-Apple-Silicon.zip"

find "$bundle_dir" -type f \( -name '*.dmg' -o -name '*.zip' \) -print0 |
  xargs -0 shasum -a 256 > "$bundle_dir/SHA256SUMS.txt"

echo "Release artifacts: $bundle_dir"
