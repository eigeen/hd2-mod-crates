//! Shared HD2 archive constants.

pub const LEGACY_MAGIC: u32 = 0xF000_0011;
pub const DSAR_MAGIC: u32 = 0x5241_5344;

// Stingray TypeIDs are Murmur64A hashes of their lowercase type names.
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

pub const GPU_ALIGN: usize = 64;
pub const STREAM_ALIGN: usize = 64;

pub fn align_up(value: usize, align: usize) -> usize {
    if align == 0 {
        return value;
    }
    value.div_ceil(align) * align
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toc::list_file_ids_from_bytes;
    use std::path::Path;

    #[test]
    fn type_ids_match_real_hd2_patch_entries() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test_files/DP-8/9ba626afa44a3aa3.patch_0");
        let bytes = std::fs::read(path).expect("read real patch fixture");
        let by_type = list_file_ids_from_bytes(&bytes).expect("parse real patch fixture");

        assert_eq!(by_type.get(&UNIT_ID).map(Vec::len), Some(22));
        assert_eq!(by_type.get(&MATERIAL_ID).map(Vec::len), Some(6));
        assert_eq!(by_type.get(&TEX_ID).map(Vec::len), Some(18));
    }
}
