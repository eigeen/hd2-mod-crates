//! WASM bridge for the browser migrator.

mod api;
mod data_source;
mod error;
mod progress;
mod zip_store;

pub use api::{
    builtin_target_options, detect_source, migrate_cross_archive, migrate_many, migrate_one,
};
