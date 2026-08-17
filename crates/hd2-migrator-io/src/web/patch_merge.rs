use super::PatchBytes;
use crate::archive::stream_metadata::normalize_patch_stream_metadata;
use crate::archive::{StreamToc, TocEntry};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PatchMergeSourceSummary {
    pub name: String,
    pub resource_count: usize,
    pub repaired_metadata_count: usize,
    pub replaced_resource_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PatchMergeSummary {
    pub input_count: usize,
    pub resource_count: usize,
    pub conflict_count: usize,
    pub duplicate_count: usize,
    pub repaired_metadata_count: usize,
    pub sources: Vec<PatchMergeSourceSummary>,
}

#[derive(Debug, Clone)]
pub struct PatchMergeResult {
    pub patch: PatchBytes,
    pub summary: PatchMergeSummary,
}

/// Merge patches in display order. Later resources replace earlier resources.
pub fn merge_patches(
    inputs: Vec<PatchBytes>,
    output_name: String,
) -> Result<PatchMergeResult, String> {
    if inputs.is_empty() {
        return Err("Choose at least one patch to merge".to_owned());
    }
    validate_output_name(&output_name)?;
    let mut state = MergeState::default();
    for input in inputs {
        state.push(input)?;
    }
    Ok(state.finish(output_name))
}

fn validate_output_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Output patch name cannot be empty".to_owned());
    }
    if trimmed != name || name.contains(['/', '\\']) || matches!(name, "." | "..") {
        return Err(
            "Output patch name must be a filename without surrounding spaces or directories"
                .to_owned(),
        );
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".gpu_resources") || lower.ends_with(".stream") {
        return Err("Output patch name must name the main patch, not a sidecar".to_owned());
    }
    Ok(())
}

struct MergeState {
    entries: BTreeMap<(u64, u64), TocEntry>,
    sources: Vec<PatchMergeSourceSummary>,
    conflict_count: usize,
    duplicate_count: usize,
    repaired_metadata_count: usize,
    unknown: u32,
    unk4_data: [u8; 56],
}

impl Default for MergeState {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            sources: Vec::new(),
            conflict_count: 0,
            duplicate_count: 0,
            repaired_metadata_count: 0,
            unknown: 0,
            unk4_data: [0; 56],
        }
    }
}

impl MergeState {
    fn push(&mut self, mut input: PatchBytes) -> Result<(), String> {
        let repairs = normalize_patch_stream_metadata(&mut input.toc, input.stream.len())
            .map_err(|error| format!("{}: {error}", input.name))?;
        let archive =
            StreamToc::from_buffers(&input.toc, &input.gpu, &input.stream, input.name.clone())
                .map_err(|error| format!("{}: {error}", input.name))?;
        self.unknown = archive.unknown;
        self.unk4_data = archive.unk4_data;
        let replaced_resource_count = self.insert_entries(archive.entries);
        self.repaired_metadata_count += repairs.len();
        self.sources.push(PatchMergeSourceSummary {
            name: input.name,
            resource_count: archive
                .types
                .iter()
                .map(|item| item.num_files as usize)
                .sum(),
            repaired_metadata_count: repairs.len(),
            replaced_resource_count,
        });
        Ok(())
    }

    fn insert_entries(&mut self, entries: Vec<TocEntry>) -> usize {
        let mut replaced = 0;
        for entry in entries {
            let key = (entry.type_id, entry.file_id);
            if let Some(previous) = self.entries.insert(key, entry) {
                replaced += 1;
                if entries_equal(&previous, &self.entries[&key]) {
                    self.duplicate_count += 1;
                } else {
                    self.conflict_count += 1;
                }
            }
        }
        replaced
    }

    fn finish(self, output_name: String) -> PatchMergeResult {
        let resource_count = self.entries.len();
        let mut archive = StreamToc {
            types: Vec::new(),
            entries: self.entries.into_values().collect(),
            unknown: self.unknown,
            unk4_data: self.unk4_data,
            name: output_name.clone(),
        };
        let (toc, gpu, stream) = archive.serialize();
        PatchMergeResult {
            patch: PatchBytes {
                name: output_name,
                toc,
                gpu,
                stream,
            },
            summary: PatchMergeSummary {
                input_count: self.sources.len(),
                resource_count,
                conflict_count: self.conflict_count,
                duplicate_count: self.duplicate_count,
                repaired_metadata_count: self.repaired_metadata_count,
                sources: self.sources,
            },
        }
    }
}

fn entries_equal(left: &TocEntry, right: &TocEntry) -> bool {
    left.file_id == right.file_id
        && left.type_id == right.type_id
        && left.unknown1 == right.unknown1
        && left.unknown2 == right.unknown2
        && left.unknown3 == right.unknown3
        && left.unknown4 == right.unknown4
        && left.toc_data == right.toc_data
        && left.gpu_data == right.gpu_data
        && left.stream_data == right.stream_data
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn later_resource_wins_and_output_reparses() {
        let result = merge_patches(
            vec![fixture("low", 7, b"low"), fixture("high", 7, b"high")],
            "merged.patch".to_owned(),
        )
        .unwrap();
        assert_eq!(result.summary.conflict_count, 1);
        assert_eq!(result.summary.sources[1].replaced_resource_count, 1);
        let parsed = StreamToc::from_buffers(
            &result.patch.toc,
            &result.patch.gpu,
            &result.patch.stream,
            result.patch.name,
        )
        .unwrap();
        assert_eq!(parsed.entries[0].toc_data, b"high");
    }

    #[test]
    fn same_file_id_with_different_type_is_preserved() {
        let result = merge_patches(
            vec![
                fixture_with_type("one", 1, 7),
                fixture_with_type("two", 2, 7),
            ],
            "merged.patch".to_owned(),
        )
        .unwrap();
        assert_eq!(result.summary.resource_count, 2);
        assert_eq!(result.summary.conflict_count, 0);
    }

    #[test]
    fn missing_referenced_sidecar_fails_content_validation() {
        let mut input = fixture("source", 7, b"toc");
        let mut parsed =
            StreamToc::from_buffers(&input.toc, &input.gpu, &input.stream, input.name.clone())
                .unwrap();
        parsed.entries[0].gpu_data = Arc::from([1, 2, 3]);
        let (toc, _, _) = parsed.serialize();
        input.toc = toc;
        input.gpu.clear();
        let error = merge_patches(vec![input], "merged.patch".to_owned()).unwrap_err();
        assert!(error.contains("gpu body OOB"));
    }

    #[test]
    fn rejects_output_paths_and_sidecar_names() {
        let input = fixture("source", 7, b"toc");
        for name in ["../merged.patch", " merged.patch", "merged.patch.stream"] {
            assert!(merge_patches(vec![input.clone()], name.to_owned()).is_err());
        }
    }

    fn fixture(name: &str, file_id: u64, toc_data: &[u8]) -> PatchBytes {
        fixture_with_entry(name, 1, file_id, toc_data)
    }

    fn fixture_with_type(name: &str, type_id: u64, file_id: u64) -> PatchBytes {
        fixture_with_entry(name, type_id, file_id, name.as_bytes())
    }

    fn fixture_with_entry(name: &str, type_id: u64, file_id: u64, toc_data: &[u8]) -> PatchBytes {
        let mut archive = StreamToc::default();
        let mut entry = TocEntry::new(file_id, type_id);
        entry.toc_data = toc_data.to_vec();
        archive.entries.push(entry);
        let (toc, gpu, stream) = archive.serialize();
        PatchBytes {
            name: name.to_owned(),
            toc,
            gpu,
            stream,
        }
    }
}
