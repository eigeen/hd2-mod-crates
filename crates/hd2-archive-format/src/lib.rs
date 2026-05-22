//! Wasm-safe in-memory primitives for HD2 archive migration.

pub mod constants;
pub mod error;
pub mod hashing;
pub mod refs;
pub mod toc;

pub use error::{MigratorError, Result};
pub use toc::{StreamToc, TocEntry, TocFileType};
