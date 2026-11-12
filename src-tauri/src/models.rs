use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileIdentity {
    pub device: String,
    pub inode: String,
    pub size: u64,
    pub modified_ns: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChapterDisplay {
    pub title: String,
    pub language: String,
    pub language_ietf: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChapterEntry {
    pub uid: Option<String>,
    pub start_ns: u64,
    pub end_ns: Option<u64>,
    pub title: String,
    pub displays: Vec<ChapterDisplay>,
    pub source_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Edition {
    pub uid: Option<String>,
    pub hidden: bool,
    pub default: bool,
    pub ordered: bool,
    pub chapters: Vec<ChapterEntry>,
    pub source_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterDocument {
    pub path: String,
    pub duration_ns: u64,
    pub file_identity: FileIdentity,
    pub editions: Vec<Edition>,
    pub active_edition: usize,
    pub advanced: bool,
    pub advanced_reason: Option<String>,
    pub original_xml: Option<String>,
    pub inferred_artist: String,
    pub inferred_release: String,
    #[serde(default)]
    pub musicbrainz_selection: Option<MusicBrainzSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicBrainzTrackMetadata {
    pub track_id: Option<String>,
    pub recording_id: Option<String>,
    pub position: u32,
    pub number: String,
    pub title: String,
    pub artist_credit: String,
    #[serde(default)]
    pub artist_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicBrainzSelection {
    pub release_id: String,
    pub release_group_id: Option<String>,
    pub release_title: String,
    pub album_artist: String,
    #[serde(default)]
    pub album_artist_ids: Vec<String>,
    pub release_date: Option<String>,
    pub original_year: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub medium_position: u32,
    pub medium_title: Option<String>,
    pub medium_track_count: u32,
    pub total_media: u32,
    pub chapter_tracks: Vec<Option<MusicBrainzTrackMetadata>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedChapters {
    pub editions: Vec<Edition>,
    pub advanced: bool,
    pub advanced_reason: Option<String>,
    pub original_xml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub available: bool,
    pub directory: Option<String>,
    pub version: Option<String>,
    pub major_version: Option<u32>,
    pub missing: Vec<String>,
    pub searched: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediumSummary {
    pub id: Option<String>,
    pub position: u32,
    pub title: Option<String>,
    pub format: Option<String>,
    pub track_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseCandidate {
    pub id: String,
    pub title: String,
    pub artist_credit: String,
    pub date: Option<String>,
    pub country: Option<String>,
    pub status: Option<String>,
    pub score: u32,
    pub media: Vec<MediumSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub id: Option<String>,
    pub position: u32,
    pub number: String,
    pub title: String,
    pub length_ms: Option<u64>,
    pub recording_id: Option<String>,
    pub artist_credit: String,
    #[serde(default)]
    pub artist_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediumTracklist {
    pub id: Option<String>,
    pub position: u32,
    pub title: Option<String>,
    pub format: Option<String>,
    pub tracks: Vec<TrackInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDetails {
    pub id: String,
    pub title: String,
    pub artist_credit: String,
    #[serde(default)]
    pub artist_ids: Vec<String>,
    pub date: Option<String>,
    pub original_year: Option<String>,
    pub release_group_id: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub media: Vec<MediumTracklist>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStream {
    pub index: u32,
    pub codec: String,
    pub codec_long_name: String,
    pub title: Option<String>,
    pub language: Option<String>,
    pub channels: u32,
    pub channel_layout: Option<String>,
    pub sample_rate: u32,
    pub bit_depth: u32,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlacProbe {
    pub available: bool,
    pub ffmpeg_path: Option<String>,
    pub version: Option<String>,
    pub streams: Vec<AudioStream>,
    pub searched: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlacFallbackMetadata {
    pub album_artist: String,
    pub album: String,
    pub release_year: String,
    pub original_year: String,
    pub label: String,
    pub catalog_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlacExtractionRequest {
    pub document: ChapterDocument,
    pub stream: AudioStream,
    pub output_parent: String,
    pub fallback: FlacFallbackMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlacTrackPreview {
    pub chapter_index: usize,
    pub relative_path: String,
    pub title: String,
    pub artist: String,
    pub track_number: String,
    pub start_ns: u64,
    pub end_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlacExtractionPlan {
    pub album_directory: String,
    pub uses_musicbrainz: bool,
    pub tracks: Vec<FlacTrackPreview>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlacProgress {
    pub phase: String,
    pub completed: usize,
    pub total: usize,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlacExtractionResult {
    pub output_directory: String,
    pub files: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub artist: String,
    pub release: String,
    pub media: String,
    pub release_id: Option<String>,
    pub chapter_count: usize,
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SaveMode {
    Copy { destination: String },
    InPlace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRequest {
    pub document: ChapterDocument,
    pub mode: SaveMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveVerification {
    pub target_path: String,
    pub recovery_path: Option<String>,
    pub warnings: Vec<String>,
    pub verified: bool,
    pub document: ChapterDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProgress {
    pub phase: String,
    pub completed: u64,
    pub total: u64,
}
