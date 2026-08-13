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
    pub position: u32,
    pub number: String,
    pub title: String,
    pub length_ms: Option<u64>,
    pub recording_id: Option<String>,
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
    pub media: Vec<MediumTracklist>,
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
