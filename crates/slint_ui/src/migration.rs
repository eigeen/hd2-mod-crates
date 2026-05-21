use mod_armor_migrator::{
    builtin_template, migrate_all, ArchiveIndex, EmptyUnitTemplate, MigrateAllOpts,
    MigrationReport, PaddingMode, ProgressSink,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct MigrationRequest {
    pub patch_path: PathBuf,
    pub data_dir: PathBuf,
    pub out_dir: PathBuf,
    pub target_filter: String,
    pub no_padding: bool,
    pub experimental_partial_remap: bool,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub running: bool,
    pub status_text: String,
    pub log_text: String,
    pub migrated_count: i32,
    pub warning_count: i32,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            running: false,
            status_text: "Ready".to_string(),
            log_text: String::new(),
            migrated_count: 0,
            warning_count: 0,
        }
    }
}

pub type SharedUiState = Arc<Mutex<UiState>>;

/// Start migration on a worker thread and publish progress through shared UI state.
pub fn start_migration(request: MigrationRequest, state: SharedUiState) -> eyre::Result<()> {
    validate_request(&request)?;
    reset_state(&state, "Preparing migration");

    std::thread::spawn(move || {
        let result = run_migration(&request, &state);
        finish_state(&state, result);
    });

    Ok(())
}

fn validate_request(request: &MigrationRequest) -> eyre::Result<()> {
    if request.patch_path.as_os_str().is_empty() {
        eyre::bail!("Patch path is required");
    }
    if request.data_dir.as_os_str().is_empty() {
        eyre::bail!("Game data directory is required");
    }
    if request.out_dir.as_os_str().is_empty() {
        eyre::bail!("Output directory is required");
    }
    Ok(())
}

fn reset_state(state: &SharedUiState, status_text: &str) {
    let mut state = state.lock().expect("UI state lock poisoned");
    state.running = true;
    state.status_text = status_text.to_string();
    state.log_text.clear();
    state.migrated_count = 0;
    state.warning_count = 0;
}

fn run_migration(
    request: &MigrationRequest,
    state: &SharedUiState,
) -> mod_armor_migrator::Result<Vec<MigrationReport>> {
    let targets = parse_target_filter(&request.target_filter);
    let template = padding_template(request.no_padding);
    let progress = UiProgress::new(Arc::clone(state));
    let opts = MigrateAllOpts {
        patch_path: &request.patch_path,
        data_dir: &request.data_dir,
        out_dir: &request.out_dir,
        archive_index: ArchiveIndex::builtin(),
        source_hash: None,
        target_hashes: targets.as_deref(),
        category: "Armor",
        patch_suffix: "9ba626afa44a3aa3.patch_0",
        empty_unit_template: template.as_ref(),
        padding_mode: padding_mode(request.no_padding),
        armor_mapping_json: None,
        experimental_partial_remap: request.experimental_partial_remap,
        progress: Some(&progress),
    };
    migrate_all(opts)
}

fn parse_target_filter(value: &str) -> Option<Vec<String>> {
    let targets: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    (!targets.is_empty()).then_some(targets)
}

fn padding_template(no_padding: bool) -> Option<EmptyUnitTemplate> {
    (!no_padding).then(builtin_template)
}

fn padding_mode(no_padding: bool) -> PaddingMode {
    if no_padding {
        PaddingMode::Disabled
    } else {
        PaddingMode::Sanitized
    }
}

fn finish_state(state: &SharedUiState, result: mod_armor_migrator::Result<Vec<MigrationReport>>) {
    let mut state = state.lock().expect("UI state lock poisoned");
    state.running = false;
    match result {
        Ok(reports) => apply_success(&mut state, &reports),
        Err(error) => {
            state.status_text = "Failed".to_string();
            state.log_text = format!("{error:?}");
        }
    }
}

fn apply_success(state: &mut UiState, reports: &[MigrationReport]) {
    let warning_count: usize = reports.iter().map(|report| report.warnings.len()).sum();
    state.migrated_count = reports.len() as i32;
    state.warning_count = warning_count as i32;
    state.status_text = format!("Migrated {} targets", reports.len());
    state.log_text = report_lines(reports).join("\n");
}

fn report_lines(reports: &[MigrationReport]) -> Vec<String> {
    reports
        .iter()
        .map(|report| {
            format!(
                "{}: {} FileIDs, {} SlotIDs, {} padded",
                report.target_name,
                report.file_id_remapped,
                report.slot_id_remapped,
                report.padded_units
            )
        })
        .collect()
}

struct UiProgress {
    state: SharedUiState,
}

impl UiProgress {
    fn new(state: SharedUiState) -> Self {
        Self { state }
    }
}

impl ProgressSink for UiProgress {
    fn target_started(&self, name: &str) {
        update_status(&self.state, format!("Migrating {name}"));
    }

    fn stage(&self, name: &str, stage: &str) {
        update_status(&self.state, format!("{name}: {stage}"));
    }

    fn target_finished(&self, name: &str) {
        update_status(&self.state, format!("Finished {name}"));
    }
}

fn update_status(state: &SharedUiState, status_text: String) {
    state.lock().expect("UI state lock poisoned").status_text = status_text;
}
