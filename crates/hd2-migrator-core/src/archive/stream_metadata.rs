//! Compatibility helper for rewriting stream declarations through the shared reader.

use super::toc_only::TocOnlyPackage;
pub use hd2_archive_format::stream_metadata::StreamMetadataRepair;

/// Rewrites only when the shared reader reports corrected stream declarations.
pub fn normalize_patch_stream_metadata(
    toc: &mut Vec<u8>,
    stream_len: usize,
) -> Result<Vec<StreamMetadataRepair>, String> {
    let (package, repairs) = TocOnlyPackage::parse_with_stream_metadata(toc, Some(stream_len))
        .map_err(|error| format!("parse patch TOC for stream repair: {error}"))?;
    if !repairs.is_empty() {
        *toc = package
            .serialize()
            .map_err(|error| format!("serialize repaired patch TOC: {error}"))?;
    }
    Ok(repairs)
}
#[cfg(test)]
mod tests {
    use super::super::toc_only::TocOnlyEntry;
    use super::*;
    use crate::constants::{TEX_ID, UNIT_ID};
    use byteorder::{ByteOrder, LittleEndian as LE};
    use hd2_archive_format::stream_metadata::POISON_STREAM_SIZE;
    const TEXTURE_FIRST_MIP_BYTES_LEFT_OFFSET: usize = 16;
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
