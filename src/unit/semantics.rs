//! Semantic Unit slot matching for armor migration.
//!
//! Mirrors `mod_armor_migrator/unit_semantics.py`. Strategy:
//!
//! 1. Extract `(BodyType, Slot, Weight, PieceType)` semantic key from each
//!    Unit TocData (regex-style scan — see [`crate::unit::names`]).
//! 2. Index target Units under three priority keys:
//!    - `"1234"`: (body, slot, weight, piece)
//!    - `"124"`:  (body, slot, piece)
//!    - `"24"`:   (slot, piece)
//! 3. For each source Unit, try the priority keys in order. Exactly-one match
//!    wins; >1 → ambiguous; 0 across all keys → missing.

use crate::archive::{StreamToc, TocEntry};
use crate::constants::UNIT_ID;
use crate::unit::names::customization_values;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnitSemanticKey {
    pub body_type: String,
    pub slot: String,
    pub weight: String,
    pub piece_type: String,
}

impl UnitSemanticKey {
    pub fn label(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.body_type, self.slot, self.weight, self.piece_type
        )
    }
}

#[derive(Debug, Clone)]
pub struct UnitSemanticIssue {
    pub source_file_id: u64,
    pub reason: String,
    pub key: Option<UnitSemanticKey>,
    pub match_level: &'static str,
    pub match_key: Vec<String>,
    pub candidates: Vec<u64>,
}

#[derive(Debug, Default, Clone)]
pub struct UnitSemanticRemap {
    pub remap: HashMap<u64, u64>,
    pub match_levels: HashMap<u64, &'static str>,
    pub missing: Vec<UnitSemanticIssue>,
    pub ambiguous: Vec<UnitSemanticIssue>,
}

impl UnitSemanticRemap {
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.ambiguous.is_empty()
    }
}

pub fn extract_semantic_key(toc_data: &[u8]) -> Option<UnitSemanticKey> {
    let [body, slot, weight, piece] = customization_values(toc_data);
    Some(UnitSemanticKey {
        body_type: body?,
        slot: slot?,
        weight: weight?,
        piece_type: piece?,
    })
}

/// Build Unit FileID remaps through prioritized customization metadata.
pub fn build_unit_semantic_remap(
    patch: &StreamToc,
    source: &StreamToc,
    target: &StreamToc,
) -> UnitSemanticRemap {
    let source_units = entries_by_file_id(source, UNIT_ID);
    let target_index = target_units_by_priority_key(target);
    let mut result = UnitSemanticRemap::default();

    let patch_units = patch
        .entries
        .iter()
        .filter(|e| e.type_id == UNIT_ID)
        .collect::<Vec<_>>();
    for patch_entry in patch_units {
        let Some(src_entry) = source_units.get(&patch_entry.file_id) else {
            continue;
        };
        let key = entry_semantic_key(patch_entry, src_entry);
        let Some(key) = key else {
            result.missing.push(UnitSemanticIssue {
                source_file_id: patch_entry.file_id,
                reason: "missing source Unit semantic key".to_string(),
                key: None,
                match_level: "",
                match_key: Vec::new(),
                candidates: Vec::new(),
            });
            continue;
        };
        apply_priority_match(&mut result, patch_entry.file_id, key, &target_index);
    }
    result
}

pub fn format_unit_semantic_issues(result: &UnitSemanticRemap, limit: usize) -> String {
    let mut issues: Vec<&UnitSemanticIssue> = result
        .missing
        .iter()
        .chain(result.ambiguous.iter())
        .collect();
    let total = issues.len();
    issues.truncate(limit);
    let mut parts = Vec::with_capacity(issues.len() + 1);
    for issue in &issues {
        let key = format_issue_key(issue);
        let extra = if issue.candidates.is_empty() {
            String::new()
        } else {
            format!(", candidates={:?}", issue.candidates)
        };
        parts.push(format!(
            "{}: {} ({}{})",
            issue.source_file_id, issue.reason, key, extra
        ));
    }
    if total > limit {
        parts.push(format!("... {} more", total - limit));
    }
    parts.join("; ")
}

fn entry_semantic_key(patch_entry: &TocEntry, source_entry: &TocEntry) -> Option<UnitSemanticKey> {
    extract_semantic_key(&patch_entry.toc_data)
        .or_else(|| extract_semantic_key(&source_entry.toc_data))
}

type PriorityKey = (&'static str, Vec<String>);

fn priority_keys(key: &UnitSemanticKey) -> [PriorityKey; 3] {
    [
        (
            "1234",
            vec![
                key.body_type.clone(),
                key.slot.clone(),
                key.weight.clone(),
                key.piece_type.clone(),
            ],
        ),
        (
            "124",
            vec![
                key.body_type.clone(),
                key.slot.clone(),
                key.piece_type.clone(),
            ],
        ),
        ("24", vec![key.slot.clone(), key.piece_type.clone()]),
    ]
}

fn target_units_by_priority_key(
    target: &StreamToc,
) -> HashMap<(&'static str, Vec<String>), Vec<u64>> {
    let mut out: HashMap<(&'static str, Vec<String>), Vec<u64>> = HashMap::new();
    for entry in target.entries.iter().filter(|e| e.type_id == UNIT_ID) {
        let Some(key) = extract_semantic_key(&entry.toc_data) else {
            continue;
        };
        for pk in priority_keys(&key) {
            out.entry(pk).or_default().push(entry.file_id);
        }
    }
    out
}

fn apply_priority_match(
    result: &mut UnitSemanticRemap,
    source_file_id: u64,
    key: UnitSemanticKey,
    target_index: &HashMap<(&'static str, Vec<String>), Vec<u64>>,
) {
    for (level, parts) in priority_keys(&key) {
        match target_index.get(&(level, parts.clone())) {
            Some(matches) if matches.len() == 1 => {
                result.remap.insert(source_file_id, matches[0]);
                result.match_levels.insert(source_file_id, level);
                return;
            }
            Some(matches) if matches.len() > 1 => {
                result.ambiguous.push(UnitSemanticIssue {
                    source_file_id,
                    reason: "multiple target Units for semantic priority key".to_string(),
                    key: Some(key.clone()),
                    match_level: level,
                    match_key: parts,
                    candidates: matches.clone(),
                });
                return;
            }
            _ => continue,
        }
    }
    result.missing.push(UnitSemanticIssue {
        source_file_id,
        reason: "no target Unit for semantic priority keys".to_string(),
        key: Some(key),
        match_level: "",
        match_key: Vec::new(),
        candidates: Vec::new(),
    });
}

fn format_issue_key(issue: &UnitSemanticIssue) -> String {
    if !issue.match_level.is_empty() {
        return format!("{}:{}", issue.match_level, issue.match_key.join("/"));
    }
    if let Some(k) = &issue.key {
        return k.label();
    }
    "<no key>".to_string()
}

fn entries_by_file_id(toc: &StreamToc, type_id: u64) -> HashMap<u64, &TocEntry> {
    let mut out = HashMap::new();
    for entry in &toc.entries {
        if entry.type_id == type_id {
            out.insert(entry.file_id, entry);
        }
    }
    out
}
