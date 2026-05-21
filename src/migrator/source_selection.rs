use crate::archive::StreamToc;
use crate::constants::{MATERIAL_ID, TEX_ID, UNIT_ID};
use crate::refs;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceArchiveCandidate {
    pub hash: String,
    pub name: String,
    pub unit_hits: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct PatchSourceFilter {
    pub patch: StreamToc,
    pub kept_units: usize,
    pub dropped_units: usize,
    pub dropped_entries: usize,
}

pub(crate) fn detect_source_archive(
    patch: &StreamToc,
    data_dir: &Path,
    archives: &[(String, String)],
    bundle_index: Option<&crate::archive::BundleIndex>,
) -> Option<SourceArchiveCandidate> {
    let patch_units = unit_file_ids(patch);
    if patch_units.is_empty() {
        return None;
    }
    let mut candidates = source_archive_candidates(&patch_units, data_dir, archives, bundle_index);
    candidates.sort_by(compare_candidates);
    log_source_candidates(&candidates);
    candidates.pop()
}

pub(crate) fn filter_patch_to_source_archive_units(
    patch: &StreamToc,
    source: &StreamToc,
) -> PatchSourceFilter {
    let source_unit_ids = unit_file_ids(source);
    let kept_unit_ids = kept_patch_unit_ids(patch, &source_unit_ids);
    let material_ids = referenced_patch_material_ids(patch, &kept_unit_ids);
    let texture_ids = referenced_patch_texture_ids(patch, &material_ids);
    let filtered_entries = patch
        .entries
        .iter()
        .filter(|entry| {
            keep_entry(
                entry.type_id,
                entry.file_id,
                &kept_unit_ids,
                &material_ids,
                &texture_ids,
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let dropped_units = patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID && !kept_unit_ids.contains(&entry.file_id))
        .count();
    PatchSourceFilter {
        patch: StreamToc {
            entries: filtered_entries.clone(),
            ..patch.clone()
        },
        kept_units: kept_unit_ids.len(),
        dropped_units,
        dropped_entries: patch.entries.len().saturating_sub(filtered_entries.len()),
    }
}

fn source_archive_candidates(
    patch_units: &HashSet<u64>,
    data_dir: &Path,
    archives: &[(String, String)],
    bundle_index: Option<&crate::archive::BundleIndex>,
) -> Vec<SourceArchiveCandidate> {
    archives
        .iter()
        .filter_map(|(hash, name)| {
            source_archive_candidate(patch_units, data_dir, hash, name, bundle_index)
        })
        .collect()
}

fn source_archive_candidate(
    patch_units: &HashSet<u64>,
    data_dir: &Path,
    hash: &str,
    name: &str,
    bundle_index: Option<&crate::archive::BundleIndex>,
) -> Option<SourceArchiveCandidate> {
    let path = data_dir.join(hash);
    if !source_archive_exists(&path, bundle_index) {
        return None;
    }
    let ids = crate::archive::list_file_ids_with_bundle(&path, bundle_index).ok()?;
    let source_units = ids
        .get(&UNIT_ID)
        .map(|file_ids| file_ids.iter().copied().collect::<HashSet<_>>())
        .unwrap_or_default();
    let unit_hits = patch_units.intersection(&source_units).count();
    (unit_hits > 0).then(|| SourceArchiveCandidate {
        hash: hash.to_string(),
        name: name.to_string(),
        unit_hits,
    })
}

fn source_archive_exists(path: &Path, bundle_index: Option<&crate::archive::BundleIndex>) -> bool {
    path.exists() || bundle_index.is_some_and(|b| b.has_package(path.to_str().unwrap_or("")))
}

fn compare_candidates(
    left: &SourceArchiveCandidate,
    right: &SourceArchiveCandidate,
) -> std::cmp::Ordering {
    left.unit_hits
        .cmp(&right.unit_hits)
        .then_with(|| right.hash.cmp(&left.hash))
}

fn log_source_candidates(candidates: &[SourceArchiveCandidate]) {
    if candidates.len() <= 1 {
        return;
    }
    let top = candidates
        .iter()
        .rev()
        .take(5)
        .map(candidate_label)
        .collect::<Vec<_>>();
    tracing::info!(
        candidates = %top.join(", "),
        "source archive candidates by Unit overlap"
    );
}

fn candidate_label(candidate: &SourceArchiveCandidate) -> String {
    format!(
        "{} [{}]: {} Unit hits",
        candidate.name, candidate.hash, candidate.unit_hits
    )
}

fn unit_file_ids(toc: &StreamToc) -> HashSet<u64> {
    toc.entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .map(|entry| entry.file_id)
        .collect()
}

fn kept_patch_unit_ids(patch: &StreamToc, source_unit_ids: &HashSet<u64>) -> HashSet<u64> {
    patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .map(|entry| entry.file_id)
        .filter(|file_id| source_unit_ids.contains(file_id))
        .collect()
}

fn referenced_patch_material_ids(patch: &StreamToc, kept_unit_ids: &HashSet<u64>) -> HashSet<u64> {
    let patch_material_ids = type_file_ids(patch, MATERIAL_ID);
    patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID && kept_unit_ids.contains(&entry.file_id))
        .flat_map(|entry| refs::list_unit_refs(&entry.toc_data))
        .filter(|file_id| patch_material_ids.contains(file_id))
        .collect()
}

fn referenced_patch_texture_ids(patch: &StreamToc, material_ids: &HashSet<u64>) -> HashSet<u64> {
    let patch_texture_ids = type_file_ids(patch, TEX_ID);
    patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == MATERIAL_ID && material_ids.contains(&entry.file_id))
        .flat_map(|entry| refs::list_material_refs(&entry.toc_data))
        .filter(|file_id| patch_texture_ids.contains(file_id))
        .collect()
}

fn type_file_ids(patch: &StreamToc, type_id: u64) -> HashSet<u64> {
    patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == type_id)
        .map(|entry| entry.file_id)
        .collect()
}

