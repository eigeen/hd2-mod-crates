//! Murmur64A hash (HD2 variant) and the 32-bit truncation used for slot IDs.
//!
//! Ports `mod_armor_migrator/hashing.py`:
//! - constant `M = 0xc6a4a7935bd1e995`, `r = 47`
//! - default `seed = 0`
//! - `murmur32 = (murmur64 >> 32) as u32` (HIGH 32 bits, not low!)

const M: u64 = 0xc6a4_a793_5bd1_e995;
const R: u32 = 47;

pub fn murmur64(data: &[u8], seed: u64) -> u64 {
    let mut h = seed ^ (M.wrapping_mul(data.len() as u64));
    let chunks = data.chunks_exact(8);
    let remainder = chunks.remainder();
    for chunk in chunks {
        let mut k = u64::from_le_bytes(chunk.try_into().expect("8 bytes"));
        k = k.wrapping_mul(M);
        k ^= k >> R;
        k = k.wrapping_mul(M);
        h ^= k;
        h = h.wrapping_mul(M);
    }
    let tail_len = remainder.len();
    if tail_len >= 7 {
        h ^= (remainder[6] as u64) << 48;
    }
    if tail_len >= 6 {
        h ^= (remainder[5] as u64) << 40;
    }
    if tail_len >= 5 {
        h ^= (remainder[4] as u64) << 32;
    }
    if tail_len >= 4 {
        h ^= (remainder[3] as u64) << 24;
    }
    if tail_len >= 3 {
        h ^= (remainder[2] as u64) << 16;
    }
    if tail_len >= 2 {
        h ^= (remainder[1] as u64) << 8;
    }
    if tail_len >= 1 {
        h ^= remainder[0] as u64;
        h = h.wrapping_mul(M);
    }

    h ^= h >> R;
    h = h.wrapping_mul(M);
    h ^= h >> R;
    h
}

/// HD2 slot IDs: HIGH 32 bits of murmur64.
#[inline]
pub fn murmur32(data: &[u8], seed: u64) -> u32 {
    (murmur64(data, seed) >> 32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::*;

    #[test]
    fn type_names_round_trip_to_constants_py() {
        // From constants.py: each TypeID is murmur64(typename, seed=0).
        // CompositeUnitID's source string is not trivially derivable; not pinned here.
        assert_eq!(murmur64(b"unit", 0), UNIT_ID);
        assert_eq!(murmur64(b"texture", 0), TEX_ID);
        assert_eq!(murmur64(b"material", 0), MATERIAL_ID);
        assert_eq!(murmur64(b"bones", 0), BONE_ID);
        assert_eq!(murmur64(b"animation", 0), ANIMATION_ID);
        assert_eq!(murmur64(b"state_machine", 0), STATE_MACHINE_ID);
        assert_eq!(murmur64(b"particles", 0), PARTICLE_ID);
        assert_eq!(murmur64(b"strings", 0), STRING_ID);
        assert_eq!(murmur64(b"physics", 0), PHYSICS_ID);
        assert_eq!(murmur64(b"wwise_bank", 0), WWISE_BANK_ID);
        assert_eq!(murmur64(b"wwise_dep", 0), WWISE_DEP_ID);
        assert_eq!(murmur64(b"wwise_stream", 0), WWISE_STREAM_ID);
        assert_eq!(murmur64(b"wwise_metadata", 0), WWISE_METADATA_ID);
    }

    #[test]
    fn murmur32_takes_high_bits() {
        // pin: high half, not low half
        let data = b"some slot name";
        let full = murmur64(data, 0);
        let half = murmur32(data, 0);
        assert_eq!(u64::from(half), full >> 32);
        assert_ne!(u64::from(half), full & 0xFFFF_FFFF);
    }
}
