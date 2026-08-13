# Music Chapter Editor

Music Chapter Editor is a focused, local-first macOS application for naming and adjusting chapters in music Blu-ray MKV rips. It combines a precise chapter grid with optional MusicBrainz release lookup and delegates all Matroska writes to MKVToolNix.

Downloads and release checksums are published on the [GitHub Releases page](https://github.com/moonknight82/music-chapter-editor/releases).

The 0.1 beta targets Apple-silicon Macs running macOS 13 or newer. The Rust core and Tauri interface are structured so Linux packaging can follow without rewriting chapter or MusicBrainz behavior.

## What it does

- Opens or accepts a dropped `.mkv` file.
- Reads flat Matroska editions and chapters without decoding media.
- Edits titles and nanosecond-precision chapter starts.
- Adds, removes, reorders, and bulk-pastes chapters.
- Imports and exports Matroska chapter XML.
- Searches MusicBrainz by artist/release/media or an exact release MBID.
- Requires an explicit chapter-to-track mapping before applying MusicBrainz titles.
- Saves an APFS-cloned/full copy or edits in place after creating recovery data.
- Re-extracts and verifies every saved chapter set.
- Opens ordered, linked, or nested chapter structures read-only rather than flattening them.

It does not re-encode audio or video. MKA, MP4/M4A, embedded playback, CUE export, tags, file splitting, and audio-stream defaults are intentionally outside the 0.1 scope.

## Requirements

- macOS 13 or newer on Apple Silicon
- [MKVToolNix 90 or newer](https://mkvtoolnix.download/downloads.html)

Install only the command-line tools with Homebrew:

```sh
brew install mkvtoolnix
```

The app checks `/opt/homebrew/bin`, `/usr/local/bin`, `PATH`, and a directory selected in the setup screen. MKVToolNix is not bundled or downloaded silently.

## Development

Install [Rust](https://rustup.rs/), Node.js, and pnpm, then run:

```sh
pnpm install
pnpm tauri dev
```

For a public build, identify the project in MusicBrainz requests with a real repository or contact URL:

```sh
MUSIC_CHAPTER_EDITOR_CONTACT=https://github.com/OWNER/REPOSITORY pnpm tauri build --target aarch64-apple-darwin
```

Validation commands:

```sh
pnpm check
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

The Rust integration fixture generates a tiny PCM Matroska file and confirms that chapter edits leave its extracted media track byte-identical. It exits cleanly with a skip message when MKVToolNix is unavailable.

Ready-to-enable GitHub Actions definitions live in `github-workflows/`. Copy them to `.github/workflows/` after authenticating with GitHub's `workflow` permission.

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

Automatic updates are deliberately disabled. Install beta updates manually.

## Project structure

- `src/` — Svelte chapter editor, paste parser, mapping interface, and save workflow.
- `src-tauri/src/chapter_xml.rs` — Matroska XML model, preservation, validation, and rendering.
- `src-tauri/src/media.rs` — MKVToolNix identification/extraction and file identity.
- `src-tauri/src/musicbrainz.rs` — rate-limited, cached MusicBrainz client.
- `src-tauri/src/commands.rs` — Tauri commands, copy/in-place writes, recovery, cancellation, and verification.
- `src-tauri/tests/` — optional MKVToolNix media-preservation integration test.

## License

Music Chapter Editor is available under the MIT License. MKVToolNix is a separate program with its own GPL/LGPL licenses and is not redistributed by this project. Music metadata is supplied by MusicBrainz under its applicable terms.
