use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Emitter};

use crate::models::{
    AudioStream, FlacExtractionPlan, FlacExtractionRequest, FlacExtractionResult,
    FlacFallbackMetadata, FlacProbe, FlacProgress, FlacTrackPreview, MusicBrainzSelection,
    MusicBrainzTrackMetadata,
};

#[derive(Debug, Deserialize)]
struct ProbeResponse {
    #[serde(default)]
    streams: Vec<ProbeStream>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    index: u32,
    #[serde(default)]
    codec_name: String,
    #[serde(default)]
    codec_long_name: String,
    sample_fmt: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    channel_layout: Option<String>,
    bits_per_sample: Option<u32>,
    bits_per_raw_sample: Option<String>,
    #[serde(default)]
    disposition: ProbeDisposition,
    #[serde(default)]
    tags: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct ProbeDisposition {
    #[serde(default)]
    default: u32,
}

#[derive(Debug, Deserialize)]
struct VerifyResponse {
    #[serde(default)]
    streams: Vec<VerifyStream>,
    format: Option<VerifyFormat>,
}

#[derive(Debug, Deserialize)]
struct VerifyStream {
    #[serde(default)]
    codec_name: String,
}

#[derive(Debug, Deserialize)]
struct VerifyFormat {
    duration: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
}

fn executable_candidates(name: &str, preferred: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(directory) = preferred {
        paths.push(directory.join(name));
    }
    paths.push(PathBuf::from("/opt/homebrew/bin").join(name));
    paths.push(PathBuf::from("/usr/local/bin").join(name));
    if let Some(path) = env::var_os("PATH") {
        paths.extend(env::split_paths(&path).map(|directory| directory.join(name)));
    }
    paths.dedup();
    paths
}

fn locate_ffmpeg(preferred: Option<&Path>) -> Result<(PathBuf, PathBuf, Vec<String>), Vec<String>> {
    let ffmpeg_candidates = executable_candidates("ffmpeg", preferred);
    let searched = ffmpeg_candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    for ffmpeg in ffmpeg_candidates {
        if !ffmpeg.is_file() {
            continue;
        }
        let ffprobe = ffmpeg.with_file_name("ffprobe");
        if ffprobe.is_file() {
            return Ok((ffmpeg, ffprobe, searched));
        }
    }
    Err(searched)
}

fn parsed_bit_depth(stream: &ProbeStream) -> u32 {
    stream
        .bits_per_raw_sample
        .as_deref()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .or(stream.bits_per_sample.filter(|value| *value > 0))
        .unwrap_or_else(|| match stream.sample_fmt.as_deref().unwrap_or_default() {
            value if value.contains("s16") => 16,
            value if value.contains("s24") => 24,
            value if value.contains("s32") || value.contains("flt") => 24,
            value if value.contains("dbl") => 32,
            _ => 0,
        })
}

fn tag(tags: &HashMap<String, String>, name: &str) -> Option<String> {
    tags.iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}

pub fn probe(path: &Path, preferred: Option<&Path>) -> Result<FlacProbe, String> {
    let (ffmpeg, ffprobe, searched) = match locate_ffmpeg(preferred) {
        Ok(paths) => paths,
        Err(searched) => {
            return Ok(FlacProbe {
                available: false,
                ffmpeg_path: None,
                version: None,
                streams: Vec::new(),
                searched,
                message: "FLAC extraction needs FFmpeg. Install it with `brew install ffmpeg`."
                    .into(),
            });
        }
    };
    let version_output = Command::new(&ffmpeg)
        .arg("-version")
        .output()
        .map_err(|error| format!("Could not run FFmpeg: {error}"))?;
    let version = String::from_utf8_lossy(&version_output.stdout)
        .lines()
        .next()
        .map(str::to_string);
    let output = Command::new(&ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index,codec_name,codec_long_name,sample_fmt,sample_rate,channels,channel_layout,bits_per_sample,bits_per_raw_sample:stream_disposition=default:stream_tags=title,language",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| format!("Could not inspect the MKV audio streams: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "FFprobe could not inspect the MKV: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let parsed: ProbeResponse = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("FFprobe returned invalid JSON: {error}"))?;
    let streams = parsed
        .streams
        .into_iter()
        .map(|stream| {
            let bit_depth = parsed_bit_depth(&stream);
            AudioStream {
                index: stream.index,
                codec: stream.codec_name,
                codec_long_name: stream.codec_long_name,
                title: tag(&stream.tags, "title"),
                language: tag(&stream.tags, "language"),
                channels: stream.channels.unwrap_or(0),
                channel_layout: stream.channel_layout,
                sample_rate: stream
                    .sample_rate
                    .as_deref()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                bit_depth,
                is_default: stream.disposition.default != 0,
            }
        })
        .collect::<Vec<_>>();
    Ok(FlacProbe {
        available: true,
        ffmpeg_path: Some(ffmpeg.display().to_string()),
        version,
        message: if streams.is_empty() {
            "No audio streams were found in this MKV.".into()
        } else {
            format!("Found {} audio stream(s).", streams.len())
        },
        streams,
        searched,
    })
}

fn clean_component(value: &str, fallback: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| match character {
            '/' | ':' | '\\' | '\0' => '_',
            _ if character.is_control() => ' ',
            _ => character,
        })
        .collect::<String>();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        fallback.into()
    } else {
        cleaned.into()
    }
}

