//! Shared migration engine for Helldivers 2 mod patches.
//!
//! Desktop and WASM hosts use [`operations`] for migration, Unit repair and
//! archive merging. [`archive`] and [`unit`] own the runtime models and algorithms.
//! [`io::DataSource`] abstracts game-data reads; native filesystem helpers remain
//! under [`io`] and the CLI orchestration entry points under [`migrator`].
//! Host UI, IPC, browser bindings and final ZIP delivery live outside this crate.

pub mod archive;
pub mod constants;
pub mod error;
pub mod hashing;
pub mod index;
pub mod io;
pub mod migrator;
pub mod operations;
pub mod padding;
pub mod refs;
pub mod target_exclusions;
pub mod unit;
pub mod web;

pub use error::{MigratorError, Result};
pub use index::ArchiveIndex;
pub use migrator::{MigrateAllOpts, MigrationReport, ProgressSink, migrate_all};
pub use padding::{EmptyUnitTemplate, PaddingMode, builtin_template, extract_template};
