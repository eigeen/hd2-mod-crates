use serde::{Deserialize, Serialize};

pub const METADATA_FILE: &str = "variant-dist.json";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageMetadata {
    pub schema_version: u32,
    pub mod_info: ModInfo,
    pub base_variant: String,
    pub variants: Vec<VariantMetadata>,
    pub assets: Vec<AssetRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manifest_options: Vec<StoredManifestOption>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModInfo {
    pub name: Option<String>,
    pub description: Option<String>,
    pub guid: Option<String>,
    pub icon_path: Option<String>,
}

/// Canonical Option metadata from the source manifest, carried through the patch package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredManifestOption {
    pub name: String,
    pub include: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VariantMetadata {
    pub name: String,
    pub files: Vec<FileRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetRecord {
    pub relative_path: String,
    pub size: u64,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileRecord {
    pub relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_relative_path: Option<String>,
    pub operation: FileOperation,
    pub source_size: Option<u64>,
    pub target_size: Option<u64>,
    pub base_hash: Option<String>,
    pub target_hash: Option<String>,
    pub stored_path: Option<String>,
    pub zstd: Option<ZstdParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileOperation {
    Base,
    SameAsBase,
    Patch,
    Full,
    Delete,
    Empty,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZstdParams {
    pub level: i32,
    pub window_log: u32,
    pub long_distance_matching: bool,
}
