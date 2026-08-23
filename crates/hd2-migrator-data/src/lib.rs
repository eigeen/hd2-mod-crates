//! Embedded data assets shared by migrator crates.

pub const ARCHIVE_INDEX_JSON: &str = include_str!("../../../assets/archivehashes.json");
pub const ARCHIVE_INDEX_OVERRIDES_JSON: &str =
    include_str!("../../../assets/archivehash_overrides.json");
pub const ARMOR_MAPPING_JSON: &str = include_str!("../../../assets/armor_mappings.merged.json");
pub const HELMET_MAPPING_JSON: &str = include_str!("../../../assets/helmet_mappings.json");
pub const BONEHASH_TEXT: &str = include_str!("../../../assets/bonehash.txt");

pub const EMPTY_MESH_TOC: &[u8] = include_bytes!("../../../assets/empty_mesh/toc.bin");
pub const EMPTY_MESH_GPU: &[u8] = include_bytes!("../../../assets/empty_mesh/gpu.bin");
pub const EMPTY_MESH_STREAM: &[u8] = include_bytes!("../../../assets/empty_mesh/stream.bin");
