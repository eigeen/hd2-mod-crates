//! Migration-specific constants plus shared archive-format re-exports.

pub use hd2_archive_format::constants::{
    ANIMATION_ID, BONE_ID, COMPOSITE_UNIT_ID, DSAR_MAGIC, GPU_ALIGN, LEGACY_MAGIC, MATERIAL_ID,
    PARTICLE_ID, PHYSICS_ID, STATE_MACHINE_ID, STREAM_ALIGN, STRING_ID, TEX_ID, UNIT_ID,
    WWISE_BANK_ID, WWISE_DEP_ID, WWISE_METADATA_ID, WWISE_STREAM_ID, align_up, type_name,
};

pub const BASE_ARCHIVE_HEX_ID: &str = "9ba626afa44a3aa3";
pub const MIN_FILE_PADDING: usize = 256;

#[inline]
pub const fn pad_to_min(n: usize, min: usize) -> usize {
    if n < min { min } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_boundaries() {
        assert_eq!(align_up(0, 64), 0);
        assert_eq!(align_up(1, 64), 64);
        assert_eq!(align_up(63, 64), 64);
        assert_eq!(align_up(64, 64), 64);
        assert_eq!(align_up(65, 64), 128);
        assert_eq!(align_up(255, 64), 256);
        assert_eq!(align_up(256, 64), 256);
        assert_eq!(align_up(257, 64), 320);
    }

    #[test]
    fn pad_to_min_floor() {
        assert_eq!(pad_to_min(0, 256), 256);
        assert_eq!(pad_to_min(255, 256), 256);
        assert_eq!(pad_to_min(256, 256), 256);
        assert_eq!(pad_to_min(257, 256), 257);
    }
}
