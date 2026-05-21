use super::{MigrationReport, TargetBuild};
use crate::archive::{StreamToc, TocEntry};
use eyre::WrapErr;
use std::collections::HashMap;
use std::path::Path;

pub(super) fn write_grouped_builds(
    builds: Vec<TargetBuild>,
    out_root: &Path,
    patch_suffix: &str,
) -> crate::Result<Vec<MigrationReport>> {
    let mut reports = Vec::new();
    for group in group_target_builds(builds) {
        let (mut patch, mut report) = merge_target_builds(group);
        let out_dir = out_root.join(super::super::safe_filename(&report.target_name));
        std::fs::create_dir_all(&out_dir)
            .wrap_err_with(|| format!("create dir {}", out_dir.display()))?;
        let out_path = out_dir.join(patch_suffix);
        patch.write_files(&out_path)?;
        report.out_path = Some(out_path.clone());
        tracing::info!(
            target = %report.target_name,
            hashes = %report.target_hash,
            output = %out_path.display(),
            entries = patch.entries.len(),
            "wrote merged target patch"
        );
        reports.push(report);
    }
    Ok(reports)
}

fn group_target_builds(builds: Vec<TargetBuild>) -> Vec<Vec<TargetBuild>> {
    let mut groups: Vec<Vec<TargetBuild>> = Vec::new();
    let mut indexes: HashMap<String, usize> = HashMap::new();
    for build in builds {
        let name = build.report.target_name.clone();
        let index = match indexes.get(&name) {
            Some(index) => *index,
            None => {
                let index = groups.len();
                indexes.insert(name, index);
                groups.push(Vec::new());
                index
            }
        };
        groups[index].push(build);
    }
    groups
}

fn merge_target_builds(mut builds: Vec<TargetBuild>) -> (StreamToc, MigrationReport) {
    debug_assert!(!builds.is_empty());
    let first = builds.remove(0);
    let mut patch = first.patch;
    let mut report = first.report;
    let mut target_hashes = vec![report.target_hash.clone()];
    let mut entry_indexes = entry_indexes_by_key(&patch);
    let mut conflict_count = 0usize;
    let mut conflict_examples = Vec::new();

    for build in builds {
        target_hashes.push(build.report.target_hash.clone());
        merge_report_totals(&mut report, build.report);
        merge_patch_entries(
            &mut patch,
            &mut entry_indexes,
            &build.patch,
            &mut conflict_count,
            &mut conflict_examples,
        );
    }

    report.target_hash = target_hashes.join(",");
    if conflict_count > 0 {
        report
            .warnings
            .push(conflict_warning(conflict_count, &conflict_examples));
    }
    (patch, report)
}

fn merge_report_totals(report: &mut MigrationReport, next: MigrationReport) {
    report.file_id_remapped += next.file_id_remapped;
    report.slot_id_remapped += next.slot_id_remapped;
    report.padded_units += next.padded_units;
    report.skipped_entries += next.skipped_entries;
    for type_id in next.skipped_types {
        if !report.skipped_types.contains(&type_id) {
            report.skipped_types.push(type_id);
        }
    }
    for (type_id, (source_count, target_count)) in next.type_counts {
        let counts = report.type_counts.entry(type_id).or_insert((0, 0));
        counts.0 += source_count;
        counts.1 += target_count;
    }
    report.warnings.extend(next.warnings);
}

fn merge_patch_entries(
    patch: &mut StreamToc,
    entry_indexes: &mut HashMap<(u64, u64), usize>,
    incoming: &StreamToc,
    conflict_count: &mut usize,
    conflict_examples: &mut Vec<String>,
) {
    for entry in &incoming.entries {
        let key = (entry.file_id, entry.type_id);
        match entry_indexes.get(&key).copied() {
            Some(index) if entries_are_equal(&patch.entries[index], entry) => {}
            Some(_) => record_merge_conflict(key, conflict_count, conflict_examples),
            None => {
                entry_indexes.insert(key, patch.entries.len());
                patch.entries.push(entry.clone());
            }
        }
    }
}

