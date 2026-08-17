use std::fs;
use std::path::{Path, PathBuf};

use self_update::backends::github::Update;
use self_update::update::{Release, ReleaseUpdate};
use serde::Serialize;
use tauri::AppHandle;
use tempfile::TempPath;

use crate::command_error::CommandError;

const REPOSITORY_OWNER: &str = "eigeen";
const REPOSITORY_NAME: &str = "hd2-mod-crates";
const ASSET_IDENTIFIER: &str = "hd2-mod-tools-desktop";
const BINARY_NAME: &str = "hd2_migrator_desktop";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const VERIFYING_KEY_HEX: Option<&str> = option_env!("HD2_UPDATE_PUBLIC_KEY_HEX");

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateMetadata {
    current_version: String,
    date: String,
    notes: Option<String>,
    target: String,
    version: String,
}

#[tauri::command]
pub async fn check_app_update() -> Result<Option<AppUpdateMetadata>, CommandError> {
    run_blocking(check_for_update).await
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle, version: String) -> Result<(), CommandError> {
    run_blocking(move || install_update(&version)).await?;
    app.restart();
}

async fn run_blocking<T, F>(operation: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CommandError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| CommandError::from_display("app_update.task_failed", error))?
}

fn check_for_update() -> Result<Option<AppUpdateMetadata>, CommandError> {
    let updater = build_updater(None)?;
    let release = updater
        .get_latest_release()
        .map_err(|error| update_error("app_update.check_failed", error))?;
    if !release_is_newer(&release)? {
        return Ok(None);
    }
    ensure_release_has_target_asset(&release)?;
    Ok(Some(metadata_from_release(release)))
}

fn install_update(version: &str) -> Result<(), CommandError> {
    ensure_version_is_newer(version)?;
    let tag = format!("v{version}");
    let updater = build_updater(Some(&tag))?;
    let backup = ExecutableBackup::create()?;
    if let Err(error) = updater.update() {
        return Err(update_failure(error, &backup));
    }
    Ok(())
}

struct ExecutableBackup {
    current_path: PathBuf,
    backup_path: TempPath,
}

impl ExecutableBackup {
    fn create() -> Result<Self, CommandError> {
        let current_path = current_executable_path()?;
        ensure_persistent_update_location(&current_path)?;
        let backup_path = copy_executable_to_temporary_backup(&current_path)?;
        Ok(Self {
            current_path,
            backup_path,
        })
    }

    fn restore_if_changed(&self) -> Result<(), std::io::Error> {
        if executable_matches_backup(&self.current_path, &self.backup_path)? {
            return Ok(());
        }
        fs::copy(&self.backup_path, &self.current_path)?;
        Ok(())
    }
}

fn executable_matches_backup(current_path: &Path, backup_path: &Path) -> std::io::Result<bool> {
    let current_bytes = match fs::read(current_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    Ok(current_bytes == fs::read(backup_path)?)
}

fn current_executable_path() -> Result<PathBuf, CommandError> {
    std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|error| update_error("app_update.install_path_failed", error))
}

fn copy_executable_to_temporary_backup(current_path: &Path) -> Result<TempPath, CommandError> {
    let parent = executable_parent(current_path)?;
    let backup = create_temporary_backup_file(parent)?;
    let backup_path = backup.into_temp_path();
    fs::copy(current_path, &backup_path)
        .map_err(|error| update_error("app_update.backup_failed", error))?;
    Ok(backup_path)
}

fn executable_parent(current_path: &Path) -> Result<&Path, CommandError> {
    current_path.parent().ok_or_else(|| {
        CommandError::new(
            "app_update.install_path_failed",
            "Executable has no parent directory",
        )
    })
}

fn create_temporary_backup_file(parent: &Path) -> Result<tempfile::NamedTempFile, CommandError> {
    tempfile::Builder::new()
        .prefix(".hd2-migrator-update-")
        .suffix(".backup")
        .tempfile_in(parent)
        .map_err(|error| update_error("app_update.install_path_not_writable", error))
}

fn ensure_persistent_update_location(current_path: &Path) -> Result<(), CommandError> {
    let temporary_directory = fs::canonicalize(std::env::temp_dir())
        .map_err(|error| update_error("app_update.install_path_failed", error))?;
    if !current_path.starts_with(temporary_directory) {
        return Ok(());
    }
    Err(CommandError::new(
        "app_update.temporary_install_path",
        "Move the executable from the temporary directory to a writable permanent folder",
    ))
}

fn update_failure(error: impl std::fmt::Display, backup: &ExecutableBackup) -> CommandError {
    let message = match backup.restore_if_changed() {
        Ok(()) => error.to_string(),
        Err(restore_error) => {
            format!("{error}; restoring the previous executable failed: {restore_error}")
        }
    };
    CommandError::new("app_update.install_failed", message)
}

