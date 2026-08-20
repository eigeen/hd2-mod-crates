//! Pure-compute helmet migration using the authoritative one-Unit mapping.

use super::mode_a_common::TargetBuildArtifact;
use crate::archive::StreamToc;
use crate::constants::UNIT_ID;
use crate::migrator::report::MigrationReport;
use crate::padding::{self, EmptyUnitTemplate, PaddingMode};
use crate::unit::culling::{CullingPolicy, replace_patch_culling_with_target};
use crate::unit::direct_mapping::{
    UnitPartMatch, UnmappedUnitPolicy, match_unit_parts, rewrite_mapped_units,
};
use crate::unit::helmet_authority::HelmetMappingTable;
use crate::web::migration::UnmatchedUnitPolicy;
use std::collections::HashMap;

pub struct HelmetMigrationInputs<'a> {
    pub patch: &'a StreamToc,
    pub source_name: &'a str,
    pub mapping_table: &'a HelmetMappingTable,
    pub empty_unit_template: Option<&'a EmptyUnitTemplate>,
    pub padding_mode: PaddingMode,
    pub unmatched_unit_policy: UnmatchedUnitPolicy,
    pub culling_policy: CullingPolicy,
}

/// Rename the mapped helmet Unit, keep non-Unit resources, and hide target-only Units.
pub fn compute_migrated_target(
    inputs: &HelmetMigrationInputs<'_>,
    target: &StreamToc,
    target_hash: &str,
    target_name: &str,
) -> crate::Result<TargetBuildArtifact> {
    let matched = mapped_helmet_match(inputs.mapping_table, inputs.source_name, target_name)?;
    ensure_mapped_units_exist(inputs.patch, target, &matched, target_name)?;
    let rewritten = rewrite_mapped_units(
        inputs.patch,
        std::slice::from_ref(&matched),
        match inputs.unmatched_unit_policy {
            UnmatchedUnitPolicy::Drop => UnmappedUnitPolicy::Drop,
            UnmatchedUnitPolicy::Keep => UnmappedUnitPolicy::Keep,
        },
    );
    let mut patch = rewritten.patch;
    let target_unit_ids = unit_file_ids(target);
    let padded_units = pad_target_only_units(&mut patch, &target_unit_ids, inputs);
    apply_culling_policy(&mut patch, target, inputs.culling_policy)?;
    let report = build_report(
        target_hash,
        target_name,
        rewritten.remapped_units,
        padded_units,
        rewritten.skipped_units,
        (unit_file_ids(inputs.patch).len(), target_unit_ids.len()),
    );
    Ok(TargetBuildArtifact {
        patch,
        report,
        unit_mappings: vec![(matched.source_file_id, matched.target_file_id)],
    })
}

fn apply_culling_policy(
    patch: &mut StreamToc,
    target: &StreamToc,
    policy: CullingPolicy,
) -> crate::Result<()> {
    if policy == CullingPolicy::Patch {
        return Ok(());
    }
    for output in patch
        .entries
        .iter_mut()
        .filter(|entry| entry.type_id == UNIT_ID)
    {
        let Some(target_unit) = target.find(output.file_id, UNIT_ID) else {
            continue;
        };
        *output = replace_patch_culling_with_target(output, target_unit)?;
    }
    Ok(())
}

fn mapped_helmet_match(
    table: &HelmetMappingTable,
    source_name: &str,
    target_name: &str,
) -> crate::Result<UnitPartMatch> {
    let source_parts = table
        .parts(source_name)
        .ok_or_else(|| eyre::eyre!("helmet {source_name:?} is missing from the bundled mapping"))?;
    let target_parts = table
        .parts(target_name)
        .ok_or_else(|| eyre::eyre!("helmet {target_name:?} is missing from the bundled mapping"))?;
    match_unit_parts(source_parts, target_parts)
        .into_iter()
        .next()
        .ok_or_else(|| eyre::eyre!("helmet mappings do not share a Helmet part"))
}

fn ensure_mapped_units_exist(
    patch: &StreamToc,
    target: &StreamToc,
    matched: &UnitPartMatch,
    target_name: &str,
) -> crate::Result<()> {
    if !has_unit(patch, matched.source_file_id) {
        eyre::bail!(
            "source patch does not contain mapped Helmet Unit {}",
            matched.source_file_id
        );
    }
    if !has_unit(target, matched.target_file_id) {
        eyre::bail!(
            "target {target_name:?} does not contain mapped Helmet Unit {}",
            matched.target_file_id
        );
    }
    Ok(())
}

