//! Migrate Helldivers 2 armor mod patches across all armor archives.
//!
//! Library entry points live under [`migrator`].

pub mod archive;
pub mod constants;
pub mod error;
pub mod hashing;
pub mod index;
pub mod io;
pub mod migrator;
pub mod padding;
pub mod refs;
pub mod target_exclusions;
pub mod unit;
pub mod web;

pub use error::{MigratorError, Result};
pub use index::ArchiveIndex;
pub use migrator::{MigrateAllOpts, MigrationReport, ProgressSink, migrate_all};
pub use padding::{EmptyUnitTemplate, PaddingMode, builtin_template, extract_template};
