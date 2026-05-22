//! Per-target migration summary.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct MigrationReport {
    pub target_hash: String,
    pub target_name: String,
    pub out_path: Option<PathBuf>,
    pub file_id_remapped: usize,
    pub slot_id_remapped: usize,
    pub padded_units: usize,
    pub skipped_entries: usize,
    pub skipped_types: Vec<u64>,
    /// `type_id -> (source_count, target_count)` for diagnostics.
    pub type_counts: HashMap<u64, (usize, usize)>,
    pub warnings: Vec<String>,
}
