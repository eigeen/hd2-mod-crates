//! Platform-neutral migration operations shared by native and browser hosts.

pub mod archive_output;
pub mod equipment;
pub mod equipment_graph;
pub mod head_armor_merge;
pub mod mapping_preview;
pub mod migration;
pub mod patch_merge;
pub mod repatch;
pub mod unified_migration;

pub use archive_output::{
    ArchiveOutputEntry, ArchiveOutputFile, ArchiveOutputManifest, PreparedArchiveOutput,
};

pub use crate::migrator::mode_a_web::WebProgress;
pub use equipment::{
    EquipmentCategory, WebDetectedSource, WebEquipmentInspection, WebEquipmentOption,
    WebMigrationMapping, inspect_equipment, inspect_equipment_with_source, list_equipment_options,
};
pub use equipment_graph::{
    EQUIPMENT_GRAPH_SCHEMA_VERSION, EquipmentPartRole, WebEquipmentGraphDiagnostic,
    WebEquipmentGraphDiagnosticCode, WebEquipmentGraphSummary, WebEquipmentPartGraph,
    WebEquipmentPartRelation, WebEquipmentPatchAnalysis, WebGraphComponent, WebGraphComponentKind,
    WebGraphEquipment, analyze_equipment_patch, analyze_equipment_patch_with_source,
    build_equipment_part_graph,
};
pub use head_armor_merge::{
    HeadArmorMergeOptions, HeadArmorMergeResult, HeadArmorMergeSkippedPart, HeadArmorMergeSummary,
    HeadArmorMergedPart, PreparedHeadArmorMerge, merge_head_into_armor, prepare_head_armor_merge,
};
pub use mapping_preview::{
    MAPPING_PREVIEW_SCHEMA_VERSION, WebEquipmentMappingPreview, WebMappingPreviewSummary,
    WebMappingPreviewUnit, WebUnitMappingAction, WebUnitMappingPreview, preview_equipment_mapping,
    preview_equipment_mappings,
};
pub use migration::{
    PatchBytes, UnmatchedUnitPolicy, WebDetectedModel, WebMigrateOptions, WebMigrationBundle,
    WebMigrationReportRow, WebMigrationSummary, WebOutputFile, WebPatchInspection, WebTargetOption,
    detect_patch_models, detect_source_archive, inspect_patch, list_target_options,
    migrate_many_with_source,
};
pub use patch_merge::{
    PatchMergeResult, PatchMergeSourceSummary, PatchMergeSummary, PreparedPatchMerge,
    merge_patches, prepare_patch_merge,
};
pub use repatch::{
    MissingUnitPolicy, UnitRepatchOptions, UnitRepatchPlan, UnitRepatchResult, UnitRepatchSummary,
    repatch_patch_plan_with_progress, repatch_patch_with_progress, repatch_units,
    repatch_units_plan_with_progress, repatch_units_with_progress,
};
#[cfg(not(target_family = "wasm"))]
pub use unified_migration::{
    ParallelVariantPatchCallbacks, migrate_variants_to_patch_sink_parallel,
};
pub use unified_migration::{
    VariantMigrationCallbacks, VariantPatchCallbacks, VariantPatchOutput, WebMigrationVariant,
    WebUnifiedMigrateOptions, WebUnitBehaviorOptions, WebUnitConflictResolution,
    WebUnitExportOverride, WebUnitMappingBehaviorKey, migrate_variants_to_patch_sink,
    migrate_variants_to_sink, migrate_variants_with_source,
};
