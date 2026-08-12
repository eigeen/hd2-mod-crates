//! Shared mapping-table driven Unit matching and top-level FileID rewriting.

use crate::archive::{StreamToc, TocEntry};
use crate::constants::UNIT_ID;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitPartMatch {
    pub source_file_id: u64,
    pub target_file_id: u64,
    pub part_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnmappedUnitPolicy {
    Drop,
    Keep,
}

#[derive(Debug, Clone)]
pub struct DirectUnitRewrite {
    pub patch: StreamToc,
    pub remapped_units: usize,
    pub skipped_units: usize,
}

/// Match source and target Unit FileIDs through their shared part labels.
pub fn match_unit_parts(
    source_parts: &HashMap<String, u64>,
    target_parts: &HashMap<String, u64>,
) -> Vec<UnitPartMatch> {
    let mut matches = source_parts
        .iter()
        .filter_map(|(part_label, source_file_id)| {
            let target_file_id = target_parts.get(part_label)?;
            Some(UnitPartMatch {
                source_file_id: *source_file_id,
                target_file_id: *target_file_id,
                part_label: part_label.clone(),
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|matched| matched.source_file_id);
    matches
}

/// Rename mapped Unit entries while preserving every non-Unit entry verbatim.
pub fn rewrite_mapped_units(
    patch: &StreamToc,
    matches: &[UnitPartMatch],
    unmapped_policy: UnmappedUnitPolicy,
) -> DirectUnitRewrite {
    let remap = matches
        .iter()
        .map(|matched| (matched.source_file_id, matched.target_file_id))
        .collect::<HashMap<_, _>>();
    let mut output = StreamToc {
        name: patch.name.clone(),
        ..Default::default()
    };
    let mut remapped_units = 0;
    let mut skipped_units = 0;
    for entry in &patch.entries {
        if entry.type_id != UNIT_ID {
            output.entries.push(entry.clone());
            continue;
        }
        if let Some(target_file_id) = remap.get(&entry.file_id) {
            output.entries.push(rename_entry(entry, *target_file_id));
            remapped_units += 1;
        } else if unmapped_policy == UnmappedUnitPolicy::Keep {
            output.entries.push(entry.clone());
        } else {
            skipped_units += 1;
        }
    }
    DirectUnitRewrite {
        patch: output,
        remapped_units,
        skipped_units,
    }
}

fn rename_entry(entry: &TocEntry, target_file_id: u64) -> TocEntry {
    let mut renamed = entry.clone();
    renamed.file_id = target_file_id;
    renamed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::MATERIAL_ID;

    #[test]
    fn matches_multiple_parts_by_label() {
        let source = HashMap::from([("body".to_string(), 1), ("arm".to_string(), 2)]);
        let target = HashMap::from([("body".to_string(), 11), ("arm".to_string(), 12)]);

        let mut matches = match_unit_parts(&source, &target);
        matches.sort_by_key(|matched| matched.source_file_id);

        assert_eq!(matches[0].target_file_id, 11);
        assert_eq!(matches[1].target_file_id, 12);
    }

    #[test]
    fn rewrites_units_and_preserves_all_non_units() {
        let patch = StreamToc {
            entries: vec![
                TocEntry::new(1, UNIT_ID),
                TocEntry::new(2, UNIT_ID),
                TocEntry::new(30, MATERIAL_ID),
            ],
            ..Default::default()
        };
        let matches = vec![UnitPartMatch {
            source_file_id: 1,
            target_file_id: 11,
            part_label: "Helmet".to_string(),
        }];

        let rewritten = rewrite_mapped_units(&patch, &matches, UnmappedUnitPolicy::Drop);

        assert_eq!(rewritten.remapped_units, 1);
        assert_eq!(rewritten.skipped_units, 1);
        assert!(
            rewritten
                .patch
                .entries
                .iter()
                .any(|entry| entry.file_id == 11)
        );
        assert!(
            rewritten
                .patch
                .entries
                .iter()
                .any(|entry| entry.file_id == 30)
        );
    }
}
