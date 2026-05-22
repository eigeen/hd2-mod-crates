//! `bundles.*.nxa` slim install reassembly.
//!
//! Thin compatibility wrapper around [`super::BundleIndex`]. New code should
//! prefer constructing one index and reusing it for all package reads.

use std::path::Path;

#[allow(dead_code)]
pub fn reassemble(data_dir: &Path, file_id_hex: &str) -> crate::Result<Vec<u8>> {
    let bundle_index = super::BundleIndex::from_data_dir(data_dir)?;
    bundle_index.load_package(file_id_hex)
}
