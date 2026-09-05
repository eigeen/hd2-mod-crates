//! WASM bridge for the browser migrator.

mod api;
mod data_source;
mod error;
mod output_sink;
mod progress;
mod zip_store;

pub use api::{
    builtin_equipment_options, builtin_target_options, detect_source, inspect_equipment,
    inspect_equipment_with_source, inspect_patch, merge_head_into_armor,
    merge_head_into_armor_stream, merge_patches, merge_patches_stream, migrate_cross_archive,
    migrate_equipment_variants, repatch_units,
};
