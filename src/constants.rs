//! Helldivers 2 archive constants and alignment helpers.
//!
//! Mirrors `mod_armor_migrator/constants.py`. The TypeID values are the
//! murmur64 hashes of the type names ("Unit", "Material", ...) seeded with 0.

pub const BASE_ARCHIVE_HEX_ID: &str = "9ba626afa44a3aa3";

pub const COMPOSITE_UNIT_ID: u64 = 14_191_111_524_867_688_662;
pub const UNIT_ID: u64 = 16_187_218_042_980_615_487;
pub const TEX_ID: u64 = 14_790_446_551_990_181_426;
pub const MATERIAL_ID: u64 = 16_915_718_763_308_572_383;
pub const BONE_ID: u64 = 1_792_059_921_637_536_489;
pub const WWISE_BANK_ID: u64 = 6_006_249_203_084_351_385;
pub const WWISE_DEP_ID: u64 = 12_624_162_998_411_505_776;
pub const WWISE_STREAM_ID: u64 = 5_785_811_756_662_211_598;
pub const WWISE_METADATA_ID: u64 = 15_351_235_653_606_224_144;
pub const PARTICLE_ID: u64 = 12_112_766_700_566_326_628;
pub const ANIMATION_ID: u64 = 10_600_967_118_105_529_382;
pub const STATE_MACHINE_ID: u64 = 11_855_396_184_103_720_540;
pub const STRING_ID: u64 = 979_299_457_696_010_195;
pub const PHYSICS_ID: u64 = 6_877_563_742_545_042_104;

pub fn type_name(type_id: u64) -> Option<&'static str> {
    Some(match type_id {
        UNIT_ID => "Unit",
        COMPOSITE_UNIT_ID => "CompositeUnit",
        TEX_ID => "Texture",
        MATERIAL_ID => "Material",
        BONE_ID => "Bones",
        ANIMATION_ID => "Animation",
        STATE_MACHINE_ID => "StateMachine",
        PARTICLE_ID => "Particle",
        WWISE_BANK_ID => "WwiseBank",
        WWISE_DEP_ID => "WwiseDep",
        WWISE_STREAM_ID => "WwiseStream",
        WWISE_METADATA_ID => "WwiseMetadata",
        STRING_ID => "String",
        PHYSICS_ID => "Physics",
        _ => return None,
    })
}

/// `0xF0000011`. Note: HD2 docs misprint as `0xF0000004`.
pub const LEGACY_MAGIC: u32 = 0xF000_0011;
/// `"DSAR"` little-endian.
pub const DSAR_MAGIC: u32 = 0x5241_5344;

pub const GPU_ALIGN: usize = 64;
pub const STREAM_ALIGN: usize = 64;
pub const MIN_FILE_PADDING: usize = 256;

#[inline]
pub const fn align_up(n: usize, alignment: usize) -> usize {
    (n + alignment - 1) / alignment * alignment
}

#[inline]
pub const fn pad_to_min(n: usize, min: usize) -> usize {
    if n < min {
        min
    } else {
        n
    }
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
