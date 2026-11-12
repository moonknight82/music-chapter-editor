# Music Chapter Editor

Music Chapter Editor is a focused, local-first macOS application for naming and adjusting chapters in music Blu-ray MKV rips. It combines a precise chapter grid with optional MusicBrainz release lookup and delegates all Matroska writes to MKVToolNix.

Downloads and release checksums are published on the [GitHub Releases page](https://github.com/moonknight82/music-chapter-editor/releases).

The 0.2 beta targets Apple-silicon Macs running macOS 13 or newer. The Rust core and Tauri interface are structured so Linux packaging can follow without rewriting chapter or MusicBrainz behavior.

## What it does

- Opens or accepts a dropped `.mkv` file.
- Reads flat Matroska editions and chapters without decoding media.
- Edits titles and nanosecond-precision chapter starts.
- Adds, removes, reorders, and bulk-pastes chapters.
- Imports and exports Matroska chapter XML.
- Searches MusicBrainz by artist/release/media or an exact release MBID.
- Lets you select only the MusicBrainz tracks belonging to the current MKV, then requires an explicit chapter-to-track mapping before applying their titles.
- Extracts a chosen MKV audio stream into one verified FLAC per chapter without changing the source file.
- Uses the selected MusicBrainz release, medium, and per-chapter track mapping for FLAC names and tags when available.
- Saves an APFS-cloned/full copy or edits in place after creating recovery data.
- Re-extracts and verifies every saved chapter set.
- Opens ordered, linked, or nested chapter structures read-only rather than flattening them.
- Checks the official GitHub release feed daily and can install a cryptographically signed update after user confirmation.

Chapter saves never re-encode audio or video. FLAC extraction intentionally decodes only the selected audio stream and encodes lossless FLAC files. MKA, MP4/M4A, embedded playback, CUE export, Matroska tags, and audio-stream defaults are intentionally outside the 0.2 scope.

## Requirements

- macOS 13 or newer on Apple Silicon
- [MKVToolNix 90 or newer](https://mkvtoolnix.download/downloads.html)
- [FFmpeg](https://ffmpeg.org/) is optional and required only for FLAC extraction

Install only the command-line tools with Homebrew:

```sh
brew install mkvtoolnix
brew install ffmpeg
```

The app checks `/opt/homebrew/bin`, `/usr/local/bin`, `PATH`, and a directory selected in the setup screen. MKVToolNix and FFmpeg are not bundled or downloaded silently. Missing FFmpeg disables only extraction; chapter editing remains available.

## FLAC extraction and naming

After opening an MKV, choose **Extract FLAC** and select the desired audio stream. The active flat edition supplies exact chapter boundaries, including unsaved title and timestamp edits. Ordered, linked, and nested Matroska chapters remain unsupported for extraction.

When a MusicBrainz mapping has been applied, its release and track metadata is authoritative for album artist, album, dates, label, catalog number, disc/track numbers, track artists/titles, and MusicBrainz IDs. Rows mapped to **Keep current title** retain the chapter title. Without MusicBrainz, the dialog provides editable fallback album fields.

Paths follow the app author's MusicBrainz Picard Preset 3, with channel count added to the technical description:

```text
Album Artist/
Album (Original Year) [Release Year - Label - Catalog Number - 96 Khz - 24 Bits - 2ch]/
01 Track Title.flac
```

Multi-medium releases use `disc.track` prefixes, such as `3.11 Track Title.flac`. The app previews every output path, refuses to overwrite existing files, writes to temporary files, and verifies the FLAC codec, duration, and title tag before finalizing each track.

## Development

Install [Rust](https://rustup.rs/), Node.js, and pnpm, then run:

```sh
pnpm install
pnpm tauri dev
```

An ordinary local build can disable release-artifact generation:

```sh
MUSIC_CHAPTER_EDITOR_CONTACT=https://github.com/OWNER/REPOSITORY \
  pnpm tauri build --target aarch64-apple-darwin \
  --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

Official release builds require the project’s updater private key. Keep it outside source control, back it up securely, and provide its absolute path through `TAURI_SIGNING_PRIVATE_KEY`. The checked-in public key verifies update archives. `scripts/release-macos.sh` runs validation, builds the app/DMG/ZIP, signs the updater archive, creates `latest.json`, verifies the app signature, and writes SHA-256 checksums. Set `MUSIC_CHAPTER_EDITOR_SKIP_DMG=1` only in a restricted environment without macOS DiskManagement; the installable ZIP and updater archive will still be produced.

Validation commands:

```sh
pnpm check
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

The Rust integration fixture generates a tiny PCM Matroska file and confirms that chapter edits leave its extracted media track byte-identical. It exits cleanly with a skip message when MKVToolNix is unavailable.

Ready-to-enable GitHub Actions definitions live in `github-workflows/`. Copy them to `.github/workflows/` after authenticating with GitHub's `workflow` permission. The release workflow expects `TAURI_SIGNING_PRIVATE_KEY` and, when used, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets. Updater-enabled releases must be published as regular GitHub releases because the app reads `releases/latest/download/latest.json`.

## Application updates

The installed app checks for a newer release at most once every 24 hours, five seconds after launch. A footer button also provides an on-demand check. It never installs silently: the user reviews the version and release notes, then chooses **Install and restart**. Unsaved chapter edits require an additional confirmation before restart.

Tauri verifies every update archive against the public key embedded in the app. The private update-signing key is ignored by Git and is not included in the application, DMG, ZIP, source archive, logs, or checksums. Losing that key prevents the existing app from accepting future updates, so maintainers must keep an offline backup.

## Saving and recovery

“Save an edited copy” is always preselected. On APFS, the app first tries a copy-on-write clone; otherwise it performs a cancellable streamed copy. It refuses to overwrite an existing destination.

“Edit in place” first verifies that the source’s device, inode, size, and modification timestamp still match the opened file. It then writes one of these beside the MKV:

- `Name.chapters-backup-YYYYMMDD-HHMMSS-mmm.xml` when chapters existed.
- `Name.chapters-backup-YYYYMMDD-HHMMSS-mmm.json` when the MKV was chapterless.

The Restore command accepts either format. A failed edit never removes its recovery file.

## MusicBrainz privacy and etiquette

The app works without MusicBrainz. When the user explicitly searches, only the entered artist, release, media filter, or release MBID is sent to `musicbrainz.org`. MKV files and chapter XML never leave the Mac.

Requests are serialized to at most one per second, cached locally for seven days, and retried with bounded backoff after rate limiting or temporary service failure. No MusicBrainz account or API key is used.

## Unsigned beta installation

The project currently has no Apple Developer identity. Release binaries are ad-hoc signed for Apple Silicon but are not Developer ID signed or notarized by Apple.

Download binaries only from the project’s official release page and compare the published SHA-256 checksum. After moving the app to Applications and trying to open it, macOS may block it. If you trust the verified build, open **System Settings → Privacy & Security** and choose **Open Anyway**. Apple documents this exception process in [Open a Mac app from an unknown developer](https://support.apple.com/guide/mac-help/mh40616/mac).

Version 0.2 and later can check for and install signed updates. The initial 0.2 installation is still manual, and macOS may require the same **Open Anyway** exception because the project does not have a Developer ID or notarization.

## Project structure

- `src/` — Svelte chapter editor, paste parser, mapping interface, and save workflow.
- `src-tauri/src/chapter_xml.rs` — Matroska XML model, preservation, validation, and rendering.
- `src-tauri/src/media.rs` — MKVToolNix identification/extraction and file identity.
- `src-tauri/src/musicbrainz.rs` — rate-limited, cached MusicBrainz client.
- `src-tauri/src/flac.rs` — FFmpeg discovery, audio probing, Picard-style naming, extraction, tagging, cancellation, and verification.
- `src-tauri/src/commands.rs` — Tauri commands, copy/in-place writes, recovery, cancellation, and verification.
- `src-tauri/tests/` — optional MKVToolNix media-preservation integration test.

## License

Music Chapter Editor is available under the MIT License. MKVToolNix is a separate program with its own GPL/LGPL licenses and is not redistributed by this project. Music metadata is supplied by MusicBrainz under its applicable terms.