fn year(value: &str) -> String {
    value.trim().chars().take(4).collect()
}

fn padded_track(value: &str, fallback: usize) -> String {
    value
        .trim()
        .parse::<u32>()
        .map(|number| format!("{number:02}"))
        .unwrap_or_else(|_| {
            let value = clean_component(value, "");
            if value.is_empty() {
                format!("{:02}", fallback + 1)
            } else {
                value
            }
        })
}

fn resolved_metadata<'a>(
    selection: Option<&'a MusicBrainzSelection>,
    fallback: &'a FlacFallbackMetadata,
) -> (&'a str, &'a str, String, String, &'a str, &'a str) {
    if let Some(selection) = selection {
        (
            &selection.album_artist,
            &selection.release_title,
            selection
                .release_date
                .as_deref()
                .map(year)
                .unwrap_or_default(),
            selection.original_year.clone().unwrap_or_default(),
            selection.label.as_deref().unwrap_or_default(),
            selection.catalog_number.as_deref().unwrap_or_default(),
        )
    } else {
        (
            &fallback.album_artist,
            &fallback.album,
            year(&fallback.release_year),
            year(&fallback.original_year),
            &fallback.label,
            &fallback.catalog_number,
        )
    }
}

fn chapter_track<'a>(
    selection: Option<&'a MusicBrainzSelection>,
    chapter_index: usize,
) -> Option<&'a MusicBrainzTrackMetadata> {
    selection?
        .chapter_tracks
        .get(chapter_index)
        .and_then(Option::as_ref)
}

