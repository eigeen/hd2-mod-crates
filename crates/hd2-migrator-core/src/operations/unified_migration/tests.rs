use super::*;

#[test]
fn allows_one_source_to_map_to_multiple_targets_in_one_variant() {
    let mapping = WebMigrationMapping {
        category: EquipmentCategory::Helmet,
        source_hash: "13f9269d08e52cf2".to_string(),
        target_hash: "a856edff49cfdd95".to_string(),
    };
    let variant = WebMigrationVariant {
        mappings: vec![
            mapping,
            WebMigrationMapping {
                category: EquipmentCategory::Helmet,
                source_hash: "13f9269d08e52cf2".to_string(),
                target_hash: "1a2fc86abd27bf5b".to_string(),
            },
        ],
    };
    assert!(validate_variants(&[variant]).is_ok());
}

#[test]
fn core_accepts_more_variants_than_the_web_ui_limit() {
    let variants = (0..64)
        .map(|_| WebMigrationVariant {
            mappings: vec![WebMigrationMapping {
                category: EquipmentCategory::Helmet,
                source_hash: "13f9269d08e52cf2".to_string(),
                target_hash: "a856edff49cfdd95".to_string(),
            }],
        })
        .collect::<Vec<_>>();

    assert!(validate_variants(&variants).is_ok());
}

#[test]
fn rejects_duplicate_mappings_in_one_variant() {
    let mapping = WebMigrationMapping {
        category: EquipmentCategory::Helmet,
        source_hash: "13f9269d08e52cf2".to_string(),
        target_hash: "a856edff49cfdd95".to_string(),
    };
    let variant = WebMigrationVariant {
        mappings: vec![mapping.clone(), mapping],
    };
    assert!(validate_variants(&[variant]).is_err());
}

#[test]
fn rejects_source_as_its_own_target() {
    let variant = WebMigrationVariant {
        mappings: vec![WebMigrationMapping {
            category: EquipmentCategory::Helmet,
            source_hash: "13f9269d08e52cf2".to_string(),
            target_hash: "13f9269d08e52cf2".to_string(),
        }],
    };
    assert!(validate_variants(&[variant]).is_err());
}

#[test]
fn numbers_combined_directories_when_a_batch_contains_multiple_variants() {
    let variant = WebMigrationVariant {
        mappings: vec![
            mapping(EquipmentCategory::Armor),
            mapping(EquipmentCategory::Helmet),
        ],
    };

    assert_eq!(
        variant_directory(&variant, "combined", 0, 2),
        "combined-001"
    );
    assert_eq!(
        variant_directory(&variant, "combined", 1, 2),
        "combined-002"
    );
    assert_eq!(variant_directory(&variant, "combined", 0, 1), "combined");
}

#[test]
fn identical_writes_are_deduplicated_but_different_writes_fail() {
    let mut output = archive(&[]);
    let first = archive(&[entry(7, vec![1])]);
    let same = archive(&[entry(7, vec![1])]);
    let conflict = archive(&[entry(7, vec![2])]);

    merge_output_entries(&mut output, &first).unwrap();
    merge_output_entries(&mut output, &same).unwrap();
    assert!(merge_output_entries(&mut output, &conflict).is_err());
}

#[test]
fn repeated_unit_mapping_edge_is_emitted_only_once() {
    let mut builder = VariantPatchBuilder::new(&archive(&[]), Default::default());
    let first_mapping = mapping_with_hashes(EquipmentCategory::Armor, "source", "target-a");
    let repeated_mapping = mapping_with_hashes(EquipmentCategory::Armor, "source", "target-b");
    let first_edge = UnitMappingEdge::test_edge(1, 9);
    let repeated_edge = UnitMappingEdge::test_edge(2, 9);
    let mut first_output = HashSet::from([9]);
    let mut repeated_output = HashSet::from([9]);
    let first = archive(&[entry(9, vec![1])]);
    let repeated = archive(&[entry(9, vec![2])]);

    builder
        .remove_redundant_unit_outputs(
            &first_mapping,
            &mut first_output,
            std::slice::from_ref(&first_edge),
        )
        .unwrap();
    builder
        .merge_selected_output(&HashSet::from([1]), &first_output, first)
        .unwrap();
    builder
        .remove_redundant_unit_outputs(&repeated_mapping, &mut repeated_output, &[repeated_edge])
        .unwrap();
    builder
        .merge_selected_output(&HashSet::from([1]), &repeated_output, repeated)
        .unwrap();

    assert_eq!(first_output, HashSet::from([9]));
    assert!(repeated_output.is_empty());
    assert_eq!(builder.output.find(9, UNIT_ID).unwrap().toc_data, vec![1]);
}

