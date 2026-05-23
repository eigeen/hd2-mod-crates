//! CLI surface: clap args, dialoguer-based interactive fill-in, logging, and
//! progress reporting.

pub mod args;
pub mod interactive;
pub mod logging;
pub mod progress;

use args::Cli;
use clap::Parser;
use eyre::WrapErr;
use hd2_migrator_io::index::ArchiveIndex;
use hd2_migrator_io::migrator::{MigrateAllOpts, MigrationReport, migrate_all};
use hd2_migrator_io::padding::{EmptyUnitTemplate, builtin_template, extract_template};
use owo_colors::OwoColorize;
use progress::IndicatifProgress;
use std::path::Path;

pub fn run() -> hd2_migrator_io::Result<()> {
    let mut cli = Cli::parse();
    logging::init(cli.verbose);
    print_startup_banner();

    let owned_index;
    let index: &ArchiveIndex = match cli.index.as_deref() {
        Some(p) => {
            owned_index = ArchiveIndex::load(p)?;
            &owned_index
        }
        None => ArchiveIndex::builtin(),
    };

    interactive::fill_in(&mut cli, index)?;

    let patch = cli
        .patch
        .clone()
        .ok_or_else(|| eyre::eyre!("--patch is required"))?;
    let out_dir = cli
        .out_dir
        .clone()
        .ok_or_else(|| eyre::eyre!("--out-dir is required"))?;

    // Build empty-mesh template (or skip for --no-padding).
    let template: Option<EmptyUnitTemplate> = if cli.no_padding {
        None
    } else if let Some(custom) = cli.empty_mesh_from.as_deref() {
        Some(
            extract_template(custom)
                .wrap_err_with(|| format!("extract empty mesh from {}", custom.display()))?,
        )
    } else {
        Some(builtin_template())
    };
    let template_ref = template.as_ref();
    let padding_mode = cli.padding_mode();

    let data_dir = cli
        .data_dir
        .clone()
        .ok_or_else(|| eyre::eyre!("--data-dir is required"))?;
    let reports = run_mode_a(
        &cli,
        index,
        &patch,
        &out_dir,
        &data_dir,
        template_ref,
        padding_mode,
    )?;

    print_summary(&reports);
    Ok(())
}

fn run_mode_a(
    cli: &Cli,
    index: &ArchiveIndex,
    patch: &Path,
    out_dir: &Path,
    data_dir: &Path,
    template: Option<&EmptyUnitTemplate>,
    padding_mode: hd2_migrator_io::padding::PaddingMode,
) -> hd2_migrator_io::Result<Vec<MigrationReport>> {
    let target_filter = if cli.target.is_empty() {
        None
    } else {
        Some(cli.target.as_slice())
    };
    let progress = IndicatifProgress::new(target_filter.map(|t| t.len() as u64).unwrap_or(0));
    let opts = MigrateAllOpts {
        patch_path: patch,
        data_dir,
        out_dir,
        archive_index: index,
        source_hash: cli.source.as_deref(),
        target_hashes: target_filter,
        category: &cli.category,
        patch_suffix: &cli.patch_suffix,
        empty_unit_template: template,
        padding_mode,
        armor_mapping_json: cli.armor_mapping_json.as_deref(),
        experimental_partial_remap: cli.experimental_partial_remap,
        progress: Some(&progress),
    };
    let result = migrate_all(opts);
    progress.finish();
    result
}

fn print_summary(reports: &[MigrationReport]) {
    if reports.is_empty() {
        eprintln!("{}", "No targets migrated.".yellow());
        return;
    }
    eprintln!();
    eprintln!("{} {} targets:", "Migrated".green().bold(), reports.len());
    for r in reports {
        let path = r
            .out_path
            .as_ref()
            .map(|p| display_path(p))
            .unwrap_or_default();
        eprintln!(
            "  {}: {} entries, {} FileIDs, {} SlotIDs, {} padded → {}",
            r.target_name.bold(),
            r.file_id_remapped,
            r.file_id_remapped,
            r.slot_id_remapped,
            r.padded_units,
            path.cyan()
        );
        for w in &r.warnings {
            eprintln!("    {} {}", "warning:".yellow(), w);
        }
    }
}

fn print_startup_banner() {
    eprintln!(
        "{} {}",
        env!("CARGO_PKG_NAME").bold(),
        format!("v{}", env!("CARGO_PKG_VERSION")).cyan()
    );
}

fn display_path(p: &Path) -> String {
    p.display().to_string()
}
