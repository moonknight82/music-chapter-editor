#!/usr/bin/env bash
set -euo pipefail

: "${MUSIC_CHAPTER_EDITOR_CONTACT:?Set MUSIC_CHAPTER_EDITOR_CONTACT to the public project or contact URL}"

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${project_root}"

target="aarch64-apple-darwin"
bundle_dir="src-tauri/target/${target}/release/bundle"
macos_dir="${bundle_dir}/macos"
product="Music Chapter Editor"
release_slug="Music-Chapter-Editor-Apple-Silicon"
version="$(node -p "require('./package.json').version")"
tag="v${version}"

bash scripts/verify-release-version.sh "${tag}"

if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  local_key="${project_root}/.private/music-chapter-editor.key"
  if [[ ! -f "${local_key}" ]]; then
    echo "Missing updater signing key. Set TAURI_SIGNING_PRIVATE_KEY or restore ${local_key}." >&2
    exit 1
  fi
  export TAURI_SIGNING_PRIVATE_KEY="${local_key}"
fi
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"

pnpm check
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build --target "${target}" --bundles app
if [[ "${MUSIC_CHAPTER_EDITOR_SKIP_DMG:-0}" != "1" ]]; then
  pnpm tauri build --target "${target}" --bundles dmg
else
  echo "Skipping DMG creation because MUSIC_CHAPTER_EDITOR_SKIP_DMG=1."
fi

app_path="${macos_dir}/${product}.app"
updater_archive="${macos_dir}/${product}.app.tar.gz"
updater_signature="${updater_archive}.sig"
release_archive="${macos_dir}/${release_slug}.app.tar.gz"
release_signature="${release_archive}.sig"
release_zip="${macos_dir}/${release_slug}.zip"

if [[ ! -d "${app_path}" || ! -f "${updater_archive}" || ! -f "${updater_signature}" ]]; then
  echo "Tauri did not create the expected macOS application and updater artifacts." >&2
  exit 1
fi

cp "${updater_archive}" "${release_archive}"
cp "${updater_signature}" "${release_signature}"
ditto -c -k --sequesterRsrc --keepParent "${app_path}" "${release_zip}"

signature="$(tr -d '\r\n' < "${release_signature}")"
release_notes="${MUSIC_CHAPTER_EDITOR_RELEASE_NOTES:-MusicBrainz track subset selection and signed in-app updates.}"
pub_date="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"
update_url="https://github.com/moonknight82/music-chapter-editor/releases/download/${tag}/${release_slug}.app.tar.gz"

UPDATE_VERSION="${version}" \
UPDATE_NOTES="${release_notes}" \
UPDATE_PUB_DATE="${pub_date}" \
UPDATE_URL="${update_url}" \
UPDATE_SIGNATURE="${signature}" \
node --input-type=module <<'NODE' > "${bundle_dir}/latest.json"
const manifest = {
  version: process.env.UPDATE_VERSION,
  notes: process.env.UPDATE_NOTES,
  pub_date: process.env.UPDATE_PUB_DATE,
  platforms: {
    "darwin-aarch64": {
      signature: process.env.UPDATE_SIGNATURE,
      url: process.env.UPDATE_URL
    }
  }
};
process.stdout.write(`${JSON.stringify(manifest, null, 2)}\n`);
NODE

find "${bundle_dir}" -type f \
  \( -name '*.dmg' -o -name "${release_slug}.zip" -o -name "${release_slug}.app.tar.gz" -o -name "${release_slug}.app.tar.gz.sig" -o -name 'latest.json' \) \
  -print0 | sort -z | xargs -0 shasum -a 256 > "${bundle_dir}/SHA256SUMS.txt"

codesign --verify --deep --strict "${app_path}"

echo "Release artifacts: ${bundle_dir}"
