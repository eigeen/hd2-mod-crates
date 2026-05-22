//! Shared HD2 archive constants.

pub const LEGACY_MAGIC: u32 = 0xF000_0011;
pub const DSAR_MAGIC: u32 = 0x5241_5344;

pub const UNIT_ID: u64 = 0x9e7b_54b7_a25f_0f56;
pub const MATERIAL_ID: u64 = 0x3d1f_bccf_0f7c_0d08;
pub const TEX_ID: u64 = 0x7e6c_15d3_46f5_a80c;

pub const GPU_ALIGN: usize = 16;
pub const STREAM_ALIGN: usize = 16;

pub fn align_up(value: usize, align: usize) -> usize {
    if align == 0 {
        return value;
    }
    value.div_ceil(align) * align
}

pub fn type_name(type_id: u64) -> Option<&'static str> {
    match type_id {
        UNIT_ID => Some("Unit"),
        MATERIAL_ID => Some("Material"),
        TEX_ID => Some("Texture"),
        _ => None,
    }
}
