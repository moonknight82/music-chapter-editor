use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::tempdir;

use crate::chapter_xml::parse_chapter_xml;
use crate::models::{ChapterDocument, Edition, FileIdentity};
use crate::tools::ToolPaths;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

pub fn file_identity(path: &Path) -> Result<FileIdentity, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("Could not inspect the MKV: {error}"))?;
    #[cfg(unix)]
    {
        let modified_ns =
            (metadata.mtime() as i128 * 1_000_000_000i128) + metadata.mtime_nsec() as i128;
        Ok(FileIdentity {
            device: metadata.dev().to_string(),
            inode: metadata.ino().to_string(),
            size: metadata.len(),
            modified_ns: modified_ns.to_string(),
        })
    }
    #[cfg(not(unix))]
    {
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos().to_string())
            .unwrap_or_default();
        Ok(FileIdentity {
            device: String::new(),
            inode: String::new(),
            size: metadata.len(),
            modified_ns,
        })
    }
}

fn duration_from_identification(json: &Value) -> u64 {
    let duration = json.pointer("/container/properties/duration");
    duration
        .and_then(Value::as_u64)
        .or_else(|| {
            duration
                .and_then(Value::as_f64)
                .map(|value| value.max(0.0) as u64)
        })
        .unwrap_or(0)
}

fn infer_names(path: &Path) -> (String, String) {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    match stem.split_once(" - ") {
        Some((artist, release)) => (artist.trim().into(), release.trim().into()),
        None => (String::new(), stem.trim().into()),
    }
}

fn empty_edition() -> Edition {
    Edition {
        uid: None,
        hidden: false,
        default: true,
        ordered: false,
        chapters: Vec::new(),
        source_index: None,
    }
}

pub fn open_document(path: &Path, tools: &ToolPaths) -> Result<ChapterDocument, String> {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("mkv"))
        != Some(true)
    {
        return Err("Version 0.1 supports MKV files only".into());
    }
    let identity = file_identity(path)?;
    let identify = tools.run("mkvmerge", ["-J".as_ref(), path.as_os_str()])?;
    if !identify.status.success() {
        return Err(format!(
            "mkvmerge could not read this file: {}",
            String::from_utf8_lossy(&identify.stderr).trim()
        ));
    }
    let identification: Value = serde_json::from_slice(&identify.stdout)
        .map_err(|error| format!("mkvmerge returned invalid JSON: {error}"))?;
    let duration_ns = duration_from_identification(&identification);

    let directory = tempdir().map_err(|error| error.to_string())?;
    let xml_path = directory.path().join("chapters.xml");
    let extract = tools.run(
        "mkvextract",
        [path.as_os_str(), "chapters".as_ref(), xml_path.as_os_str()],
    )?;
    if extract.status.code() == Some(2) {
        return Err(format!(
            "mkvextract could not read chapters: {}",
            String::from_utf8_lossy(&extract.stderr).trim()
        ));
    }

    let (editions, advanced, advanced_reason, original_xml) = if xml_path.is_file() {
        let xml = fs::read_to_string(&xml_path)
            .map_err(|error| format!("Could not read extracted chapters: {error}"))?;
        let parsed = parse_chapter_xml(&xml)?;
        (
            parsed.editions,
            parsed.advanced,
            parsed.advanced_reason,
            Some(xml),
        )
    } else {
        (vec![empty_edition()], false, None, None)
    };
    let active_edition = editions
        .iter()
        .position(|edition| edition.default)
        .unwrap_or(0);
    let (inferred_artist, inferred_release) = infer_names(path);
    Ok(ChapterDocument {
        path: path.display().to_string(),
        duration_ns,
        file_identity: identity,
        editions,
        active_edition,
        advanced,
        advanced_reason,
        original_xml,
        inferred_artist,
        inferred_release,
        musicbrainz_selection: None,
    })
}