fn entry_indexes_by_key(patch: &StreamToc) -> HashMap<(u64, u64), usize> {
    patch
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| ((entry.file_id, entry.type_id), index))
        .collect()
}

fn entries_are_equal(left: &TocEntry, right: &TocEntry) -> bool {
    left.toc_data == right.toc_data
        && left.gpu_data == right.gpu_data
        && left.stream_data == right.stream_data
}

fn record_merge_conflict(
    key: (u64, u64),
    conflict_count: &mut usize,
    conflict_examples: &mut Vec<String>,
) {
    *conflict_count += 1;
    if conflict_examples.len() < 6 {
        conflict_examples.push(entry_key_label(key));
    }
}

fn conflict_warning(conflict_count: usize, examples: &[String]) -> String {
    format!(
        "same-name merge kept first entry for {conflict_count} conflicting FileID/TypeID pair(s): {}",
        examples.join(", ")
    )
}

fn entry_key_label((file_id, type_id): (u64, u64)) -> String {
    let type_name = crate::constants::type_name(type_id).unwrap_or("<unknown>");
    format!("0x{file_id:016x}/{type_name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::UNIT_ID;

    const TEST_TYPE_ID: u64 = UNIT_ID;

    #[test]
    fn merge_appends_missing_entries() {
        let group = vec![
            target_build(0, "hash-a", "Same", vec![entry(1, 10)]),
            target_build(1, "hash-b", "Same", vec![entry(2, 20)]),
        ];

        let (patch, report) = merge_target_builds(group);

        assert_eq!(patch.entries.len(), 2);
        assert!(patch.find(1, TEST_TYPE_ID).is_some());
        assert!(patch.find(2, TEST_TYPE_ID).is_some());
        assert_eq!(report.target_hash, "hash-a,hash-b");
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn merge_ignores_identical_duplicate_entries() {
        let group = vec![
            target_build(0, "hash-a", "Same", vec![entry(1, 10)]),
            target_build(1, "hash-b", "Same", vec![entry(1, 10)]),
        ];

        let (patch, report) = merge_target_builds(group);

        assert_eq!(patch.entries.len(), 1);
        assert_eq!(patch.entries[0].toc_data, vec![10]);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn merge_keeps_first_entry_on_conflict() {
        let group = vec![
            target_build(0, "hash-a", "Same", vec![entry(1, 10)]),
            target_build(1, "hash-b", "Same", vec![entry(1, 20)]),
        ];

        let (patch, report) = merge_target_builds(group);

        assert_eq!(patch.entries.len(), 1);
        assert_eq!(patch.entries[0].toc_data, vec![10]);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("0x0000000000000001/Unit"));
    }

    #[test]
    fn group_target_builds_preserves_first_name_order() {
        let groups = group_target_builds(vec![
            target_build(0, "hash-a", "Same", vec![entry(1, 10)]),
            target_build(1, "hash-b", "Other", vec![entry(2, 20)]),
            target_build(2, "hash-c", "Same", vec![entry(3, 30)]),
        ]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0][0].report.target_hash, "hash-a");
        assert_eq!(groups[0][1].report.target_hash, "hash-c");
        assert_eq!(groups[1][0].report.target_hash, "hash-b");
    }

    fn target_build(
        order: usize,
        target_hash: &str,
        target_name: &str,
        entries: Vec<TocEntry>,
    ) -> TargetBuild {
        let patch = StreamToc {
            entries,
            ..Default::default()
        };
        TargetBuild {
            order,
            patch,
            report: MigrationReport {
                target_hash: target_hash.to_string(),
                target_name: target_name.to_string(),
                file_id_remapped: 1,
                ..MigrationReport::default()
            },
        }
    }

    fn entry(file_id: u64, marker: u8) -> TocEntry {
        let mut entry = TocEntry::new(file_id, TEST_TYPE_ID);
        entry.toc_data = vec![marker];
        entry.gpu_data = vec![marker + 1];
        entry.stream_data = vec![marker + 2];
        entry
    }
}
