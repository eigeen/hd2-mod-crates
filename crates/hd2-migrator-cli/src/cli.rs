//! CLI surface: clap args, dialoguer-based interactive fill-in, logging, and
//! progress reporting.

pub mod args;
pub mod interactive;
pub mod logging;
pub mod progress;

use args::{Cli, CliCommand, ExtractWebMetadataArgs};
use clap::Parser;
use eyre::WrapErr;
use hd2_migrator_io::index::ArchiveIndex;
use hd2_migrator_io::migrator::{MigrateAllOpts, MigrationReport, migrate_all};
use hd2_migrator_io::padding::{EmptyUnitTemplate, builtin_template, extract_template};
use hd2_migrator_io::web::{ExtractMetadataOptions, WebGameMetadata, extract_game_metadata};
use owo_colors::OwoColorize;
use progress::IndicatifProgress;
use std::path::Path;

pub fn run() -> hd2_migrator_io::Result<()> {
    let mut cli = Cli::parse();
    logging::init(cli.verbose);
    print_startup_banner();

    if let Some(command) = cli.command {
        return run_command(command);
    }

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

fn run_command(command: CliCommand) -> hd2_migrator_io::Result<()> {
    match command {
        CliCommand::ExtractWebMetadata(args) => extract_web_metadata(args),
    }
}

fn extract_web_metadata(args: ExtractWebMetadataArgs) -> hd2_migrator_io::Result<()> {
    let owned_index;
    let index = match args.index.as_deref() {
        Some(path) => {
            owned_index = ArchiveIndex::load(path)?;
            &owned_index
        }
        None => ArchiveIndex::builtin(),
    };
    let metadata = extract_game_metadata(ExtractMetadataOptions {
        data_dir: &args.data_dir,
        archive_index: index,
        category: &args.category,
    })?;
    let text = serialize_web_metadata(&metadata)?;
    write_utf8(&args.out, &text)?;
    eprintln!(
        "{} {} targets -> {}",
        "Extracted".green().bold(),
        metadata.targets.len(),
        args.out.display().to_string().cyan()
    );
    Ok(())
}

/// Serializes product metadata as compact JSON to keep browser asset size low.
fn serialize_web_metadata(metadata: &WebGameMetadata) -> hd2_migrator_io::Result<String> {
    serde_json::to_string(metadata).map_err(Into::into)
}

fn write_utf8(path: &Path, text: &str) -> hd2_migrator_io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .wrap_err_with(|| format!("create dir {}", parent.display()))?;
    }
    std::fs::write(path, text.as_bytes())
        .wrap_err_with(|| format!("write UTF-8 {}", path.display()))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_metadata_serialization_is_compact() {
        let metadata = WebGameMetadata::new("Armor", Vec::new());
        let text = serialize_web_metadata(&metadata).unwrap();

        assert_eq!(
            text,
            r#"{"schemaVersion":1,"category":"Armor","targets":[]}"#
        );
        assert!(!text.contains('\n'));
        assert!(!text.contains("  "));
    }
}
