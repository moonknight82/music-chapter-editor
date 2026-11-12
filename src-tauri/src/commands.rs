use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use chrono::Local;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};
use tempfile::tempdir;

use crate::AppState;
use crate::chapter_xml::{
    parse_chapter_xml, render_chapter_xml, semantically_equal, validate_editions,
};
use crate::media::{file_identity, open_document};
use crate::models::{
    ChapterDocument, FlacExtractionPlan, FlacExtractionRequest, FlacExtractionResult, FlacProbe,
    ImportedChapters, ReleaseCandidate, ReleaseDetails, SaveMode, SaveProgress, SaveRequest,
    SaveVerification, SearchRequest, ToolStatus,
};
use crate::tools::{ToolPaths, locate_tools, status};

fn configured_tools(state: &AppState) -> Result<ToolPaths, String> {
    let configured = state.tool_directory.lock().unwrap().clone();
    locate_tools(configured.as_deref()).map_err(|status| status.message)
}

fn write_settings(app: &AppHandle, directory: Option<&Path>) -> Result<(), String> {
    let config = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&config).map_err(|error| error.to_string())?;
    let body = json!({ "toolDirectory": directory.map(|path| path.display().to_string()) });
    fs::write(
        config.join("settings.json"),
        serde_json::to_vec_pretty(&body).unwrap(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn probe_tools(custom_path: Option<String>, state: State<'_, AppState>) -> ToolStatus {
    let configured = custom_path
        .map(PathBuf::from)
        .or_else(|| state.tool_directory.lock().unwrap().clone());
    status(configured.as_deref())
}

#[tauri::command]
pub fn configure_tools_path(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ToolStatus, String> {
    let directory = PathBuf::from(path);
    let tools = locate_tools(Some(&directory)).map_err(|status| status.message)?;
    *state.tool_directory.lock().unwrap() = Some(tools.directory.clone());
    write_settings(&app, Some(&tools.directory))?;
    Ok(status(Some(&tools.directory)))
}

#[tauri::command]
pub fn open_mkv(path: String, state: State<'_, AppState>) -> Result<ChapterDocument, String> {
    open_document(Path::new(&path), &configured_tools(&state)?)
}

#[tauri::command]
pub fn import_chapter_xml(path: String) -> Result<ImportedChapters, String> {
    let xml = fs::read_to_string(path)
        .map_err(|error| format!("Could not read the XML file: {error}"))?;
    parse_chapter_xml(&xml)
}

#[tauri::command]
pub fn export_chapter_xml(mut document: ChapterDocument, path: String) -> Result<(), String> {
    let xml = render_chapter_xml(&mut document.editions, document.original_xml.as_deref())?;
    fs::write(path, xml).map_err(|error| format!("Could not export chapter XML: {error}"))
}

#[tauri::command]
pub fn apply_track_mapping(
    mut document: ChapterDocument,
    edition_index: usize,
    titles: Vec<Option<String>>,
) -> Result<ChapterDocument, String> {
    let edition = document
        .editions
        .get_mut(edition_index)
        .ok_or_else(|| "The selected edition no longer exists".to_string())?;
    if titles.len() != edition.chapters.len() {
        return Err("The mapping must contain one entry per chapter".into());
    }
    for (chapter, title) in edition.chapters.iter_mut().zip(titles) {
        if let Some(title) = title {
            chapter.title = title;
        }
    }
    Ok(document)
}

#[tauri::command]
pub async fn search_releases(
    request: SearchRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ReleaseCandidate>, String> {
    state.musicbrainz.search(&app, request).await
}

#[tauri::command]
pub async fn load_release(
    id: String,
    refresh: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ReleaseDetails, String> {
    state
        .musicbrainz
        .load_release(&app, &id, refresh.unwrap_or(false))
        .await
}

fn same_identity(path: &Path, expected: &crate::models::FileIdentity) -> Result<(), String> {
    let current = file_identity(path)?;
    if &current != expected {
        return Err("The source MKV changed after it was opened. Reload it before saving.".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn try_clone(source: &Path, destination: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(source) = CString::new(source.as_os_str().as_bytes()) else {
        return false;
    };
    let Ok(destination) = CString::new(destination.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::clonefile(source.as_ptr(), destination.as_ptr(), 0) == 0 }
}

#[cfg(not(target_os = "macos"))]
fn try_clone(_source: &Path, _destination: &Path) -> bool {
    false
}

fn copy_with_progress(
    source: &Path,
    destination: &Path,
    app: &AppHandle,
    state: &AppState,
) -> Result<(), String> {
    if destination.exists() {
        return Err("The destination already exists. Choose a new filename.".into());
    }
    let result = (|| {
        let total = fs::metadata(source)
            .map_err(|error| error.to_string())?
            .len();
        if try_clone(source, destination) {
            #[cfg(unix)]
            {
                let metadata = fs::metadata(destination).map_err(|error| error.to_string())?;
                let mut permissions = metadata.permissions();
                permissions.set_mode(permissions.mode() | 0o200);
                fs::set_permissions(destination, permissions).map_err(|error| error.to_string())?;
            }
            let _ = app.emit(
                "save-progress",
                SaveProgress {
                    phase: "copying".into(),
                    completed: total,
                    total,
                },
            );
            return Ok(());
        }
        let mut input =
            File::open(source).map_err(|error| format!("Could not open the source: {error}"))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|error| format!("Could not create the destination: {error}"))?;
        let mut buffer = vec![0u8; 8 * 1024 * 1024];
        let mut completed = 0;
        loop {
            if state.cancel_save.load(Ordering::Relaxed) {
                return Err("Save cancelled".into());
            }
            let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            completed += read as u64;
            let _ = app.emit(
                "save-progress",
                SaveProgress {
                    phase: "copying".into(),
                    completed,
                    total,
                },
            );
        }
        output.sync_all().map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn backup_original(document: &ChapterDocument) -> Result<PathBuf, String> {
    let source = Path::new(&document.path);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("chapters");
    let timestamp = Local::now().format("%Y%m%d-%H%M%S-%3f");
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    if let Some(xml) = document.original_xml.as_deref() {
        let path = parent.join(format!("{stem}.chapters-backup-{timestamp}.xml"));
        fs::write(&path, xml)
            .map_err(|error| format!("Could not create the recovery XML: {error}"))?;
        Ok(path)
    } else {
        let path = parent.join(format!("{stem}.chapters-backup-{timestamp}.json"));
        let marker = json!({ "hadChapters": false, "source": document.path });
        fs::write(&path, serde_json::to_vec_pretty(&marker).unwrap())
            .map_err(|error| format!("Could not create the recovery marker: {error}"))?;
        Ok(path)
    }
}

fn run_chapter_write(
    tools: &ToolPaths,
    target: &Path,
    xml: Option<&Path>,
) -> Result<Vec<String>, String> {
    let chapter_argument = xml
        .map(|path| path.as_os_str())
        .unwrap_or_else(|| "".as_ref());
    let output = tools.run(
        "mkvpropedit",
        [target.as_os_str(), "--chapters".as_ref(), chapter_argument],
    )?;
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .trim()
    .to_string();
    match output.status.code() {
        Some(0) => Ok(Vec::new()),
        Some(1) => Ok(vec![if message.is_empty() {
            "MKVToolNix completed with a warning".into()
        } else {
            message
        }]),
        _ => Err(if message.is_empty() {
            "mkvpropedit could not update the file".into()
        } else {
            message
        }),
    }
}

fn has_chapters(document: &ChapterDocument) -> bool {
    document
        .editions
        .iter()
        .any(|edition| !edition.chapters.is_empty())
}

fn perform_save(
    mut request: SaveRequest,
    app: AppHandle,
    state: &AppState,
    tools: ToolPaths,
) -> Result<SaveVerification, String> {
    if request.document.advanced {
        return Err(
            "Advanced ordered, linked, or nested chapters are read-only in version 0.1".into(),
        );
    }
    let mut warnings = validate_editions(&request.document.editions, request.document.duration_ns)?;
    let source = PathBuf::from(&request.document.path);
    same_identity(&source, &request.document.file_identity)?;
    state.cancel_save.store(false, Ordering::Relaxed);

    let (target, recovery, app_created_copy) = match &request.mode {
        SaveMode::Copy { destination } => {
            let destination = PathBuf::from(destination);
            if destination
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("mkv"))
                != Some(true)
            {
                return Err("The destination must use the .mkv extension".into());
            }
            copy_with_progress(&source, &destination, &app, state)?;
            (destination, None, true)
        }
        SaveMode::InPlace => (
            source.clone(),
            Some(backup_original(&request.document)?),
            false,
        ),
    };

    let result = (|| {
        let directory = tempdir().map_err(|error| error.to_string())?;
        let xml_path = directory.path().join("chapters.xml");
        let xml = if has_chapters(&request.document) {
            let xml = render_chapter_xml(
                &mut request.document.editions,
                request.document.original_xml.as_deref(),
            )?;
            fs::write(&xml_path, xml).map_err(|error| error.to_string())?;
            Some(xml_path.as_path())
        } else {
            None
        };
        let _ = app.emit(
            "save-progress",
            SaveProgress {
                phase: "writing".into(),
                completed: 0,
                total: 1,
            },
        );
        warnings.extend(run_chapter_write(&tools, &target, xml)?);
        let verified = open_document(&target, &tools)?;
        let mut verified = verified;
        verified.musicbrainz_selection = request.document.musicbrainz_selection.clone();
        let matches = if has_chapters(&request.document) {
            semantically_equal(&request.document.editions, &verified.editions)
        } else {
            !has_chapters(&verified)
        };
        if !matches {
            return Err("Post-save verification did not match the requested chapters".into());
        }
        let _ = app.emit(
            "save-progress",
            SaveProgress {
                phase: "verified".into(),
                completed: 1,
                total: 1,
            },
        );
        Ok(SaveVerification {
            target_path: target.display().to_string(),
            recovery_path: recovery.as_ref().map(|path| path.display().to_string()),
            warnings,
            verified: true,
            document: verified,
        })
    })();
    if result.is_err() && app_created_copy {
        let _ = fs::remove_file(&target);
    }
    result
}

#[tauri::command]
pub async fn save_chapters(
    request: SaveRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SaveVerification, String> {
    let tools = configured_tools(&state)?;
    let app_for_task = app.clone();
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        perform_save(request, app_for_task, &state_inner, tools)
    })
    .await
    .map_err(|error| format!("Save task failed: {error}"))?
}

#[tauri::command]
pub fn cancel_save(state: State<'_, AppState>) {
    state.cancel_save.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub async fn probe_flac(path: String, state: State<'_, AppState>) -> Result<FlacProbe, String> {
    let configured = state.tool_directory.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::flac::probe(Path::new(&path), configured.as_deref())
    })
    .await
    .map_err(|error| format!("Audio probe task failed: {error}"))?
}

#[tauri::command]
pub fn preview_flac_extraction(
    request: FlacExtractionRequest,
) -> Result<FlacExtractionPlan, String> {
    crate::flac::plan(&request)
}

#[tauri::command]
pub async fn extract_flac(
    request: FlacExtractionRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<FlacExtractionResult, String> {
    let configured = state.tool_directory.lock().unwrap().clone();
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::flac::extract(
            request,
            configured.as_deref(),
            &app,
            &state_inner.cancel_flac,
        )
    })
    .await
    .map_err(|error| format!("FLAC extraction task failed: {error}"))?
}

#[tauri::command]
pub fn cancel_flac(state: State<'_, AppState>) {
    state.cancel_flac.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn restore_recovery(
    target_path: String,
    recovery_path: String,
    state: State<'_, AppState>,
) -> Result<ChapterDocument, String> {
    let tools = configured_tools(&state)?;
    let target = PathBuf::from(target_path);
    let recovery = PathBuf::from(recovery_path);
    let restored = if recovery.extension().and_then(|value| value.to_str()) == Some("xml") {
        let xml = fs::read_to_string(&recovery).map_err(|error| error.to_string())?;
        let expected = parse_chapter_xml(&xml)?;
        run_chapter_write(&tools, &target, Some(&recovery))?;
        let restored = open_document(&target, &tools)?;
        if !semantically_equal(&expected.editions, &restored.editions) {
            return Err("Recovery verification did not match the backup XML".into());
        }
        restored
    } else {
        let marker: serde_json::Value =
            serde_json::from_slice(&fs::read(&recovery).map_err(|error| error.to_string())?)
                .map_err(|error| format!("Invalid recovery marker: {error}"))?;
        if marker.get("hadChapters").and_then(|value| value.as_bool()) != Some(false) {
            return Err("This recovery marker is not recognized".into());
        }
        run_chapter_write(&tools, &target, None)?;
        let restored = open_document(&target, &tools)?;
        if has_chapters(&restored) {
            return Err(
                "Recovery verification found chapters after restoring a chapterless file".into(),
            );
        }
        restored
    };
    Ok(restored)
}