#[test]
fn different_source_equipment_unit_edges_conflict() {
    let mut builder = VariantPatchBuilder::new(&archive(&[]), Default::default());
    let first_mapping = mapping_with_hashes(EquipmentCategory::Armor, "source-a", "target-a");
    let second_mapping = mapping_with_hashes(EquipmentCategory::Armor, "source-b", "target-b");
    let mut first_output = HashSet::from([9]);
    let mut second_output = HashSet::from([9]);

    builder
        .remove_redundant_unit_outputs(
            &first_mapping,
            &mut first_output,
            &[UnitMappingEdge::test_edge(1, 9)],
        )
        .unwrap();
    let error = builder
        .remove_redundant_unit_outputs(
            &second_mapping,
            &mut second_output,
            &[UnitMappingEdge::test_edge(2, 9)],
        )
        .unwrap_err();

    assert!(error.to_string().contains("different source Units"));
}

#[test]
fn preferred_conflict_source_suppresses_the_other_unit_output() {
    let behavior = CompiledUnitBehavior::compile(&WebUnitBehaviorOptions {
        conflict_resolutions: vec![WebUnitConflictResolution {
            target_file_id: "9".to_string(),
            preferred_source_file_id: "2".to_string(),
        }],
        ..Default::default()
    })
    .unwrap();
    let mut builder = VariantPatchBuilder::new(&archive(&[]), behavior);
    let mut first_output = HashSet::from([9]);
    let mut second_output = HashSet::from([9]);

    builder
        .remove_redundant_unit_outputs(
            &mapping_with_hashes(EquipmentCategory::Armor, "source-a", "target"),
            &mut first_output,
            &[UnitMappingEdge::test_edge(1, 9)],
        )
        .unwrap();
    builder
        .remove_redundant_unit_outputs(
            &mapping_with_hashes(EquipmentCategory::Armor, "source-b", "target"),
            &mut second_output,
            &[UnitMappingEdge::test_edge(2, 9)],
        )
        .unwrap();

    assert!(first_output.is_empty());
    assert_eq!(second_output, HashSet::from([9]));
}

#[test]
fn export_override_removes_a_converted_unit() {
    let behavior = CompiledUnitBehavior::compile(&WebUnitBehaviorOptions {
        export_overrides: vec![WebUnitExportOverride {
            file_id: "9".to_string(),
            export: false,
        }],
        ..Default::default()
    })
    .unwrap();
    let mut builder = VariantPatchBuilder::new(&archive(&[]), behavior);
    let mut output = HashSet::from([9]);

    builder
        .remove_redundant_unit_outputs(
            &mapping_with_hashes(EquipmentCategory::Armor, "source", "target"),
            &mut output,
            &[UnitMappingEdge::test_edge(1, 9)],
        )
        .unwrap();

    assert!(output.is_empty());
}

#[test]
fn export_override_can_keep_an_unrecognized_unit_under_drop_policy() {
    let original = archive(&[entry(7, vec![1])]);
    let behavior = CompiledUnitBehavior::compile(&WebUnitBehaviorOptions {
        export_overrides: vec![WebUnitExportOverride {
            file_id: "7".to_string(),
            export: true,
        }],
        ..Default::default()
    })
    .unwrap();
    let builder = VariantPatchBuilder::new(&original, behavior);

    let output = builder.finish(&original, UnmatchedUnitPolicy::Drop);

    assert!(output.find(7, UNIT_ID).is_some());
}

