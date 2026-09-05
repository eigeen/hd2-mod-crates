//! Safe, raw-preserving Stingray Unit mesh composition.

mod format;
mod vertex;

use crate::archive::TocEntry;

pub use format::{MergeDiagnostic, MergeOutcome};

/// Return materials referenced by normal, visible helmet sections.
pub fn visible_material_file_ids(entry: &TocEntry) -> crate::Result<Vec<u64>> {
    format::visible_material_file_ids(entry)
}

/// Merge every supported visible helmet LOD into one armor body Unit.
pub fn merge_visible_head_into_body(
    body: &TocEntry,
    helmet: &TocEntry,
    material_file_ids: &std::collections::HashMap<u64, u64>,
) -> crate::Result<MergeOutcome> {
    format::merge_visible_head_into_body(body, helmet, material_file_ids)
}
