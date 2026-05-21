//! DSAR (LZ4-compressed) container support.
//!
//! Mirrors `_decompress_dsar` in `archive.py`. Header layout:
//!
//! ```text
//! 0..4    magic = 0x52415344 "DSAR"
//! 4..8    unused
//! 8..12   num_chunks
//! 0x20 + i*0x20: chunk descriptor (32 B each)
//!   0..8   unc_off (u64)
//!   8..16  comp_off (u64)
//!   16..20 unc_sz (u32)
//!   20..24 comp_sz (u32)
//!   24..25 comp_type (u8); 3 = LZ4 block
//!   25..26 chunk_type (u8)
//!   26..32 reserved
//! ```

use crate::constants::DSAR_MAGIC;
use crate::error::MigratorError;
use byteorder::{ByteOrder, LittleEndian as LE};
use eyre::WrapErr;
use std::path::Path;

const COMP_LZ4: u8 = 3;

pub fn decompress_file(path: &Path) -> crate::Result<Vec<u8>> {
    let data = std::fs::read(path).wrap_err_with(|| format!("read DSAR {}", path.display()))?;
    decompress(&data)
}

pub fn decompress(data: &[u8]) -> crate::Result<Vec<u8>> {
    if data.len() < 0x20 {
        eyre::bail!("DSAR truncated: {} bytes", data.len());
    }
    let magic = LE::read_u32(&data[0..4]);
    if magic != DSAR_MAGIC {
        return Err(MigratorError::BadMagic {
            expected: DSAR_MAGIC,
            got: magic,
        }
        .into());
    }
    let num_chunks = LE::read_u32(&data[8..12]) as usize;
    let descriptors_end = 0x20 + num_chunks * 0x20;
    if data.len() < descriptors_end {
        eyre::bail!("DSAR descriptors truncated");
    }
    let mut out: Vec<u8> = Vec::new();
    for i in 0..num_chunks {
        let off = 0x20 + i * 0x20;
        let _unc_off = LE::read_u64(&data[off..off + 8]);
        let comp_off = LE::read_u64(&data[off + 8..off + 16]) as usize;
        let unc_sz = LE::read_u32(&data[off + 16..off + 20]) as usize;
        let comp_sz = LE::read_u32(&data[off + 20..off + 24]) as usize;
        let comp_type = data[off + 24];
        let chunk = data
            .get(comp_off..comp_off + comp_sz)
            .ok_or_else(|| eyre::eyre!("DSAR chunk OOB at offset {comp_off}"))?;
        if comp_type == COMP_LZ4 {
            let decompressed = lz4_flex::block::decompress(chunk, unc_sz)
                .map_err(|e| MigratorError::Lz4(e.to_string()))?;
            out.extend_from_slice(&decompressed);
        } else {
            // uncompressed chunk passthrough
            out.extend_from_slice(chunk);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_dsar_magic() {
        let bad = vec![0u8; 0x20];
        let err = decompress(&bad).unwrap_err();
        assert!(format!("{err}").contains("bad magic"));
    }
}
