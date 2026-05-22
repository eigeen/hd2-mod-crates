//! Migrate Helldivers 2 armor mod patches across all armor archives.
//!
//! Library entry points live under [`migrator`].

pub mod archive;
pub mod constants;
pub mod error;
pub mod hashing;
pub mod index;
pub mod migrator;
pub mod padding;
pub mod refs;
pub mod target_exclusions;
pub mod unit;

pub use error::{MigratorError, Result};
pub use index::ArchiveIndex;
pub use migrator::{migrate_all, MigrateAllOpts, MigrationReport, ProgressSink};
pub use padding::{builtin_template, extract_template, EmptyUnitTemplate, PaddingMode};
