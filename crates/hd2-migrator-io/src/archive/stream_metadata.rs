//! Bounded normalization for invalid stream metadata in user patches.

use super::TOC_ENTRY_SIZE;
use super::toc_only::{TocHeader, TocOnlyEntry, TocOnlyPackage};
use crate::constants::{BONE_ID, TEX_ID, UNIT_ID};
use byteorder::{ByteOrder, LittleEndian as LE};

const POISON_STREAM_SIZE: u32 = 0xDFA6_4C92;
const TEXTURE_FIRST_MIP_BYTES_LEFT_OFFSET: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamMetadataRepair {
    pub file_id: u64,
    pub type_id: u64,
    pub original_size: u32,
    pub repaired_size: u32,
    pub poison_value_detected: bool,
}

/// Recalculate mismatched stream sizes before any sidecar slice is attempted.
pub fn normalize_patch_stream_metadata(
    toc: &mut Vec<u8>,
    stream_len: usize,
) -> Result<Vec<StreamMetadataRepair>, String> {
    if !has_mismatched_stream_metadata(toc, stream_len)? {
        return Ok(Vec::new());
    }
    let mut package = TocOnlyPackage::parse(toc)
        .map_err(|error| format!("parse patch TOC for stream repair: {error}"))?;
    let repairs = calculate_repairs(&package.entries, stream_len)?;
    apply_repairs(&mut package.entries, &repairs);
    *toc = package
        .serialize()
        .map_err(|error| format!("serialize repaired patch TOC: {error}"))?;
    Ok(repairs.into_iter().map(|repair| repair.details).collect())
}

#[derive(Debug)]
struct IndexedRepair {
    entry_index: usize,
    details: StreamMetadataRepair,
}

fn has_mismatched_stream_metadata(toc: &[u8], stream_len: usize) -> Result<bool, String> {
    let header = TocHeader::parse(toc)
        .map_err(|error| format!("parse patch TOC for stream validation: {error}"))?;
    let table = stream_entry_table(toc, &header)?;
    Ok(table.chunks_exact(TOC_ENTRY_SIZE).any(|raw| {
        let (type_id, offset, size) = read_stream_declaration(raw);
        declaration_mismatches(type_id, offset, size, stream_len)
    }))
}

fn stream_entry_table<'a>(toc: &'a [u8], header: &TocHeader) -> Result<&'a [u8], String> {
    let table_end = header
        .table_size()
        .map_err(|error| format!("parse patch TOC for stream validation: {error}"))?;
    let entry_bytes = header
        .num_files
        .checked_mul(TOC_ENTRY_SIZE)
        .ok_or_else(|| "parse patch TOC for stream validation: entry table overflow".to_owned())?;
    let table_start = table_end - entry_bytes;
    toc.get(table_start..table_end).ok_or_else(|| {
        "parse patch TOC for stream validation: header table is truncated".to_owned()
    })
}

fn read_stream_declaration(raw: &[u8]) -> (u64, u64, u32) {
    (
        LE::read_u64(&raw[8..16]),
        LE::read_u64(&raw[24..32]),
        LE::read_u32(&raw[60..64]),
    )
}

fn declaration_mismatches(type_id: u64, offset: u64, size: u32, stream_len: usize) -> bool {
    if matches!(type_id, UNIT_ID | BONE_ID) {
        return size != 0;
    }
    !stream_declaration_is_in_bounds(offset, size, stream_len)
}

fn calculate_repairs(
    entries: &[TocOnlyEntry],
    stream_len: usize,
) -> Result<Vec<IndexedRepair>, String> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            declaration_mismatches(
                entry.type_id,
                entry.stream_offset,
                entry.stream_size,
                stream_len,
            )
        })
        .map(|(index, entry)| calculate_repair(index, entry, stream_len))
        .collect()
}

fn calculate_repair(
    entry_index: usize,
    entry: &TocOnlyEntry,
    stream_len: usize,
) -> Result<IndexedRepair, String> {
    let repaired_size = calculate_actual_stream_size(entry, stream_len)?;
    validate_repaired_interval(entry, repaired_size, stream_len)?;
    Ok(IndexedRepair {
        entry_index,
        details: StreamMetadataRepair {
            file_id: entry.file_id,
            type_id: entry.type_id,
            original_size: entry.stream_size,
            repaired_size,
            poison_value_detected: entry.stream_size == POISON_STREAM_SIZE,
        },
    })
}

/// HD2SDK writes Unit/Bones with empty StreamData and stores a texture's total
/// streamed byte count in the first mip record's BytesLeft field.
fn calculate_actual_stream_size(entry: &TocOnlyEntry, stream_len: usize) -> Result<u32, String> {
    if matches!(entry.type_id, UNIT_ID | BONE_ID) || stream_len == 0 {
        return Ok(0);
    }
    if entry.type_id == TEX_ID {
        return texture_stream_size(&entry.toc_data);
    }
    Err(resource_error(
        entry,
        stream_len,
        "the resource type has no exact stream-size calculator",
    ))
}

