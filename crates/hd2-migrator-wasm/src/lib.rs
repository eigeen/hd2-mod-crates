//! WASM bridge for the browser migrator.

mod api;
mod error;
mod zip_store;

pub use api::{builtin_target_options, detect_source, migrate_many, migrate_one};
