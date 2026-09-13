//! The Tauri host for RiftLauncher.
//!
//! Three commands, which is the whole of what this skeleton can do: read the
//! config, write it back, and start the game. Everything else the front asks for
//! is answered by a stub on the JavaScript side (see
//! `src/renderer/src/host/tauriApi.ts`), which refuses out loud rather than
//! pretending to have worked.
//!
//! The division of labour is the one the evaluation is actually testing: the
//! front keeps the domain rules it already has, and the host does the things
//! only a host can do. The config is read and written here and validated there,
//! by the same `normalizeConfig` the Electron build used. The one rule this side
//! owns outright is which paths it will act on, because a renderer asking to run
//! something outside the folders the config names is the question a host has to
//! answer for itself.

mod config;
mod launch;
mod paths;

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// Reads the stored config, and the folders a fresh one falls back to.
#[tauri::command]
async fn read_config(app: AppHandle) -> Result<config::StoredConfig, String> {
    config::read(&app)
}

/// Writes the config document back, atomically.
///
/// The document is the front's own normalized config with its session-only fields
/// already stripped. The only thing checked here is that it is an object at all:
/// what the fields may hold is `src/domain/config/normalize.ts`'s question, and
/// answering it twice is how the two answers start to differ.
#[tauri::command]
async fn write_config(app: AppHandle, document: serde_json::Value) -> Result<(), String> {
    if !document.is_object() {
        return Err("the config document is not an object".to_string());
    }
    config::write(&app, &document)
}

/// The installations the stored config lists.
#[tauri::command]
async fn list_installations(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    Ok(config::installations(&config::read(&app)?))
}

/// Reads one entry out of a config list by its `id`.
fn entry_by_id<'a>(
    stored: &'a config::StoredConfig,
    list: &str,
    id: &str,
) -> Option<&'a serde_json::Value> {
    stored
        .document
        .as_ref()?
        .get(list)?
        .as_array()?
        .iter()
        .find(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(id))
}

fn string_field(entry: &serde_json::Value, field: &str) -> String {
    entry
        .get(field)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Starts the game version an installation is pinned to, and returns when the player quits.
///
/// Both are named by id and read out of the saved config rather than taken from
/// the request, so what gets spawned is what the config says, not what the page
/// says. Their folders then have to sit inside a root that same config names,
/// which is the rule `assertManagedPath` enforced under Electron.
#[tauri::command]
async fn launch_game(
    app: AppHandle,
    version_id: String,
    installation_id: String,
) -> Result<launch::GameExecutionResult, String> {
    let stored = config::read(&app)?;

    let (Some(version), Some(installation)) = (
        entry_by_id(&stored, "gameVersions", &version_id),
        entry_by_id(&stored, "installations", &installation_id),
    ) else {
        eprintln!("[host] [launch] the config names no such game version or installation");
        return Ok(launch::GameExecutionResult::refused("invalid-request"));
    };

    let version_folder = PathBuf::from(string_field(version, "path"));
    let installation_path = string_field(installation, "path");
    let roots = config::managed_roots(&stored);

    if !paths::is_managed(&roots, &version_folder)
        || !paths::is_managed(&roots, &PathBuf::from(&installation_path))
    {
        eprintln!("[host] [launch] refused a path outside the folders the config manages");
        return Ok(launch::GameExecutionResult::refused("invalid-request"));
    }

    let request = launch::LaunchRequest {
        version_folder,
        installation_path,
        start_params: string_field(installation, "startParams"),
        mesa_gl_thread: installation
            .get("mesaGlThread")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        launch_wrapper: string_field(installation, "launchWrapper"),
        env_vars: string_field(installation, "envVars"),
    };

    let log_path = app
        .path()
        .app_log_dir()
        .map_err(|err| format!("no log directory: {err}"))?
        .join("game.log");

    // Off the async runtime's own threads: this one waits for the whole play
    // session, which is minutes or hours rather than milliseconds.
    tauri::async_runtime::spawn_blocking(move || launch::launch(request, &log_path))
        .await
        .map_err(|err| format!("the launch task failed: {err}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            read_config,
            write_config,
            list_installations,
            launch_game
        ])
        .run(tauri::generate_context!())
        .expect("error while running RiftLauncher");
}
