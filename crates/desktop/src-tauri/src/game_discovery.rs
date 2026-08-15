use crate::command_error::CommandError;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ},
};

const HELLDIVERS_2_STEAM_APP_ID: u32 = 553850;
const HELLDIVERS_2_STEAM_DIR_NAME: &str = "Helldivers 2";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDataDiscovery {
    data_dir: Option<PathBuf>,
    candidates: Vec<PathBuf>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SteamAppManifest {
    app_id: u32,
    install_dir: String,
}

/// Find the most likely Helldivers 2 `data` directory without blocking the UI.
#[tauri::command]
pub async fn detect_game_data_dir() -> Result<GameDataDiscovery, CommandError> {
    tauri::async_runtime::spawn_blocking(discover_game_data_dir)
        .await
        .map_err(|error| CommandError::from_display("gameData.discoveryFailed", error))
}

/// Verify that a manually selected directory contains a supported HD2 data layout.
#[tauri::command]
pub async fn validate_game_data_dir(path: PathBuf) -> Result<(), CommandError> {
    tauri::async_runtime::spawn_blocking(move || validate_game_data_dir_blocking(&path))
        .await
        .map_err(|error| CommandError::from_display("task.joinFailed", error))?
        .map_err(|error| CommandError::new("gameData.invalid", error))
}

fn validate_game_data_dir_blocking(path: &Path) -> Result<(), String> {
    if is_valid_game_data_dir(path) {
        return Ok(());
    }
    Err(format!(
        "The selected folder is not a Helldivers 2 data directory: {}",
        path.display()
    ))
}

fn discover_game_data_dir() -> GameDataDiscovery {
    let candidates = valid_data_dirs(candidate_data_dirs());
    GameDataDiscovery {
        data_dir: candidates.first().cloned(),
        candidates,
    }
}

fn candidate_data_dirs() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    collect_steam_manifest_candidates(&mut candidates);
    collect_common_install_candidates(&mut candidates);
    candidates
}

fn collect_steam_manifest_candidates(candidates: &mut Vec<PathBuf>) {
    for library_root in steam_library_roots() {
        let Some(install_root) = steam_app_install_root(&library_root, HELLDIVERS_2_STEAM_APP_ID)
        else {
            continue;
        };
        push_unique_path(candidates, install_root.join("data"));
    }
}

fn collect_common_install_candidates(candidates: &mut Vec<PathBuf>) {
    for library_root in fallback_steam_roots() {
        let install_root = library_root
            .join("steamapps")
            .join("common")
            .join(HELLDIVERS_2_STEAM_DIR_NAME);
        push_unique_path(candidates, install_root.join("data"));
    }
}

fn steam_library_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for steam_root in discovered_steam_roots() {
        push_unique_path(&mut roots, steam_root.clone());
        collect_extra_library_roots(&steam_root, &mut roots);
    }
    roots
}

fn collect_extra_library_roots(steam_root: &Path, roots: &mut Vec<PathBuf>) {
    let path = steam_root.join("steamapps").join("libraryfolders.vdf");
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for root in parse_library_folders(&text) {
        push_unique_path(roots, root);
    }
}

fn steam_app_install_root(library_root: &Path, app_id: u32) -> Option<PathBuf> {
    let manifest = library_root
        .join("steamapps")
        .join(format!("appmanifest_{app_id}.acf"));
    let text = std::fs::read_to_string(manifest).ok()?;
    let manifest = parse_app_manifest(&text)?;
    Some(
        library_root
            .join("steamapps")
            .join("common")
            .join(manifest.install_dir),
    )
}

fn parse_app_manifest(text: &str) -> Option<SteamAppManifest> {
    let values = quoted_pairs_by_key(text);
    Some(SteamAppManifest {
        app_id: values.get("appid")?.parse().ok()?,
        install_dir: values.get("installdir")?.to_string(),
    })
}

fn parse_library_folders(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(quoted_pair)
        .filter(|(key, value)| *key == "path" || is_legacy_library_path(key, value))
        .filter_map(|(_, value)| non_empty_path(value))
        .collect()
}

fn valid_data_dirs(candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    candidates
        .into_iter()
        .filter(|path| is_valid_game_data_dir(path))
        .fold(Vec::new(), push_path_fold)
}

fn is_valid_game_data_dir(path: &Path) -> bool {
    path.is_dir() && (path.join("bundles.nxa").is_file() || path.join("9ba626afa44a3aa3").is_file())
}

#[cfg(windows)]
fn discovered_steam_roots() -> Vec<PathBuf> {
    registry_steam_roots()
        .into_iter()
        .chain(fallback_steam_roots())
        .filter(|path| path.exists())
        .fold(Vec::new(), push_path_fold)
}

#[cfg(not(windows))]
fn discovered_steam_roots() -> Vec<PathBuf> {
    fallback_steam_roots()
        .into_iter()
        .filter(|path| path.exists())
        .fold(Vec::new(), push_path_fold)
}

