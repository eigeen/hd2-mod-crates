//! Browser-oriented migration entry points.
//!
//! The browser surface intentionally trades cross-archive migration for
//! safety: the native filesystem migrator remains the only path that can
//! synthesize real cross-archive output. WASM callers list targets from the
//! bundled archive index, identify the source via the authoritative armor
//! mapping, and may pass-through to the source archive itself.

pub mod migration;

pub use migration::{
    PatchBytes, WebMigrateOptions, WebMigrationBundle, WebMigrationReportRow, WebMigrationSummary,
    WebOutputFile, WebTargetOption, detect_source_archive, list_target_options, migrate_many,
    migrate_one,
};