#[test]
fn combines_independent_outputs_from_sources_that_share_units() {
    let original = archive(&[
        entry(1, vec![1]),
        entry(2, vec![2]),
        entry(3, vec![3]),
        entry(4, vec![4]),
    ]);
    let first = archive(&[entry(10, vec![1]), entry(11, vec![2])]);
    let second = archive(&[entry(20, vec![1]), entry(21, vec![3])]);
    let mut builder = VariantPatchBuilder::new(&original, Default::default());

    builder
        .merge_selected_output(&HashSet::from([1, 2]), &HashSet::from([10, 11]), first)
        .unwrap();
    builder
        .merge_selected_output(&HashSet::from([1, 3]), &HashSet::from([20, 21]), second)
        .unwrap();
    let combined = builder.finish(&original, UnmatchedUnitPolicy::Keep);

    assert_eq!(
        crate::operations::migration::unit_file_ids(&combined),
        HashSet::from([4, 10, 11, 20, 21]),
    );
}

#[test]
fn combined_warning_is_empty_when_every_original_unit_is_mapped() {
    let original = archive(&[entry(1, vec![1]), entry(2, vec![2])]);
    let mut builder = VariantPatchBuilder::new(&original, Default::default());
    builder
        .merge_selected_output(
            &HashSet::from([1, 2]),
            &HashSet::from([10, 20]),
            archive(&[entry(10, vec![1]), entry(20, vec![2])]),
        )
        .unwrap();

    assert!(combined_variant_warnings(&builder, &original, UnmatchedUnitPolicy::Keep).is_empty());
    assert_eq!(builder.unconverted_original_units(&original).len(), 0);
}

#[test]
fn combined_warning_reports_units_outside_configured_mappings() {
    let original = archive(&[entry(1, vec![1]), entry(2, vec![2])]);
    let mut builder = VariantPatchBuilder::new(&original, Default::default());
    builder
        .merge_selected_output(
            &HashSet::from([1]),
            &HashSet::from([10]),
            archive(&[entry(10, vec![1])]),
        )
        .unwrap();

    let keep = combined_variant_warnings(&builder, &original, UnmatchedUnitPolicy::Keep);
    let drop = combined_variant_warnings(&builder, &original, UnmatchedUnitPolicy::Drop);
    assert_eq!(builder.unconverted_original_units(&original).len(), 1);
    assert_eq!(
        keep,
        ["kept 1 part not covered by the configured equipment mappings"]
    );
    assert_eq!(
        drop,
        ["dropped 1 part not covered by the configured equipment mappings"]
    );
}

#[test]
fn drop_filter_keeps_selected_units_and_their_non_unit_content() {
    let patch = archive(&[entry(1, vec![1]), entry(2, vec![2]), TocEntry::new(9, 1234)]);
    let filtered = filter_to_units(patch, &HashSet::from([1]));

    assert!(filtered.find(1, UNIT_ID).is_some());
    assert!(filtered.find(2, UNIT_ID).is_none());
    assert!(filtered.find(9, 1234).is_some());
}

fn archive(entries: &[TocEntry]) -> StreamToc {
    StreamToc {
        entries: entries.to_vec(),
        ..Default::default()
    }
}

fn entry(file_id: u64, toc_data: Vec<u8>) -> TocEntry {
    let mut entry = TocEntry::new(file_id, UNIT_ID);
    entry.toc_data = toc_data;
    entry
}

fn mapping(category: EquipmentCategory) -> WebMigrationMapping {
    mapping_with_hashes(category, "source", "target")
}

fn mapping_with_hashes(
    category: EquipmentCategory,
    source_hash: &str,
    target_hash: &str,
) -> WebMigrationMapping {
    WebMigrationMapping {
        category,
        source_hash: source_hash.to_string(),
        target_hash: target_hash.to_string(),
    }
}