fn keep_entry(
    type_id: u64,
    file_id: u64,
    unit_ids: &HashSet<u64>,
    material_ids: &HashSet<u64>,
    texture_ids: &HashSet<u64>,
) -> bool {
    match type_id {
        UNIT_ID => unit_ids.contains(&file_id),
        MATERIAL_ID => material_ids.contains(&file_id),
        TEX_ID => texture_ids.contains(&file_id),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::TocEntry;
    use byteorder::{ByteOrder, LittleEndian as LE};

    const OTHER_ID: u64 = 42;

    #[test]
    fn detect_source_archive_prefers_most_unit_hits() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_archive(temp.path(), "source-a", &[1, 2]);
        write_archive(temp.path(), "source-b", &[1, 2, 3]);
        let patch = StreamToc {
            entries: vec![unit_entry(1, &[]), unit_entry(2, &[]), unit_entry(3, &[])],
            ..Default::default()
        };
        let archives = vec![
            ("source-a".to_string(), "Source A".to_string()),
            ("source-b".to_string(), "Source B".to_string()),
        ];

        let detected = detect_source_archive(&patch, temp.path(), &archives, None).unwrap();

        assert_eq!(detected.hash, "source-b");
        assert_eq!(detected.name, "Source B");
        assert_eq!(detected.unit_hits, 3);
    }

    #[test]
    fn filter_keeps_only_units_present_in_source_archive() {
        let patch = StreamToc {
            entries: vec![
                unit_entry(1, &[10]),
                unit_entry(2, &[20]),
                unit_entry(3, &[30]),
            ],
            ..Default::default()
        };
        let source = StreamToc {
            entries: vec![TocEntry::new(1, UNIT_ID), TocEntry::new(3, UNIT_ID)],
            ..Default::default()
        };

        let filtered = filter_patch_to_source_archive_units(&patch, &source);

        assert_eq!(
            entry_keys(&filtered.patch),
            vec![(1, UNIT_ID), (3, UNIT_ID)]
        );
        assert_eq!(filtered.kept_units, 2);
        assert_eq!(filtered.dropped_units, 1);
    }

    #[test]
    fn filter_keeps_material_and_texture_reference_closure() {
        let patch = StreamToc {
            entries: vec![
                unit_entry(1, &[10]),
                unit_entry(2, &[20]),
                material_entry(10, &[100]),
                material_entry(20, &[200]),
                texture_entry(100),
                texture_entry(200),
                TocEntry::new(7, OTHER_ID),
            ],
            ..Default::default()
        };
        let source = StreamToc {
            entries: vec![TocEntry::new(1, UNIT_ID)],
            ..Default::default()
        };

        let filtered = filter_patch_to_source_archive_units(&patch, &source);

        assert_eq!(
            entry_keys(&filtered.patch),
            vec![
                (1, UNIT_ID),
                (10, MATERIAL_ID),
                (100, TEX_ID),
                (7, OTHER_ID)
            ]
        );
        assert_eq!(filtered.dropped_units, 1);
    }

    fn entry_keys(patch: &StreamToc) -> Vec<(u64, u64)> {
        patch
            .entries
            .iter()
            .map(|entry| (entry.file_id, entry.type_id))
            .collect()
    }

    fn unit_entry(file_id: u64, material_ids: &[u64]) -> TocEntry {
        let mut entry = TocEntry::new(file_id, UNIT_ID);
        entry.toc_data = unit_blob(material_ids);
        entry
    }

    fn material_entry(file_id: u64, texture_ids: &[u64]) -> TocEntry {
        let mut entry = TocEntry::new(file_id, MATERIAL_ID);
        entry.toc_data = material_blob(texture_ids);
        entry
    }

    fn texture_entry(file_id: u64) -> TocEntry {
        TocEntry::new(file_id, TEX_ID)
    }

    fn write_archive(dir: &Path, name: &str, unit_ids: &[u64]) {
        let mut toc = StreamToc {
            entries: unit_ids
                .iter()
                .map(|file_id| TocEntry::new(*file_id, UNIT_ID))
                .collect(),
            ..Default::default()
        };
        toc.write_files(&dir.join(name)).expect("write archive");
    }

    fn unit_blob(material_ids: &[u64]) -> Vec<u8> {
        let materials_offset = 0x80;
        let total = materials_offset + 4 + 4 * material_ids.len() + 8 * material_ids.len();
        let mut blob = vec![0u8; total];
        LE::write_u32(&mut blob[0x70..0x74], materials_offset as u32);
        LE::write_u32(
            &mut blob[materials_offset..materials_offset + 4],
            material_ids.len() as u32,
        );
        let base = materials_offset + 4 + 4 * material_ids.len();
        for (index, file_id) in material_ids.iter().enumerate() {
            LE::write_u64(&mut blob[base + 8 * index..base + 8 * index + 8], *file_id);
        }
        blob
    }

    fn material_blob(texture_ids: &[u64]) -> Vec<u8> {
        let base = 136;
        let mut blob = vec![0u8; base + 4 * texture_ids.len() + 8 * texture_ids.len()];
        LE::write_u32(&mut blob[0x40..0x44], texture_ids.len() as u32);
        let tex_base = base + 4 * texture_ids.len();
        for (index, file_id) in texture_ids.iter().enumerate() {
            LE::write_u64(
                &mut blob[tex_base + 8 * index..tex_base + 8 * index + 8],
                *file_id,
            );
        }
        blob
    }
}