fn pad_target_only_units(
    patch: &mut StreamToc,
    target_unit_ids: &[u64],
    inputs: &HelmetMigrationInputs<'_>,
) -> usize {
    let Some(template) = inputs.empty_unit_template else {
        return 0;
    };
    padding::pad_patch(patch, target_unit_ids, template, inputs.padding_mode, None).len()
}

fn build_report(
    target_hash: &str,
    target_name: &str,
    renamed: usize,
    padded_units: usize,
    skipped_entries: usize,
    unit_counts: (usize, usize),
) -> MigrationReport {
    MigrationReport {
        target_hash: target_hash.to_string(),
        target_name: target_name.to_string(),
        file_id_remapped: renamed,
        padded_units,
        skipped_entries,
        type_counts: HashMap::from([(UNIT_ID, unit_counts)]),
        ..Default::default()
    }
}

fn has_unit(archive: &StreamToc, file_id: u64) -> bool {
    archive
        .entries
        .iter()
        .any(|entry| entry.type_id == UNIT_ID && entry.file_id == file_id)
}

fn unit_file_ids(archive: &StreamToc) -> Vec<u64> {
    archive
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .map(|entry| entry.file_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::TocEntry;
    use crate::constants::MATERIAL_ID;

    const TABLE: &str = r#"{
        "Source": {"Helmet": 10},
        "Target": {"Helmet": 20}
    }"#;

    #[test]
    fn renames_mapped_unit_and_drops_other_units() {
        let table = TABLE.parse::<HelmetMappingTable>().unwrap();
        let patch = archive("patch", &[(10, UNIT_ID), (11, UNIT_ID), (30, MATERIAL_ID)]);
        let target = archive("target", &[(20, UNIT_ID)]);
        let inputs = inputs(&patch, &table, None);

        let artifact = compute_migrated_target(&inputs, &target, "target-hash", "Target").unwrap();

        assert!(has_unit(&artifact.patch, 20));
        assert!(!has_unit(&artifact.patch, 10));
        assert!(!has_unit(&artifact.patch, 11));
        assert!(
            artifact
                .patch
                .entries
                .iter()
                .any(|entry| entry.file_id == 30)
        );
        assert_eq!(artifact.report.file_id_remapped, 1);
        assert_eq!(artifact.report.skipped_entries, 1);
    }

    #[test]
    fn pads_additional_target_units() {
        let table = TABLE.parse::<HelmetMappingTable>().unwrap();
        let patch = archive("patch", &[(10, UNIT_ID)]);
        let target = archive("target", &[(20, UNIT_ID), (21, UNIT_ID)]);
        let template = padding::builtin_template();
        let inputs = inputs(&patch, &table, Some(&template));

        let artifact = compute_migrated_target(&inputs, &target, "target-hash", "Target").unwrap();

        assert!(has_unit(&artifact.patch, 20));
        assert!(has_unit(&artifact.patch, 21));
        assert_eq!(artifact.report.padded_units, 1);
    }

    #[test]
    fn keeps_non_helmet_units_when_requested() {
        let table = TABLE.parse::<HelmetMappingTable>().unwrap();
        let patch = archive("patch", &[(10, UNIT_ID), (11, UNIT_ID)]);
        let target = archive("target", &[(20, UNIT_ID)]);
        let mut inputs = inputs(&patch, &table, None);
        inputs.unmatched_unit_policy = UnmatchedUnitPolicy::Keep;

        let artifact = compute_migrated_target(&inputs, &target, "target-hash", "Target").unwrap();

        assert!(has_unit(&artifact.patch, 20));
        assert!(has_unit(&artifact.patch, 11));
        assert_eq!(artifact.report.skipped_entries, 0);
    }

    fn inputs<'a>(
        patch: &'a StreamToc,
        table: &'a HelmetMappingTable,
        template: Option<&'a EmptyUnitTemplate>,
    ) -> HelmetMigrationInputs<'a> {
        HelmetMigrationInputs {
            patch,
            source_name: "Source",
            mapping_table: table,
            empty_unit_template: template,
            padding_mode: if template.is_none() {
                PaddingMode::Disabled
            } else {
                PaddingMode::Sanitized
            },
            unmatched_unit_policy: UnmatchedUnitPolicy::Drop,
            culling_policy: CullingPolicy::Patch,
        }
    }

    fn archive(name: &str, entries: &[(u64, u64)]) -> StreamToc {
        StreamToc {
            name: name.to_string(),
            entries: entries
                .iter()
                .map(|(file_id, type_id)| TocEntry::new(*file_id, *type_id))
                .collect(),
            ..Default::default()
        }
    }
}
