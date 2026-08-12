//! Browser-oriented migration entry points.
//!
//! Two entry points coexist:
//! - [`migrate_many`]: same-source pass-through only (no game data access).
//!   Identifies the source via the authoritative armor mapping and outputs
//!   the patch unchanged under the source archive's directory.
//! - [`migrate_many_with_source`]: full Mode A cross-archive migration backed
//!   by an async [`crate::io::DataSource`] (browser supplies it via the File
//!   System Access API; native callers can use [`crate::io::NativeDataSource`]).

pub mod migration;
pub mod repatch;

pub use crate::migrator::mode_a_web::WebProgress;
pub use migration::{
    PatchBytes, WebMigrateOptions, WebMigrationBundle, WebMigrationReportRow, WebMigrationSummary,
    WebOutputFile, WebTargetOption, detect_source_archive, list_target_options, migrate_many,
    migrate_many_with_source, migrate_one,
};
pub use repatch::{
    MissingUnitPolicy, UnitRepatchOptions, UnitRepatchResult, UnitRepatchSummary, repatch_units,
};
