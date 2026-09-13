//! The config file, and nothing else.
//!
//! This host does file IO for the config and leaves every question about what
//! the document may contain to `src/domain/config/normalize.ts`, which is the
//! same validation the Electron host ran and is now shared with the front. Two
//! normalizers would be one too many: the one that drifts is the one that stops
//! agreeing with the file on disk.
//!
//! The file is `config.json` in the application config directory, the same name
//! the launcher has always used.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager};

pub const CONFIG_FILE_NAME: &str = "config.json";

/// The three roots a fresh config falls back to.
///
/// They hang off the OS config directory, which is what Electron called
/// `appData`, and they keep the folder names the launcher already writes so a
/// config written by the Electron build keeps pointing at the same places.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFolders {
    pub default_installations_folder: String,
    pub default_versions_folder: String,
    pub backups_folder: String,
}

/// What `read_config` answers: the stored document when there is a usable one,
/// and the fallback folders either way.
///
/// `document` is `None` for a config that is not there and for one that does not
/// parse. The front reads both the same way, as "nothing usable on disk", and
/// normalizes its way to a default config; which of the two it was is said in
/// the log rather than in the type, because nothing on the front does anything
/// different with the answer.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StoredConfig {
    pub document: Option<serde_json::Value>,
    pub folders: ConfigFolders,
}

fn config_folders(app: &AppHandle) -> Result<ConfigFolders, String> {
    let base = app
        .path()
        .config_dir()
        .map_err(|err| format!("no config directory: {err}"))?;

    let named = |name: &str| base.join(name).to_string_lossy().into_owned();

    Ok(ConfigFolders {
        default_installations_folder: named("RiftLauncherInstallations"),
        default_versions_folder: named("RiftLauncherGameVersions"),
        backups_folder: named("RiftLauncherBackups"),
    })
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("no app config directory: {err}"))?;
    Ok(directory.join(CONFIG_FILE_NAME))
}

/// Reads `config.json`, or reports that there is nothing usable to read.
pub fn read(app: &AppHandle) -> Result<StoredConfig, String> {
    let folders = config_folders(app)?;
    let path = config_path(app)?;

    let document = match fs::read_to_string(&path) {
        Ok(text) => {
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(value) => Some(value),
                Err(err) => {
                    eprintln!("[host] [config] config.json does not parse, falling back to defaults: {err}");
                    None
                }
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(format!("could not read the config: {err}")),
    };

    Ok(StoredConfig { document, folders })
}

/// Writes `config.json` so a reader sees either the old document or the new one.
///
/// The temporary file is written in the destination's own directory, which is
/// what makes the rename an atomic replace rather than a copy across a
/// filesystem boundary. The data is flushed to the disk before the rename, so a
/// machine that loses power mid-write leaves the previous config intact rather
/// than an empty file where the config used to be.
pub fn write(app: &AppHandle, document: &serde_json::Value) -> Result<(), String> {
    let path = config_path(app)?;
    let directory = path
        .parent()
        .ok_or_else(|| "the config path has no directory".to_string())?;
    fs::create_dir_all(directory)
        .map_err(|err| format!("could not create the config directory: {err}"))?;

    let serialized = serde_json::to_vec_pretty(document)
        .map_err(|err| format!("could not serialize the config: {err}"))?;

    let temporary = directory.join(format!("{CONFIG_FILE_NAME}.{}.tmp", std::process::id()));
    let mut file = fs::File::create(&temporary)
        .map_err(|err| format!("could not open the config for writing: {err}"))?;
    let written = file
        .write_all(&serialized)
        .and_then(|()| file.sync_all())
        .map_err(|err| format!("could not write the config: {err}"));
    drop(file);

    if let Err(err) = written {
        let _ = fs::remove_file(&temporary);
        return Err(err);
    }

    fs::rename(&temporary, &path).map_err(|err| {
        let _ = fs::remove_file(&temporary);
        format!("could not replace the config: {err}")
    })
}

/// The installations the stored config lists, in the order it lists them.
pub fn installations(stored: &StoredConfig) -> Vec<serde_json::Value> {
    stored
        .document
        .as_ref()
        .and_then(|document| document.get("installations"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Every root the stored config names: the three configured folders, plus the
/// folder of every installation and every game version it lists.
pub fn managed_roots(stored: &StoredConfig) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from(&stored.folders.default_installations_folder),
        PathBuf::from(&stored.folders.default_versions_folder),
        PathBuf::from(&stored.folders.backups_folder),
    ];

    let document = match stored.document.as_ref() {
        Some(document) => document,
        None => return roots,
    };

    for field in [
        "defaultInstallationsFolder",
        "defaultVersionsFolder",
        "backupsFolder",
    ] {
        if let Some(folder) = document.get(field).and_then(|value| value.as_str()) {
            roots.push(PathBuf::from(folder));
        }
    }

    for list in ["installations", "gameVersions"] {
        let entries = document.get(list).and_then(|value| value.as_array());
        for entry in entries.into_iter().flatten() {
            if let Some(path) = entry.get("path").and_then(|value| value.as_str()) {
                roots.push(PathBuf::from(path));
            }
        }
    }

    roots.retain(|root| !root.as_os_str().is_empty() && !crate::paths::is_root(root));
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stored(document: serde_json::Value) -> StoredConfig {
        StoredConfig {
            document: Some(document),
            folders: ConfigFolders {
                default_installations_folder: "/home/player/.config/RiftLauncherInstallations"
                    .into(),
                default_versions_folder: "/home/player/.config/RiftLauncherGameVersions".into(),
                backups_folder: "/home/player/.config/RiftLauncherBackups".into(),
            },
        }
    }

    #[test]
    fn the_roots_cover_the_folders_and_every_entry_the_config_names() {
        let config = stored(json!({
            "installations": [{ "path": "/games/survival" }],
            "gameVersions": [{ "path": "/games/vs-1.20.0" }]
        }));

        let roots = managed_roots(&config);
        assert!(roots.contains(&PathBuf::from("/games/survival")));
        assert!(roots.contains(&PathBuf::from("/games/vs-1.20.0")));
        assert!(roots.contains(&PathBuf::from("/home/player/.config/RiftLauncherBackups")));
    }

    #[test]
    fn a_config_naming_the_filesystem_root_grants_nothing_extra() {
        let config = stored(json!({ "installations": [{ "path": "/" }, { "path": "" }] }));
        let roots = managed_roots(&config);
        assert!(!roots.contains(&PathBuf::from("/")));
        assert!(!roots.iter().any(|root| root.as_os_str().is_empty()));
    }

    #[test]
    fn a_document_with_no_installations_lists_none() {
        assert!(installations(&stored(json!({}))).is_empty());
    }
}
