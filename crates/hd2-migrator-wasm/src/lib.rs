//! WASM bridge for the browser migrator.

mod api;
mod error;
mod zip_store;

pub use api::{
    build_metadata, builtin_target_options, detect_source, list_targets, migrate_many, migrate_one,
};
