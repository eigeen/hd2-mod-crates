const FNV_OFFSET_A: u64 = 0xcbf29ce484222325;
const FNV_OFFSET_B: u64 = 0x6c62272e07bb0142;
const FNV_PRIME: u64 = 0x00000100000001b3;

/// Computes a stable content hash without relying on platform hashers.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut a = FNV_OFFSET_A;
    let mut b = FNV_OFFSET_B;

    for byte in bytes {
        a ^= u64::from(*byte);
        a = a.wrapping_mul(FNV_PRIME);
        b ^= u64::from(byte.rotate_left(1));
        b = b.wrapping_mul(FNV_PRIME ^ 0x9e3779b97f4a7c15);
    }

    format!("{a:016x}{b:016x}")
}

#[cfg(test)]
mod tests {
    use super::content_hash;

    #[test]
    fn hash_is_stable() {
        assert_eq!(content_hash(b"abc"), "e71fa2190541574b3b774dd288a58bf4");
    }
}