pub fn plan(request: &FlacExtractionRequest) -> Result<FlacExtractionPlan, String> {
    if request.document.advanced {
        return Err(
            "Ordered, linked, or nested chapters cannot be used for FLAC extraction.".into(),
        );
    }
    let edition = request
        .document
        .editions
        .get(request.document.active_edition)
        .ok_or_else(|| "The active edition no longer exists.".to_string())?;
    if edition.chapters.is_empty() {
        return Err("Add or import chapters before extracting FLAC tracks.".into());
    }
    let selection = request.document.musicbrainz_selection.as_ref();
    let (album_artist, album, release_year, original_year, label, catalog_number) =
        resolved_metadata(selection, &request.fallback);
    let mut warnings = Vec::new();
    for (name, value) in [
        ("album artist", album_artist),
        ("album", album),
        ("original year", original_year.as_str()),
        ("release year", release_year.as_str()),
        ("label", label),
        ("catalog number", catalog_number),
    ] {
        if value.trim().is_empty() {
            warnings.push(format!("The {name} metadata is blank."));
        }
    }
    if request.stream.sample_rate == 0 {
        warnings.push("FFprobe could not determine the audio sample rate.".into());
    }
    if request.stream.bit_depth == 0 {
        warnings.push("FFprobe could not determine the source bit depth.".into());
    }
    let technical = [
        (!release_year.is_empty()).then_some(release_year.clone()),
        (!label.trim().is_empty()).then(|| label.trim().to_string()),
        (!catalog_number.trim().is_empty()).then(|| catalog_number.trim().to_string()),
        (request.stream.sample_rate > 0)
            .then(|| format!("{} Khz", request.stream.sample_rate / 1000)),
        (request.stream.bit_depth > 0).then(|| format!("{} Bits", request.stream.bit_depth)),
        (request.stream.channels > 0).then(|| format!("{}ch", request.stream.channels)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" - ");
    let album_folder = if technical.is_empty() {
        format!(
            "{} ({})",
            clean_component(album, "Untitled album"),
            clean_component(&original_year, "Unknown year")
        )
    } else {
        format!(
            "{} ({}) [{}]",
            clean_component(album, "Untitled album"),
            clean_component(&original_year, "Unknown year"),
            clean_component(&technical, "Audio")
        )
    };
    let album_directory =
        PathBuf::from(clean_component(album_artist, "Unknown artist")).join(album_folder);
    let mut tracks = Vec::with_capacity(edition.chapters.len());
    for (index, chapter) in edition.chapters.iter().enumerate() {
        let end_ns = edition
            .chapters
            .get(index + 1)
            .map(|next| next.start_ns)
            .or(chapter.end_ns)
            .unwrap_or(request.document.duration_ns);
        if end_ns <= chapter.start_ns {
            return Err(format!(
                "Chapter {} does not have a positive duration.",
                index + 1
            ));
        }
        let mapped = chapter_track(selection, index);
        let title = mapped
            .map(|track| track.title.as_str())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(&chapter.title);
        let artist = mapped
            .map(|track| track.artist_credit.as_str())
            .filter(|artist| !artist.trim().is_empty())
            .unwrap_or(album_artist);
        let number = mapped
            .map(|track| track.number.as_str())
            .filter(|number| !number.trim().is_empty())
            .unwrap_or("");
        let prefix = if selection.is_some_and(|value| value.total_media > 1) {
            format!(
                "{}.",
                selection.map(|value| value.medium_position).unwrap_or(1)
            )
        } else {
            String::new()
        };
        let filename = format!(
            "{prefix}{} {}.flac",
            padded_track(number, index),
            clean_component(title, &format!("Track {}", index + 1))
        );
        tracks.push(FlacTrackPreview {
            chapter_index: index,
            relative_path: album_directory.join(filename).display().to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            track_number: if number.trim().is_empty() {
                (index + 1).to_string()
            } else {
                number.to_string()
            },
            start_ns: chapter.start_ns,
            end_ns,
        });
    }
    Ok(FlacExtractionPlan {
        album_directory: album_directory.display().to_string(),
        uses_musicbrainz: selection.is_some(),
        tracks,
        warnings,
    })
}

fn seconds(nanoseconds: u64) -> String {
    format!(
        "{}.{:09}",
        nanoseconds / 1_000_000_000,
        nanoseconds % 1_000_000_000
    )
}

fn add_metadata(command: &mut Command, key: &str, value: &str) {
    if !value.trim().is_empty() {
        command.arg("-metadata").arg(format!("{key}={value}"));
    }
}

fn run_ffmpeg(command: &mut Command, cancel: &AtomicBool) -> Result<(), String> {
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start FFmpeg: {error}"))?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("FLAC extraction cancelled.".into());
        }
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) => {
                let mut error = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut error);
                }
                return if status.success() {
                    Ok(())
                } else {
                    Err(if error.trim().is_empty() {
                        "FFmpeg could not extract this chapter.".into()
                    } else {
                        error.trim().into()
                    })
                };
            }
            None => thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn verify_track(ffprobe: &Path, path: &Path, title: &str) -> Result<(), String> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name:format=duration:format_tags=title,album,artist",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| format!("Could not verify the FLAC file: {error}"))?;
    let parsed: VerifyResponse = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("FFprobe returned invalid verification data: {error}"))?;
    if parsed
        .streams
        .first()
        .map(|stream| stream.codec_name.as_str())
        != Some("flac")
    {
        return Err("The extracted file did not contain a FLAC audio stream.".into());
    }
    let format = parsed
        .format
        .ok_or_else(|| "The extracted FLAC has no readable format metadata.".to_string())?;
    let duration = format
        .duration
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    if duration <= 0.0 {
        return Err("The extracted FLAC has no positive duration.".into());
    }
    if tag(&format.tags, "title").as_deref() != Some(title) {
        return Err("The extracted FLAC title tag did not verify.".into());
    }
    Ok(())
}