#[cfg(windows)]
fn registry_steam_roots() -> Vec<PathBuf> {
    [
        (HKEY_CURRENT_USER, r"Software\Valve\Steam"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam"),
    ]
    .into_iter()
    .filter_map(|(hive, key)| registry_steam_root(hive, key))
    .collect()
}

#[cfg(windows)]
fn registry_steam_root(hive: winreg::HKEY, key: &str) -> Option<PathBuf> {
    let key = RegKey::predef(hive)
        .open_subkey_with_flags(key, KEY_READ)
        .ok()?;
    registry_value_path(&key, "InstallPath").or_else(|| registry_value_path(&key, "SteamPath"))
}

#[cfg(windows)]
fn registry_value_path(key: &RegKey, value: &str) -> Option<PathBuf> {
    key.get_value::<String, _>(value)
        .ok()
        .and_then(non_empty_path)
}

fn fallback_steam_roots() -> Vec<PathBuf> {
    ('C'..='H')
        .flat_map(|drive| {
            [
                PathBuf::from(format!("{drive}:\\Program Files (x86)\\Steam")),
                PathBuf::from(format!("{drive}:\\Program Files\\Steam")),
                PathBuf::from(format!("{drive}:\\SteamLibrary")),
            ]
        })
        .collect()
}

fn quoted_pairs_by_key(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(quoted_pair)
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn quoted_pair(line: &str) -> Option<(&str, &str)> {
    let key_start = line.find('"')? + 1;
    let key_end = line[key_start..].find('"')? + key_start;
    let value_start = line[key_end + 1..].find('"')? + key_end + 2;
    let value_end = line[value_start..].find('"')? + value_start;
    Some((&line[key_start..key_end], &line[value_start..value_end]))
}

fn is_legacy_library_path(key: &str, value: &str) -> bool {
    key.parse::<u32>().is_ok() && looks_like_path(value)
}

fn looks_like_path(value: &str) -> bool {
    value.contains(':') || value.contains('\\') || value.contains('/')
}

fn non_empty_path(path: impl AsRef<str>) -> Option<PathBuf> {
    let value = path.as_ref().trim();
    (!value.is_empty()).then(|| PathBuf::from(normalize_path_text(value)))
}

fn push_path_fold(mut paths: Vec<PathBuf>, path: PathBuf) -> Vec<PathBuf> {
    push_unique_path(&mut paths, path);
    paths
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    let path = normalize_path(path);
    if !paths.contains(&path) {
        paths.push(path);
    }
}

fn normalize_path(path: PathBuf) -> PathBuf {
    path.to_str()
        .map(normalize_path_text)
        .map(PathBuf::from)
        .unwrap_or(path)
}

fn normalize_path_text(value: &str) -> String {
    uppercase_windows_drive_letter(&normalize_windows_separators(value.replace("\\\\", "\\")))
}

#[cfg(windows)]
fn normalize_windows_separators(value: String) -> String {
    value.replace('/', "\\")
}

#[cfg(not(windows))]
fn normalize_windows_separators(value: String) -> String {
    value
}

#[cfg(windows)]
fn uppercase_windows_drive_letter(value: &str) -> String {
    let Some((drive, rest)) = value.split_once(':') else {
        return value.to_string();
    };
    if drive.len() != 1 || !drive.as_bytes()[0].is_ascii_alphabetic() {
        return value.to_string();
    }
    format!("{}:{rest}", drive.to_ascii_uppercase())
}

#[cfg(not(windows))]
fn uppercase_windows_drive_letter(value: &str) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_library_folder_paths() {
        let roots = parse_library_folders(
            r#""libraryfolders"
{
    "0"
    {
        "path" "D:\\Program Files (x86)\\Steam"
    }
    "1" "E:\\SteamLibrary"
}"#,
        );

        assert_eq!(
            roots,
            vec![
                PathBuf::from(r"D:\Program Files (x86)\Steam"),
                PathBuf::from(r"E:\SteamLibrary"),
            ]
        );
    }

    #[test]
    fn parses_helldivers_manifest() {
        let manifest = parse_app_manifest(
            r#""AppState"
{
    "appid" "553850"
    "installdir" "Helldivers 2"
}"#,
        );

        assert_eq!(
            manifest,
            Some(SteamAppManifest {
                app_id: HELLDIVERS_2_STEAM_APP_ID,
                install_dir: HELLDIVERS_2_STEAM_DIR_NAME.to_string(),
            })
        );
    }

    #[test]
    fn validates_slim_data_dir() {
        let sandbox = tempfile::tempdir().expect("tempdir");
        let data_dir = sandbox.path().join("data");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        std::fs::write(data_dir.join("bundles.nxa"), b"bundle").expect("write bundle");

        assert!(is_valid_game_data_dir(&data_dir));
    }

    #[test]
    fn normalizes_mixed_windows_path_text() {
        let path = normalize_path(PathBuf::from(
            r"d:/program files (x86)/steam\steamapps\common\Helldivers 2\data",
        ));

        assert_eq!(
            path,
            PathBuf::from(r"D:\program files (x86)\steam\steamapps\common\Helldivers 2\data")
        );
    }
}
