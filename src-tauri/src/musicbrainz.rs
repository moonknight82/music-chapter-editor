use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use regex::Regex;
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};

use crate::models::{
    MediumSummary, MediumTracklist, ReleaseCandidate, ReleaseDetails, SearchRequest, TrackInfo,
};

const CACHE_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug)]
pub struct MusicBrainzClient {
    client: Client,
    last_request: Mutex<Instant>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    releases: Vec<MbRelease>,
}

#[derive(Debug, Clone, Deserialize)]
struct ArtistCredit {
    #[serde(default)]
    name: String,
    #[serde(default)]
    joinphrase: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MbRecording {
    id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MbTrack {
    #[serde(default)]
    position: u32,
    #[serde(default)]
    number: String,
    #[serde(default)]
    title: String,
    length: Option<u64>,
    recording: Option<MbRecording>,
}

#[derive(Debug, Clone, Deserialize)]
struct MbMedium {
    id: Option<String>,
    #[serde(default)]
    position: u32,
    title: Option<String>,
    format: Option<String>,
    #[serde(rename = "track-count", default)]
    track_count: u32,
    #[serde(default)]
    tracks: Vec<MbTrack>,
}

#[derive(Debug, Clone, Deserialize)]
struct MbRelease {
    id: String,
    #[serde(default)]
    title: String,
    date: Option<String>,
    country: Option<String>,
    status: Option<String>,
    score: Option<u32>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<ArtistCredit>,
    #[serde(default)]
    media: Vec<MbMedium>,
}

fn artist_credit(credit: &[ArtistCredit]) -> String {
    credit
        .iter()
        .map(|item| format!("{}{}", item.name, item.joinphrase))
        .collect::<String>()
}

fn candidate(release: MbRelease) -> ReleaseCandidate {
    ReleaseCandidate {
        id: release.id,
        title: release.title,
        artist_credit: artist_credit(&release.artist_credit),
        date: release.date,
        country: release.country,
        status: release.status,
        score: release.score.unwrap_or(100),
        media: release
            .media
            .into_iter()
            .map(|medium| MediumSummary {
                id: medium.id,
                position: medium.position,
                title: medium.title,
                format: medium.format,
                track_count: medium.track_count,
            })
            .collect(),
    }
}

fn details(release: MbRelease) -> ReleaseDetails {
    ReleaseDetails {
        id: release.id,
        title: release.title,
        artist_credit: artist_credit(&release.artist_credit),
        media: release
            .media
            .into_iter()
            .map(|medium| MediumTracklist {
                id: medium.id,
                position: medium.position,
                title: medium.title,
                format: medium.format,
                tracks: medium
                    .tracks
                    .into_iter()
                    .map(|track| TrackInfo {
                        position: track.position,
                        number: track.number,
                        title: track.title,
                        length_ms: track.length,
                        recording_id: track.recording.and_then(|recording| recording.id),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn cache_file(app: &AppHandle, url: &str) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Could not resolve the cache directory: {error}"))?
        .join("musicbrainz");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create the MusicBrainz cache: {error}"))?;
    let digest = hex::encode(Sha256::digest(url.as_bytes()));
    Ok(directory.join(format!("{digest}.json")))
}

fn read_cache(path: &PathBuf) -> Option<Vec<u8>> {
    let metadata = fs::metadata(path).ok()?;
    let age = SystemTime::now()
        .duration_since(metadata.modified().ok()?)
        .ok()?;
    (age <= CACHE_MAX_AGE)
        .then(|| fs::read(path).ok())
        .flatten()
}

fn extract_mbid(value: &str) -> Option<String> {
    Regex::new(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b")
        .unwrap()
        .find(value)
        .map(|found| found.as_str().to_ascii_lowercase())
}

fn lucene_phrase(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

impl MusicBrainzClient {
    pub fn new() -> Result<Self, String> {
        let contact = option_env!("MUSIC_CHAPTER_EDITOR_CONTACT")
            .unwrap_or("app.musicchaptereditor.desktop; local open-source beta");
        let client = Client::builder()
            .user_agent(format!("MusicChapterEditor/0.1.0 ({contact})"))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            client,
            last_request: Mutex::new(Instant::now() - Duration::from_secs(2)),
        })
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        app: &AppHandle,
        url: Url,
        refresh: bool,
    ) -> Result<T, String> {
        let key = url.as_str().to_string();
        let cache = cache_file(app, &key)?;
        if !refresh {
            if let Some(bytes) = read_cache(&cache) {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    return Ok(value);
                }
            }
        }

        let mut delay = 1;
        for attempt in 0..3 {
            {
                let mut last = self.last_request.lock().await;
                let elapsed = last.elapsed();
                if elapsed < Duration::from_secs(1) {
                    sleep(Duration::from_secs(1) - elapsed).await;
                }
                *last = Instant::now();
            }
            let response = self
                .client
                .get(url.clone())
                .send()
                .await
                .map_err(|error| format!("MusicBrainz request failed: {error}"))?;
            if (response.status() == StatusCode::SERVICE_UNAVAILABLE
                || response.status() == StatusCode::TOO_MANY_REQUESTS)
                && attempt < 2
            {
                sleep(Duration::from_secs(delay)).await;
                delay *= 2;
                continue;
            }
            if !response.status().is_success() {
                return Err(format!("MusicBrainz returned HTTP {}", response.status()));
            }
            let bytes = response.bytes().await.map_err(|error| error.to_string())?;
            let parsed = serde_json::from_slice(&bytes)
                .map_err(|error| format!("MusicBrainz returned invalid data: {error}"))?;
            let _ = fs::write(cache, &bytes);
            return Ok(parsed);
        }
        Err("MusicBrainz is temporarily unavailable after three retries".into())
    }

    pub async fn search(
        &self,
        app: &AppHandle,
        request: SearchRequest,
    ) -> Result<Vec<ReleaseCandidate>, String> {
        if let Some(value) = request
            .release_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let id = extract_mbid(value)
                .ok_or_else(|| "Enter a valid MusicBrainz release URL or MBID".to_string())?;
            let release = self.load_raw(app, &id, request.refresh).await?;
            return Ok(vec![candidate(release)]);
        }
        if request.artist.trim().is_empty() || request.release.trim().is_empty() {
            return Err("Artist and release are required unless an MBID is supplied".into());
        }
        let mut query = format!(
            "artist:\"{}\" AND release:\"{}\"",
            lucene_phrase(request.artist.trim()),
            lucene_phrase(request.release.trim())
        );
        if request.media != "All" {
            query.push_str(&format!(
                " AND format:\"{}\"",
                lucene_phrase(&request.media)
            ));
        }
        let mut url = Url::parse("https://musicbrainz.org/ws/2/release/").unwrap();
        url.query_pairs_mut()
            .append_pair("query", &query)
            .append_pair("fmt", "json")
            .append_pair("limit", "25");
        let response: SearchResponse = self.get_json(app, url, request.refresh).await?;
        let mut releases: Vec<_> = response.releases.into_iter().map(candidate).collect();
        let media_filter = request.media.clone();
        let count = request.chapter_count as i64;
        releases.sort_by_key(|release| {
            let matching_media: Vec<_> = release
                .media
                .iter()
                .filter(|medium| {
                    media_filter == "All" || medium.format.as_deref() == Some(&media_filter)
                })
                .collect();
            let media_penalty = usize::from(matching_media.is_empty());
            let count_delta = matching_media
                .iter()
                .map(|medium| (medium.track_count as i64 - count).unsigned_abs())
                .min()
                .unwrap_or(u64::MAX);
            (media_penalty, count_delta, std::cmp::Reverse(release.score))
        });
        Ok(releases)
    }

    async fn load_raw(
        &self,
        app: &AppHandle,
        id: &str,
        refresh: bool,
    ) -> Result<MbRelease, String> {
        let mut url = Url::parse(&format!("https://musicbrainz.org/ws/2/release/{id}")).unwrap();
        url.query_pairs_mut()
            .append_pair("inc", "recordings+artist-credits+release-groups+media")
            .append_pair("fmt", "json");
        self.get_json(app, url, refresh).await
    }

    pub async fn load_release(
        &self,
        app: &AppHandle,
        id: &str,
        refresh: bool,
    ) -> Result<ReleaseDetails, String> {
        let id = extract_mbid(id).ok_or_else(|| "Invalid MusicBrainz release MBID".to_string())?;
        Ok(details(self.load_raw(app, &id, refresh).await?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_mbid_from_url() {
        assert_eq!(
            extract_mbid("https://musicbrainz.org/release/5c700b4e-949e-40a4-a363-7841d8195a50"),
            Some("5c700b4e-949e-40a4-a363-7841d8195a50".into())
        );
    }

    #[test]
    fn escapes_lucene_phrases() {
        assert_eq!(lucene_phrase("AC/DC \"Live\""), "AC/DC \\\"Live\\\"");
    }

    #[test]
    fn parses_multi_medium_release_and_recordings() {
        let release: MbRelease = serde_json::from_str(
            r#"{
              "id":"5c700b4e-949e-40a4-a363-7841d8195a50",
              "title":"Live at Home",
              "date":"2025-01-02",
              "country":"GB",
              "status":"Official",
              "score":97,
              "artist-credit":[{"name":"Artist A","joinphrase":" & "},{"name":"Artist B","joinphrase":""}],
              "media":[
                {"id":"disc-1","position":1,"format":"Blu-ray","track-count":2,"tracks":[
                  {"position":1,"number":"1","title":"Intro","length":61000,"recording":{"id":"recording-1"}},
                  {"position":2,"number":"2","title":"Song","length":240000,"recording":{"id":"recording-2"}}
                ]},
                {"id":"disc-2","position":2,"title":"Encore","format":"DVD","track-count":1,"tracks":[
                  {"position":1,"number":"1","title":"Encore","length":180000}
                ]}
              ]
            }"#,
        )
        .unwrap();
        let summary = candidate(release.clone());
        assert_eq!(summary.artist_credit, "Artist A & Artist B");
        assert_eq!(summary.score, 97);
        assert_eq!(summary.media.len(), 2);
        assert_eq!(summary.media[0].track_count, 2);

        let loaded = details(release);
        assert_eq!(loaded.media[0].tracks[1].title, "Song");
        assert_eq!(
            loaded.media[0].tracks[0].recording_id.as_deref(),
            Some("recording-1")
        );
        assert_eq!(loaded.media[1].title.as_deref(), Some("Encore"));
    }

    #[test]
    fn rejects_values_without_a_release_mbid() {
        assert_eq!(
            extract_mbid("https://musicbrainz.org/release/not-an-id"),
            None
        );
        assert_eq!(extract_mbid(""), None);
    }
}