fn texture_stream_size(toc_data: &[u8]) -> Result<u32, String> {
    let end = TEXTURE_FIRST_MIP_BYTES_LEFT_OFFSET + size_of::<u32>();
    let bytes = toc_data
        .get(TEXTURE_FIRST_MIP_BYTES_LEFT_OFFSET..end)
        .ok_or_else(|| "texture TOC body is too small to read first mip BytesLeft".to_owned())?;
    Ok(LE::read_u32(bytes))
}

fn validate_repaired_interval(
    entry: &TocOnlyEntry,
    repaired_size: u32,
    stream_len: usize,
) -> Result<(), String> {
    if stream_declaration_is_in_bounds(entry.stream_offset, repaired_size, stream_len) {
        return Ok(());
    }
    Err(resource_error(
        entry,
        stream_len,
        &format!("calculated stream size {repaired_size} is still out of bounds"),
    ))
}

fn stream_declaration_is_in_bounds(offset: u64, size: u32, stream_len: usize) -> bool {
    size == 0
        || offset
            .checked_add(u64::from(size))
            .is_some_and(|end| end <= stream_len as u64)
}

fn resource_error(entry: &TocOnlyEntry, stream_len: usize, reason: &str) -> String {
    format!(
        "resource type=0x{:016x}, file=0x{:016x} has mismatched stream metadata: offset={}, size={}, streamLength={stream_len}; {reason}",
        entry.type_id, entry.file_id, entry.stream_offset, entry.stream_size
    )
}

fn apply_repairs(entries: &mut [TocOnlyEntry], repairs: &[IndexedRepair]) {
    for repair in repairs {
        entries[repair.entry_index].stream_size = repair.details.repaired_size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::TocFileType;

    #[test]
    fn calculates_payload_free_size_from_resource_semantics() {
        let mut toc = fixture(UNIT_ID, 17, POISON_STREAM_SIZE, vec![1, 2, 3]);
        let repairs = normalize_patch_stream_metadata(&mut toc, 0).unwrap();
        assert_eq!(repairs.len(), 1);
        assert_eq!(repairs[0].original_size, POISON_STREAM_SIZE);
        assert_eq!(repairs[0].repaired_size, 0);
        assert!(repairs[0].poison_value_detected);
        let repaired = TocOnlyPackage::parse(&toc).unwrap();
        assert_eq!(repaired.entries[0].stream_offset, 17);
        assert_eq!(repaired.entries[0].stream_size, 0);
    }

    #[test]
    fn calculates_texture_size_from_first_mip_metadata() {
        let mut texture_toc = vec![0; 20];
        LE::write_u32(
            &mut texture_toc[TEXTURE_FIRST_MIP_BYTES_LEFT_OFFSET..20],
            32,
        );
        let mut toc = fixture(TEX_ID, 8, POISON_STREAM_SIZE, texture_toc);
        let repairs = normalize_patch_stream_metadata(&mut toc, 40).unwrap();
        assert_eq!(repairs[0].repaired_size, 32);
        let repaired = TocOnlyPackage::parse(&toc).unwrap();
        assert_eq!(repaired.entries[0].stream_size, 32);
    }

    #[test]
    fn skips_repair_work_when_declarations_match() {
        let original = fixture(TEX_ID, 4, 8, vec![0; 20]);
        let mut toc = original.clone();
        assert!(
            normalize_patch_stream_metadata(&mut toc, 12)
                .unwrap()
                .is_empty()
        );
        assert_eq!(toc, original);
    }

    #[test]
    fn rejects_unknown_non_empty_stream_size_mismatch() {
        let mut toc = fixture(0x1234, 0, POISON_STREAM_SIZE, vec![1, 2, 3]);
        let error = normalize_patch_stream_metadata(&mut toc, 16).unwrap_err();
        assert!(error.contains("no exact stream-size calculator"));
    }

    #[test]
    fn calculates_zero_for_any_resource_when_stream_is_empty() {
        let mut toc = fixture(TEX_ID, 0, 32, vec![1, 2, 3]);
        let repairs = normalize_patch_stream_metadata(&mut toc, 0).unwrap();
        assert_eq!(repairs[0].repaired_size, 0);
    }

    fn fixture(type_id: u64, stream_offset: u64, stream_size: u32, toc_data: Vec<u8>) -> Vec<u8> {
        TocOnlyPackage {
            types: vec![TocFileType::new(type_id, 1)],
            entries: vec![TocOnlyEntry {
                file_id: 42,
                type_id,
                unknown1: 0,
                unknown2: 0,
                unknown3: 16,
                unknown4: 64,
                toc_data,
                stream_offset,
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
