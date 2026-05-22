//! Wasm-safe migration planning types.

use hd2_archive_format::{constants::UNIT_ID, StreamToc, TocEntry};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationReport {
    pub target_hash: String,
    pub target_name: String,
    pub file_id_remapped: usize,
    pub slot_id_remapped: usize,
    pub padded_units: usize,
    pub skipped_entries: usize,
    pub skipped_types: Vec<u64>,
    pub type_counts: HashMap<u64, (usize, usize)>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RemapPlan {
    pub remap: HashMap<u64, u64>,
    pub skipped_types: Vec<u64>,
    pub type_counts: HashMap<u64, (usize, usize)>,
    pub skipped_file_ids: HashSet<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaddingMode {
    Disabled,
    Sanitized,
    Verbatim,
}

pub fn build_ordinal_remap(source: &StreamToc, target: &StreamToc) -> RemapPlan {
    let source_by_type = entries_by_type(&source.entries);
    let target_by_type = entries_by_type(&target.entries);
    let mut plan = RemapPlan::default();

    for type_id in type_order(&source.entries) {
        let source_entries = source_by_type.get(&type_id).map(Vec::as_slice).unwrap_or(&[]);
        let target_entries = target_by_type.get(&type_id).map(Vec::as_slice).unwrap_or(&[]);
        plan.type_counts
            .insert(type_id, (source_entries.len(), target_entries.len()));
        if target_entries.is_empty() || type_id == UNIT_ID {
            record_skipped_type(&mut plan, type_id, source_entries);
            continue;
        }
        if source_entries.len() > target_entries.len() {
            plan.skipped_types.push(type_id);
            for entry in &source_entries[target_entries.len()..] {
                plan.skipped_file_ids.insert(entry.file_id);
            }
        }
        for (source_entry, target_entry) in source_entries.iter().zip(target_entries.iter()) {
            if source_entry.file_id != target_entry.file_id {
                plan.remap.insert(source_entry.file_id, target_entry.file_id);
            }
        }
    }
    plan
}

fn record_skipped_type(plan: &mut RemapPlan, type_id: u64, entries: &[&TocEntry]) {
    plan.skipped_types.push(type_id);
    for entry in entries {
        plan.skipped_file_ids.insert(entry.file_id);
    }
}

fn type_order(entries: &[TocEntry]) -> Vec<u64> {
    let mut order = Vec::new();
    for entry in entries {
        if !order.contains(&entry.type_id) {
            order.push(entry.type_id);
        }
    }
    order
}

fn entries_by_type(entries: &[TocEntry]) -> HashMap<u64, Vec<&TocEntry>> {
    let mut out: HashMap<u64, Vec<&TocEntry>> = HashMap::new();
    for entry in entries {
        out.entry(entry.type_id).or_default().push(entry);
    }
    out
}
