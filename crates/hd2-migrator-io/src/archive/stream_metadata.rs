//! Bounded normalization for invalid stream metadata in user patches.

use super::toc_only::TocOnlyPackage;
use crate::constants::{BONE_ID, UNIT_ID};

const POISON_STREAM_SIZE: u32 = 0xDFA6_4C92;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamMetadataRepair {
    pub file_id: u64,
    pub type_id: u64,
    pub poison_value_detected: bool,
}

/// Clear only out-of-bounds declarations known to be payload-free.
pub fn normalize_patch_stream_metadata(
    toc: &mut Vec<u8>,
    stream_len: usize,
) -> Result<Vec<StreamMetadataRepair>, String> {
    let mut package = TocOnlyPackage::parse(toc)
        .map_err(|error| format!("parse patch TOC for stream validation: {error}"))?;
    let mut repairs = Vec::new();
    for entry in &mut package.entries {
        if stream_declaration_is_valid(entry.stream_offset, entry.stream_size, stream_len)? {
            continue;
        }
        if !matches!(entry.type_id, UNIT_ID | BONE_ID) || stream_len != 0 {
            return Err(format!(
                "resource type=0x{:016x}, file=0x{:016x} has out-of-bounds stream metadata: offset={}, size={}, streamLength={stream_len}",
                entry.type_id, entry.file_id, entry.stream_offset, entry.stream_size
            ));
        }
        repairs.push(StreamMetadataRepair {
            file_id: entry.file_id,
            type_id: entry.type_id,
            poison_value_detected: entry.stream_size == POISON_STREAM_SIZE,
        });
        entry.stream_offset = 0;
        entry.stream_size = 0;
    }
    if !repairs.is_empty() {
        *toc = package
            .serialize()
            .map_err(|error| format!("serialize normalized patch TOC: {error}"))?;
    }
    Ok(repairs)
}

fn stream_declaration_is_valid(offset: u64, size: u32, stream_len: usize) -> Result<bool, String> {
    if size == 0 {
        return Ok(true);
    }
    let end = offset
        .checked_add(u64::from(size))
        .ok_or_else(|| format!("stream offset overflow: offset={offset}, size={size}"))?;
    Ok(end <= stream_len as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::TocFileType;
    use crate::archive::toc_only::{TocOnlyEntry, TocOnlyPackage};
    use crate::constants::TEX_ID;

    #[test]
    fn repairs_poisoned_payload_free_entry_with_empty_stream() {
        let mut toc = fixture(UNIT_ID, POISON_STREAM_SIZE);
        let repairs = normalize_patch_stream_metadata(&mut toc, 0).unwrap();
        assert_eq!(repairs.len(), 1);
        assert!(repairs[0].poison_value_detected);
        let repaired = TocOnlyPackage::parse(&toc).unwrap();
        assert_eq!(repaired.entries[0].stream_size, 0);
    }

    #[test]
    fn rejects_out_of_bounds_texture_stream() {
        let mut toc = fixture(TEX_ID, 32);
        let error = normalize_patch_stream_metadata(&mut toc, 0).unwrap_err();
        assert!(error.contains("out-of-bounds stream metadata"));
    }

    fn fixture(type_id: u64, stream_size: u32) -> Vec<u8> {
        TocOnlyPackage {
            types: vec![TocFileType::new(type_id, 1)],
            entries: vec![TocOnlyEntry {
                file_id: 42,
                type_id,
                unknown1: 0,
                unknown2: 0,
                unknown3: 16,
                unknown4: 64,
                toc_data: vec![1, 2, 3],
                stream_offset: 0,
                gpu_offset: 0,
                stream_size,
                gpu_size: 0,
            }],
            unknown: 0,
            unk4_data: [0; 56],
        }
        .serialize()
        .unwrap()
    }
}