pub fn extract(
    request: FlacExtractionRequest,
    preferred: Option<&Path>,
    app: &AppHandle,
    cancel: &AtomicBool,
) -> Result<FlacExtractionResult, String> {
    extract_inner(request, preferred, cancel, |progress| {
        let _ = app.emit("flac-progress", progress);
    })
}

fn extract_inner(
    request: FlacExtractionRequest,
    preferred: Option<&Path>,
    cancel: &AtomicBool,
    emit_progress: impl Fn(FlacProgress),
) -> Result<FlacExtractionResult, String> {
    let (ffmpeg, ffprobe, _) = locate_ffmpeg(preferred).map_err(|_| {
        "FLAC extraction needs FFmpeg. Install it with `brew install ffmpeg`.".to_string()
    })?;
    let extraction_plan = plan(&request)?;
    let output_parent = PathBuf::from(&request.output_parent);
    if !output_parent.is_dir() {
        return Err("Choose an existing output folder.".into());
    }
    let final_paths = extraction_plan
        .tracks
        .iter()
        .map(|track| output_parent.join(&track.relative_path))
        .collect::<Vec<_>>();
    let unique_paths = final_paths.iter().collect::<HashSet<_>>();
    if unique_paths.len() != final_paths.len() {
        return Err("The naming preview contains duplicate output paths. Adjust the chapter or MusicBrainz track numbers before extracting.".into());
    }
    if let Some(path) = final_paths.iter().find(|path| path.exists()) {
        return Err(format!(
            "Nothing was extracted because the destination already contains {}.",
            path.display()
        ));
    }
    let album_directory = output_parent.join(&extraction_plan.album_directory);
    fs::create_dir_all(&album_directory)
        .map_err(|error| format!("Could not create the album folder: {error}"))?;
    cancel.store(false, Ordering::Relaxed);

    let selection = request.document.musicbrainz_selection.as_ref();
    let (album_artist, album, release_year, original_year, label, catalog_number) =
        resolved_metadata(selection, &request.fallback);
    let total_tracks = extraction_plan.tracks.len();
    let mut files = Vec::new();
    for (position, (preview, destination)) in extraction_plan
        .tracks
        .iter()
        .zip(final_paths.iter())
        .enumerate()
    {
        if cancel.load(Ordering::Relaxed) {
            return Err("FLAC extraction cancelled.".into());
        }
        let filename = destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("track.flac")
            .to_string();
        emit_progress(FlacProgress {
            phase: "extracting".into(),
            completed: position,
            total: total_tracks,
            filename: filename.clone(),
        });
        let temporary = destination.with_file_name(format!(
            ".music-chapter-editor-{}-{}.partial.flac",
            std::process::id(),
            position
        ));
        let mapped = chapter_track(selection, preview.chapter_index);
        let mut command = Command::new(&ffmpeg);
        command
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-ss",
            ])
            .arg(seconds(preview.start_ns))
            .arg("-i")
            .arg(&request.document.path)
            .arg("-map")
            .arg(format!("0:{}", request.stream.index))
            .arg("-t")
            .arg(seconds(preview.end_ns - preview.start_ns))
            .args([
                "-vn",
                "-sn",
                "-dn",
                "-map_metadata",
                "-1",
                "-c:a",
                "flac",
                "-compression_level",
                "8",
            ]);
        add_metadata(&mut command, "TITLE", &preview.title);
        add_metadata(&mut command, "ARTIST", &preview.artist);
        add_metadata(&mut command, "ALBUM", album);
        add_metadata(&mut command, "ALBUMARTIST", album_artist);
        let release_date = selection
            .and_then(|value| value.release_date.as_deref())
            .unwrap_or(&release_year);
        add_metadata(&mut command, "DATE", release_date);
        add_metadata(&mut command, "ORIGINALYEAR", &original_year);
        add_metadata(&mut command, "ORIGINALDATE", &original_year);
        add_metadata(&mut command, "LABEL", label);
        add_metadata(&mut command, "CATALOGNUMBER", catalog_number);
        add_metadata(&mut command, "TRACKNUMBER", &preview.track_number);
        add_metadata(
            &mut command,
            "TRACKTOTAL",
            &selection
                .map(|value| value.medium_track_count)
                .filter(|value| *value > 0)
                .unwrap_or(total_tracks as u32)
                .to_string(),
        );
        if let Some(selection) = selection {
            add_metadata(
                &mut command,
                "DISCNUMBER",
                &selection.medium_position.to_string(),
            );
            add_metadata(
                &mut command,
                "DISCTOTAL",
                &selection.total_media.to_string(),
            );
            add_metadata(&mut command, "MUSICBRAINZ_ALBUMID", &selection.release_id);
            if let Some(id) = selection.release_group_id.as_deref() {
                add_metadata(&mut command, "MUSICBRAINZ_RELEASEGROUPID", id);
            }
            if !selection.album_artist_ids.is_empty() {
                add_metadata(
                    &mut command,
                    "MUSICBRAINZ_ALBUMARTISTID",
                    &selection.album_artist_ids.join("; "),
                );
            }
        }
        if let Some(mapped) = mapped {
            if let Some(id) = mapped.recording_id.as_deref() {
                add_metadata(&mut command, "MUSICBRAINZ_TRACKID", id);
            }
            if let Some(id) = mapped.track_id.as_deref() {
                add_metadata(&mut command, "MUSICBRAINZ_RELEASETRACKID", id);
            }
            if !mapped.artist_ids.is_empty() {
                add_metadata(
                    &mut command,
                    "MUSICBRAINZ_ARTISTID",
                    &mapped.artist_ids.join("; "),
                );
            }
        }
        command.arg("-f").arg("flac").arg(&temporary);
        let result = run_ffmpeg(&mut command, cancel)
            .and_then(|_| verify_track(&ffprobe, &temporary, &preview.title));
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            for created in &files {
                let _ = fs::remove_file(created);
            }
            return Err(format!("Could not create {filename}: {error}"));
        }
        if let Err(error) = fs::hard_link(&temporary, destination) {
            let _ = fs::remove_file(&temporary);
            for created in &files {
                let _ = fs::remove_file(created);
            }
            return Err(format!(
                "Could not finalize {filename} without overwriting: {error}"
            ));
        }
        let _ = fs::remove_file(&temporary);
        files.push(destination.display().to_string());
        emit_progress(FlacProgress {
            phase: "verified".into(),
            completed: position + 1,
            total: total_tracks,
            filename,
        });
    }
    Ok(FlacExtractionResult {
        output_directory: album_directory.display().to_string(),
        files,
        warnings: extraction_plan.warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChapterDocument, ChapterEntry, Edition, FileIdentity};

    fn request(selection: Option<MusicBrainzSelection>) -> FlacExtractionRequest {
        FlacExtractionRequest {
            document: ChapterDocument {
                path: "/tmp/album.mkv".into(),
                duration_ns: 120_000_000_000,
                file_identity: FileIdentity {
                    device: "1".into(),
                    inode: "1".into(),
                    size: 1,
                    modified_ns: "1".into(),
                },
                editions: vec![Edition {
                    uid: None,
                    hidden: false,
                    default: true,
                    ordered: false,
                    chapters: vec![
                        ChapterEntry {
                            uid: None,
                            start_ns: 0,
                            end_ns: None,
                            title: "Intro".into(),
                            displays: vec![],
                            source_index: None,
                        },
                        ChapterEntry {
                            uid: None,
                            start_ns: 30_000_000_000,
                            end_ns: None,
                            title: "Song / Two".into(),
                            displays: vec![],
                            source_index: None,
                        },
                    ],
                    source_index: None,
                }],
                active_edition: 0,
                advanced: false,
                advanced_reason: None,
                original_xml: None,
                inferred_artist: "Fallback Artist".into(),
                inferred_release: "Fallback Album".into(),
                musicbrainz_selection: selection,
            },
            stream: AudioStream {
                index: 2,
                codec: "pcm_s24le".into(),
                codec_long_name: "PCM signed 24-bit little-endian".into(),
                title: Some("Stereo".into()),
                language: Some("eng".into()),
                channels: 2,
                channel_layout: Some("stereo".into()),
                sample_rate: 96_000,
                bit_depth: 24,
                is_default: false,
            },
            output_parent: "/tmp".into(),
            fallback: FlacFallbackMetadata {
                album_artist: "Fallback Artist".into(),
                album: "Fallback Album".into(),
                release_year: "2024".into(),
                original_year: "1976".into(),
                label: "Fallback Label".into(),
                catalog_number: "ABC 1".into(),
            },
        }
    }

    #[test]
    fn preset_three_uses_musicbrainz_and_selected_track_numbers() {
        let selection = MusicBrainzSelection {
            release_id: "release-id".into(),
            release_group_id: Some("group-id".into()),
            release_title: "Black and Blue".into(),
            album_artist: "The Rolling Stones".into(),
            album_artist_ids: vec!["artist-id".into()],
            release_date: Some("2025-11-14".into()),
            original_year: Some("1976".into()),
            label: Some("Polydor".into()),
            catalog_number: Some("12345".into()),
            medium_position: 3,
            medium_title: Some("New mix".into()),
            medium_track_count: 12,
            total_media: 4,
            chapter_tracks: vec![
                Some(MusicBrainzTrackMetadata {
                    track_id: Some("track-11".into()),
                    recording_id: Some("recording-11".into()),
                    position: 11,
                    number: "11".into(),
                    title: "Hot Stuff".into(),
                    artist_credit: "The Rolling Stones".into(),
                    artist_ids: vec!["artist-id".into()],
                }),
                None,
            ],
        };
        let plan = plan(&request(Some(selection))).unwrap();
        assert!(plan.uses_musicbrainz);
        assert_eq!(
            plan.album_directory,
            "The Rolling Stones/Black and Blue (1976) [2025 - Polydor - 12345 - 96 Khz - 24 Bits - 2ch]"
        );
        assert!(
            plan.tracks[0]
                .relative_path
                .ends_with("3.11 Hot Stuff.flac")
        );
        assert!(
            plan.tracks[1]
                .relative_path
                .ends_with("3.02 Song _ Two.flac")
        );
    }

    #[test]
    fn falls_back_to_editable_metadata_without_musicbrainz() {
        let plan = plan(&request(None)).unwrap();
        assert!(!plan.uses_musicbrainz);
        assert!(
            plan.album_directory
                .starts_with("Fallback Artist/Fallback Album (1976)")
        );
        assert!(plan.tracks[0].relative_path.ends_with("01 Intro.flac"));
    }

    #[test]
    fn extracts_and_verifies_real_flac_tracks_without_touching_source() {
        let Ok((ffmpeg, ffprobe, _)) = locate_ffmpeg(None) else {
            eprintln!("skipping FLAC integration test: FFmpeg is unavailable");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.mkv");
        let status = Command::new(&ffmpeg)
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:duration=2",
                "-map",
                "0:a",
                "-map",
                "1:a",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-c:a",
                "pcm_s24le",
                "-metadata:s:a:0",
                "title=Stereo 440",
                "-metadata:s:a:1",
                "title=Stereo 880",
                "-y",
            ])
            .arg(&source)
            .status()
            .unwrap();
        assert!(status.success());
        let source_before = fs::read(&source).unwrap();
        let found = probe(&source, ffmpeg.parent()).unwrap();
        assert_eq!(found.streams.len(), 2);
        let mut extraction = request(None);
        extraction.document.path = source.display().to_string();
        extraction.document.duration_ns = 2_000_000_000;
        extraction.document.editions[0].chapters[1].start_ns = 1_000_000_000;
        extraction.stream = found.streams[1].clone();
        extraction.output_parent = directory.path().display().to_string();
        let result =
            extract_inner(extraction, ffmpeg.parent(), &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(result.files.len(), 2);
        assert_eq!(fs::read(&source).unwrap(), source_before);
        for file in result.files {
            let output = Command::new(&ffprobe)
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "stream=codec_name:format_tags=album,album_artist,title",
                    "-of",
                    "json",
                ])
                .arg(file)
                .output()
                .unwrap();
            assert!(output.status.success());
            let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                json.pointer("/streams/0/codec_name")
                    .and_then(|v| v.as_str()),
                Some("flac")
            );
            assert_eq!(
                json.pointer("/format/tags/ALBUM").and_then(|v| v.as_str()),
                Some("Fallback Album")
            );
        }
    }
}
