//! Shared Patch sidecar length validation.

use super::toc_only::{TocOnlyEntry, TocOnlyPackage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidecarRequirements {
    pub gpu: u64,
    pub stream: u64,
}

/// Calculate the bytes actually referenced by non-empty sidecar intervals.
pub fn patch_sidecar_requirements(toc: &[u8]) -> Result<SidecarRequirements, String> {
    let package = TocOnlyPackage::parse(toc)
        .map_err(|error| format!("parse patch TOC for sidecar validation: {error}"))?;
    Ok(SidecarRequirements {
        gpu: required_length(&package.entries, SidecarKind::Gpu)?,
        stream: required_length(&package.entries, SidecarKind::Stream)?,
    })
}

#[derive(Debug, Clone, Copy)]
enum SidecarKind {
    Gpu,
    Stream,
}

fn required_length(entries: &[TocOnlyEntry], kind: SidecarKind) -> Result<u64, String> {
    entries.iter().try_fold(0, |required, entry| {
        let (offset, size) = sidecar_interval(entry, kind);
        if size == 0 {
            return Ok(required);
        }
        let end = offset.checked_add(u64::from(size)).ok_or_else(|| {
            format!(
                "{} metadata overflow for type 0x{:016x}, file 0x{:016x}: offset={offset}, size={size}",
                sidecar_label(kind), entry.type_id, entry.file_id
            )
        })?;
        Ok(required.max(end))
    })
}

fn sidecar_interval(entry: &TocOnlyEntry, kind: SidecarKind) -> (u64, u32) {
    match kind {
        SidecarKind::Gpu => (entry.gpu_offset, entry.gpu_size),
        SidecarKind::Stream => (entry.stream_offset, entry.stream_size),
    }
}

fn sidecar_label(kind: SidecarKind) -> &'static str {
    match kind {
        SidecarKind::Gpu => "GPU",
        SidecarKind::Stream => "stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::TocFileType;

    #[test]
    fn ignores_offsets_for_empty_intervals() {
        let toc = fixture(TocOnlyEntry {
            stream_offset: 1_398_144,
            gpu_offset: u64::MAX,
            stream_size: 0,
            gpu_size: 0,
            ..entry()
        });

        assert_eq!(
            patch_sidecar_requirements(&toc).unwrap(),
            SidecarRequirements { gpu: 0, stream: 0 }
        );
    }

    #[test]
    fn uses_the_largest_non_empty_interval_end() {
        let toc = fixture(TocOnlyEntry {
            stream_offset: 96,
            gpu_offset: 64,
            stream_size: 32,
            gpu_size: 16,
            ..entry()
        });

        assert_eq!(
            patch_sidecar_requirements(&toc).unwrap(),
            SidecarRequirements {
                gpu: 80,
                stream: 128,
            }
        );
    }

    #[test]
    fn rejects_non_empty_interval_overflow() {
        let toc = fixture(TocOnlyEntry {
            stream_offset: u64::MAX,
            stream_size: 1,
            ..entry()
        });

        let error = patch_sidecar_requirements(&toc).unwrap_err();

        assert!(error.contains("stream metadata overflow"));
    }

    fn fixture(entry: TocOnlyEntry) -> Vec<u8> {
        TocOnlyPackage {
            types: vec![TocFileType::new(entry.type_id, 1)],
            entries: vec![entry],
            unknown: 0,
            unk4_data: [0; 56],
        }
        .serialize()
        .unwrap()
    }

    fn entry() -> TocOnlyEntry {
        TocOnlyEntry {
            file_id: 42,
            type_id: 99,
            unknown1: 0,
            unknown2: 0,
            unknown3: 16,
            unknown4: 64,
            toc_data: Vec::new(),
            stream_offset: 0,
            gpu_offset: 0,
            stream_size: 0,
            gpu_size: 0,
        }
    }
}