fn build_updater(target_tag: Option<&str>) -> Result<Box<dyn ReleaseUpdate>, CommandError> {
    let asset_identifier = target_asset_identifier();
    let mut builder = Update::configure();
    builder
        .repo_owner(REPOSITORY_OWNER)
        .repo_name(REPOSITORY_NAME)
        .bin_name(BINARY_NAME)
        .identifier(&asset_identifier)
        .current_version(CURRENT_VERSION)
        .no_confirm(true)
        .show_download_progress(false)
        .show_output(false)
        .verifying_keys([verifying_key()?]);
    if let Some(tag) = target_tag {
        builder.target_version_tag(tag);
    }
    builder
        .build()
        .map_err(|error| update_error("app_update.configuration_failed", error))
}

fn release_is_newer(release: &Release) -> Result<bool, CommandError> {
    self_update::version::bump_is_greater(CURRENT_VERSION, &release.version)
        .map_err(|error| update_error("app_update.invalid_release", error))
}

fn ensure_version_is_newer(version: &str) -> Result<(), CommandError> {
    let newer = self_update::version::bump_is_greater(CURRENT_VERSION, version)
        .map_err(|error| update_error("app_update.invalid_version", error))?;
    if newer {
        return Ok(());
    }
    Err(CommandError::new(
        "app_update.invalid_version",
        format!("Version {version} is not newer than {CURRENT_VERSION}"),
    ))
}

fn ensure_release_has_target_asset(release: &Release) -> Result<(), CommandError> {
    let target = self_update::get_target();
    let identifier = target_asset_identifier();
    if release
        .assets
        .iter()
        .any(|asset| asset.name.contains(&identifier))
    {
        return Ok(());
    }
    Err(CommandError::new(
        "app_update.asset_missing",
        format!(
            "Release {} has no signed archive for {target}",
            release.version
        ),
    ))
}

fn target_asset_identifier() -> String {
    format!("{ASSET_IDENTIFIER}-{}", self_update::get_target())
}

fn metadata_from_release(release: Release) -> AppUpdateMetadata {
    AppUpdateMetadata {
        current_version: CURRENT_VERSION.to_owned(),
        date: release.date,
        notes: release.body,
        target: self_update::get_target().to_owned(),
        version: release.version,
    }
}

fn verifying_key() -> Result<[u8; 32], CommandError> {
    let encoded = VERIFYING_KEY_HEX.ok_or_else(|| {
        CommandError::new(
            "app_update.disabled",
            "This build does not contain an update verifying key",
        )
    })?;
    decode_verifying_key(encoded)
}

fn decode_verifying_key(encoded: &str) -> Result<[u8; 32], CommandError> {
    if encoded.len() != 64 {
        return Err(CommandError::new(
            "app_update.invalid_key",
            "The update verifying key must contain 64 hexadecimal characters",
        ));
    }
    let mut key = [0_u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = decode_hex_byte(&encoded[index * 2..index * 2 + 2])?;
    }
    Ok(key)
}

fn decode_hex_byte(encoded: &str) -> Result<u8, CommandError> {
    u8::from_str_radix(encoded, 16)
        .map_err(|error| CommandError::from_display("app_update.invalid_key", error))
}

fn update_error(code: &'static str, error: impl std::fmt::Display) -> CommandError {
    CommandError::from_display(code, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_ci_public_key_format() {
        let encoded = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        let key = decode_verifying_key(encoded).unwrap();
        assert_eq!(key, std::array::from_fn(|index| index as u8));
    }

    #[test]
    fn rejects_invalid_public_keys() {
        assert!(decode_verifying_key("abcd").is_err());
        assert!(decode_verifying_key(&"z".repeat(64)).is_err());
    }

    #[test]
    fn rejects_an_archive_for_a_different_target() {
        let release = Release {
            version: "9.9.9".to_owned(),
            assets: vec![self_update::update::ReleaseAsset {
                name: format!("{ASSET_IDENTIFIER}-different-target-v9.9.9.zip"),
                download_url: String::new(),
            }],
            ..Release::default()
        };
        assert!(ensure_release_has_target_asset(&release).is_err());
    }

    #[test]
    fn restores_the_previous_executable_after_a_failed_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let backup_file = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        fs::write(backup_file.path(), b"previous executable").unwrap();
        let current_path = directory.path().join("application.exe");
        let backup = ExecutableBackup {
            current_path: current_path.clone(),
            backup_path: backup_file.into_temp_path(),
        };
        update_failure("replacement failed", &backup);
        assert_eq!(fs::read(current_path).unwrap(), b"previous executable");
    }

    #[test]
    fn restores_over_a_partially_written_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let backup_file = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        fs::write(backup_file.path(), b"previous executable").unwrap();
        let current_path = directory.path().join("application.exe");
        fs::write(&current_path, b"partial replacement").unwrap();
        let backup = ExecutableBackup {
            current_path: current_path.clone(),
            backup_path: backup_file.into_temp_path(),
        };
        update_failure("replacement failed", &backup);
        assert_eq!(fs::read(current_path).unwrap(), b"previous executable");
    }
}
