use crate::error::Result;
use crate::metadata::ZstdParams;
use std::io::{Cursor, Read, Write};
use zstd::zstd_safe::{CParameter, DParameter};

const WINDOW_LOG_MIN: u32 = 10;
const WINDOW_LOG_MAX: u32 = 31;

/// Compresses target bytes using the base bytes as a zstd raw ref-prefix.
pub fn create_patch(base: &[u8], target: &[u8], level: i32) -> Result<(Vec<u8>, ZstdParams)> {
    let params = params_for_patch(target.len() as u64, level);
    let mut encoder = zstd::stream::write::Encoder::with_ref_prefix(Vec::new(), level, base)?;

    encoder.set_parameter(CParameter::WindowLog(params.window_log))?;
    encoder.set_parameter(CParameter::EnableLongDistanceMatching(
        params.long_distance_matching,
    ))?;
    encoder.write_all(target)?;

    Ok((encoder.finish()?, params))
}

/// Applies a patch created by `create_patch` with the same base bytes.
pub fn apply_patch(base: &[u8], patch: &[u8], params: &ZstdParams) -> Result<Vec<u8>> {
    let mut decoder = zstd::stream::read::Decoder::with_ref_prefix(Cursor::new(patch), base)?;
    decoder.set_parameter(DParameter::WindowLogMax(params.window_log))?;

    let mut output = Vec::new();
    decoder.read_to_end(&mut output)?;
    Ok(output)
}

/// Compresses a standalone file for variants that do not have a base file.
pub fn compress_full(bytes: &[u8], level: i32) -> Result<Vec<u8>> {
    Ok(zstd::stream::encode_all(Cursor::new(bytes), level)?)
}

/// Decompresses a standalone full-file payload.
pub fn decompress_full(bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(zstd::stream::decode_all(Cursor::new(bytes))?)
}

fn params_for_patch(target_size: u64, level: i32) -> ZstdParams {
    let window_log = ceil_log2(target_size.max(1)).clamp(WINDOW_LOG_MIN, WINDOW_LOG_MAX);
    let long_distance_matching = window_log > 23;

    ZstdParams {
        level,
        window_log,
        long_distance_matching,
    }
}

fn ceil_log2(value: u64) -> u32 {
    if value <= 1 {
        return 0;
    }
    u64::BITS - (value - 1).leading_zeros()
}

#[cfg(test)]
mod tests {
    use super::{apply_patch, create_patch};

    #[test]
    fn prefix_patch_roundtrip() {
        let base = b"hello super earth base data";
        let target = b"hello super earth patched target data";
        let (patch, params) = create_patch(base, target, 3).unwrap();
        let restored = apply_patch(base, &patch, &params).unwrap();

        assert_eq!(restored, target);
    }
}
