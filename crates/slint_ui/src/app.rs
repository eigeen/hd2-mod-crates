use crate::migration::{start_migration, MigrationRequest, SharedUiState, UiState};
use crate::path_import::first_path_from_drop_payload;
use crate::AppWindow;
use rfd::FileDialog;
use slint::{ComponentHandle, SharedString, Timer, TimerMode};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Launch the Slint prototype window.
pub fn run() -> eyre::Result<()> {
    init_logging();
    let ui = AppWindow::new()?;
    let state = Arc::new(Mutex::new(UiState::default()));
    bind_file_dialogs(&ui);
    bind_drop_imports(&ui);
    bind_run_action(&ui, Arc::clone(&state));
    let _timer = start_state_timer(&ui, state);
    ui.run()?;
    Ok(())
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

fn bind_file_dialogs(ui: &AppWindow) {
    let patch_ui = ui.as_weak();
    ui.on_choose_patch(move || choose_file(&patch_ui, UiPathField::Patch));

    let data_ui = ui.as_weak();
    ui.on_choose_data_dir(move || choose_folder(&data_ui, UiPathField::DataDir));

    let out_ui = ui.as_weak();
    ui.on_choose_out_dir(move || choose_folder(&out_ui, UiPathField::OutDir));
}

fn choose_file(ui: &slint::Weak<AppWindow>, field: UiPathField) {
    let Some(path) = FileDialog::new().pick_file() else {
        return;
    };
    set_path_field(ui, field, &path);
}

fn choose_folder(ui: &slint::Weak<AppWindow>, field: UiPathField) {
    let Some(path) = FileDialog::new().pick_folder() else {
        return;
    };
    set_path_field(ui, field, &path);
}

fn bind_drop_imports(ui: &AppWindow) {
    let weak_ui = ui.as_weak();
    ui.on_path_dropped(move |field, payload| {
        let Some(path) = first_path_from_drop_payload(payload.as_str()) else {
            return;
        };
        let field = UiPathField::from_key(field.as_str());
        set_path_field(&weak_ui, field, &path);
    });
}

fn set_path_field(ui: &slint::Weak<AppWindow>, field: UiPathField, path: &Path) {
    let Some(ui) = ui.upgrade() else {
        return;
    };
    let value = path_to_shared_string(path);
    match field {
        UiPathField::Patch => ui.set_patch_path(value),
        UiPathField::DataDir => ui.set_data_dir(value),
        UiPathField::OutDir => ui.set_out_dir(value),
    }
}

fn bind_run_action(ui: &AppWindow, state: SharedUiState) {
    let weak_ui = ui.as_weak();
    ui.on_run_migration(move || {
        let Some(ui) = weak_ui.upgrade() else {
            return;
        };
        if ui.get_running() {
            return;
        }
        let request = request_from_ui(&ui);
        if let Err(error) = start_migration(request, Arc::clone(&state)) {
            apply_error(&ui, error);
        }
    });
}

fn request_from_ui(ui: &AppWindow) -> MigrationRequest {
    MigrationRequest {
        patch_path: PathBuf::from(ui.get_patch_path().as_str()),
        data_dir: PathBuf::from(ui.get_data_dir().as_str()),
        out_dir: PathBuf::from(ui.get_out_dir().as_str()),
        target_filter: ui.get_target_filter().to_string(),
        no_padding: ui.get_no_padding(),
        experimental_partial_remap: ui.get_experimental_partial_remap(),
    }
}

fn apply_error(ui: &AppWindow, error: eyre::Report) {
    ui.set_status_text("Input error".into());
    ui.set_log_text(format!("{error:?}").into());
}

fn start_state_timer(ui: &AppWindow, state: SharedUiState) -> Timer {
    let timer = Timer::default();
    let weak_ui = ui.as_weak();
    timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(ui) = weak_ui.upgrade() else {
            return;
        };
        let snapshot = state.lock().expect("UI state lock poisoned").clone();
        apply_state_snapshot(&ui, snapshot);
    });
    timer
}

fn apply_state_snapshot(ui: &AppWindow, state: UiState) {
    ui.set_running(state.running);
    ui.set_status_text(state.status_text.into());
    ui.set_log_text(state.log_text.into());
    ui.set_migrated_count(state.migrated_count);
    ui.set_warning_count(state.warning_count);
}

fn path_to_shared_string(path: &Path) -> SharedString {
    path.display().to_string().into()
}

#[derive(Debug, Clone, Copy)]
enum UiPathField {
    Patch,
    DataDir,
    OutDir,
}

impl UiPathField {
    fn from_key(key: &str) -> Self {
        match key {
            "data" => Self::DataDir,
            "out" => Self::OutDir,
            _ => Self::Patch,
        }
    }
}
