use clap::{ArgAction, Parser};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "hd2-migrator", version, about, long_about = None)]
pub struct Cli {
    /// Path to the source mod patch (e.g. 9ba626afa44a3aa3.patch_0).
    #[arg(long)]
    pub patch: Option<PathBuf>,

    /// Output directory for migrated variants.
    #[arg(long = "out-dir")]
    pub out_dir: Option<PathBuf>,

    /// Game data/ directory.
    #[arg(long = "data-dir")]
    pub data_dir: Option<PathBuf>,

    /// Override source archive hex hash; auto-detected if omitted.
    #[arg(long)]
    pub source: Option<String>,

    /// Comma-separated target hashes (or names). Empty = all.
    #[arg(long, value_delimiter = ',')]
    pub target: Vec<String>,

    /// Archive category in archivehashes.json.
    #[arg(long, default_value = "Armor")]
    pub category: String,

    /// Override archivehashes.json path; defaults to the bundled copy.
    #[arg(long)]
    pub index: Option<PathBuf>,

    /// Override authoritative armor Unit-part mapping JSON.
    #[arg(long = "armor-mapping-json")]
    pub armor_mapping_json: Option<PathBuf>,

    /// Custom empty mesh patch to use as padding template; defaults to builtin.
    #[arg(long = "empty-mesh-from")]
    pub empty_mesh_from: Option<PathBuf>,

    /// Disable empty-mesh padding for target-only Unit slots.
    #[arg(long = "no-padding")]
    pub no_padding: bool,

    /// Use empty mesh template bytes verbatim (no sanitization).
    #[arg(long = "empty-mesh-verbatim")]
    pub empty_mesh_verbatim: bool,

    /// Output patch filename inside each target's directory.
    #[arg(long = "patch-suffix", default_value = "9ba626afa44a3aa3.patch_0")]
    pub patch_suffix: String,

    /// Emit incomplete Unit remaps for testing.
    #[arg(long = "experimental-partial-remap")]
    pub experimental_partial_remap: bool,

    /// Increase logging verbosity: -v info, -vv debug, -vvv trace.
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    /// Fail (don't prompt) when required args are missing.
    #[arg(long = "non-interactive")]
    pub non_interactive: bool,
}

impl Cli {
    pub fn padding_mode(&self) -> hd2_migrator_io::padding::PaddingMode {
        if self.no_padding {
            hd2_migrator_io::padding::PaddingMode::Disabled
        } else if self.empty_mesh_verbatim {
            hd2_migrator_io::padding::PaddingMode::Verbatim
        } else {
            hd2_migrator_io::padding::PaddingMode::Sanitized
        }
    }
}
