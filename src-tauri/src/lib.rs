pub mod chapter_xml;
mod commands;
pub mod media;
pub mod models;
mod musicbrainz;
mod timecode;
pub mod tools;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use tauri::Manager;

use musicbrainz::MusicBrainzClient;

#[derive(Clone)]
pub struct AppState {
    tool_directory: Arc<Mutex<Option<PathBuf>>>,
    cancel_save: Arc<AtomicBool>,
    musicbrainz: Arc<MusicBrainzClient>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    tool_directory: Option<String>,
}

fn load_tool_directory(app: &tauri::App) -> Option<PathBuf> {
    let path = app.path().app_config_dir().ok()?.join("settings.json");
    let bytes = fs::read(path).ok()?;
    let settings: Settings = serde_json::from_slice(&bytes).ok()?;
    settings.tool_directory.map(PathBuf::from)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = AppState {
                tool_directory: Arc::new(Mutex::new(load_tool_directory(app))),
                cancel_save: Arc::new(AtomicBool::new(false)),
                musicbrainz: Arc::new(MusicBrainzClient::new()?),
            };
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::probe_tools,
            commands::configure_tools_path,
            commands::open_mkv,
            commands::import_chapter_xml,
            commands::export_chapter_xml,
            commands::apply_track_mapping,
            commands::search_releases,
            commands::load_release,
            commands::save_chapters,
            commands::cancel_save,
            commands::restore_recovery,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Music Chapter Editor");
}
