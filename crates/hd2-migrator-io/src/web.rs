//! Browser-oriented migration entry points.
//!
//! This module keeps Web/WASM callers on byte buffers and JSON metadata so the
//! native filesystem migrator can remain unchanged.

pub mod extract;
pub mod metadata;
pub mod migration;

pub use extract::{ExtractMetadataOptions, extract_game_metadata};
pub use metadata::{WebArchiveMetadata, WebGameMetadata, WebTargetOption};
pub use migration::{
    PatchBytes, WebMigrateOptions, WebMigrationBundle, WebMigrationReportRow, WebMigrationSummary,
    WebOutputFile, detect_source_archive, list_target_options, migrate_many, migrate_one,
};
