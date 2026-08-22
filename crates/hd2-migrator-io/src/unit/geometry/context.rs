use super::signature::{
    build_archive_signatures, build_selected_unit_signatures, build_unit_signature,
};
use super::{GeometryMatchSettings, UnitGeometrySignature};
use crate::archive::StreamToc;
use crate::constants::UNIT_ID;
use crate::unit::names::{extract_customization_name, UnitCustomizationName};
use std::collections::{BTreeSet, HashMap, HashSet};

const EMPTY_PLACEHOLDER_MAX_VERTICES: usize = 100;
const EMPTY_PLACEHOLDER_MAX_DIAGONAL: f64 = 1e-4;

#[derive(Debug)]
pub(super) struct UnitMatchContext {
    pub(super) patch_unit_ids: BTreeSet<u64>,
    pub(super) source_signatures: HashMap<u64, UnitGeometrySignature>,
    pub(super) target_signatures: HashMap<u64, UnitGeometrySignature>,
    pub(super) source_names: HashMap<u64, Option<UnitCustomizationName>>,
    pub(super) target_names: HashMap<u64, Option<UnitCustomizationName>>,
    pub(super) source_variants: HashMap<u64, String>,
    pub(super) target_variants: HashMap<u64, String>,
}

// ---------- context construction ----------------------------------------

fn patch_source_unit_ids(patch: &StreamToc, source: &StreamToc) -> BTreeSet<u64> {
    let source_ids: BTreeSet<u64> = source
        .entries
        .iter()
        .filter(|e| e.type_id == UNIT_ID)
        .map(|e| e.file_id)
        .collect();
    patch
        .entries
        .iter()
        .filter(|e| e.type_id == UNIT_ID && source_ids.contains(&e.file_id))
        .map(|e| e.file_id)
        .collect()
}

pub(super) fn build_match_context(
    patch: &StreamToc,
    source: &StreamToc,
    target: &StreamToc,
    settings: &GeometryMatchSettings,
) -> UnitMatchContext {
    let patch_unit_ids = patch_source_unit_ids(patch, source);
    UnitMatchContext {
        source_signatures: build_selected_unit_signatures(source, &patch_unit_ids, settings),
        target_signatures: build_archive_signatures(target, settings),
        source_names: source_customization_names(patch, source),
        target_names: archive_customization_names(target),
        source_variants: source_body_variants(patch, source),
        target_variants: archive_body_variants(target),
        patch_unit_ids,
    }
}

fn archive_body_variants(toc: &StreamToc) -> HashMap<u64, String> {
    toc.entries
        .iter()
        .filter(|e| e.type_id == UNIT_ID)
        .map(|e| {
            (
                e.file_id,
                crate::unit::names::body_variant(&e.toc_data).to_string(),
            )
        })
        .collect()
}

fn source_body_variants(patch: &StreamToc, source: &StreamToc) -> HashMap<u64, String> {
    source_customization_names(patch, source)
        .into_iter()
        .map(|(id, name)| {
            let v = name
                .as_ref()
                .map(UnitCustomizationName::body_variant)
                .unwrap_or("Unknown")
                .to_string();
            (id, v)
        })
        .collect()
}

fn source_customization_names(
    patch: &StreamToc,
    source: &StreamToc,
) -> HashMap<u64, Option<UnitCustomizationName>> {
    let mut names = archive_customization_names(source);
    for entry in patch.entries.iter().filter(|e| e.type_id == UNIT_ID) {
        let new_name = extract_customization_name(&entry.toc_data);
        if let Some(slot) = names.get_mut(&entry.file_id)
            && new_name.is_some()
        {
            *slot = new_name;
        }
    }
    names
}

fn archive_customization_names(toc: &StreamToc) -> HashMap<u64, Option<UnitCustomizationName>> {
    toc.entries
        .iter()
        .filter(|e| e.type_id == UNIT_ID)
        .map(|e| (e.file_id, extract_customization_name(&e.toc_data)))
        .collect()
}

pub(super) fn empty_patch_source_unit_ids(
    patch: &StreamToc,
    patch_unit_ids: &BTreeSet<u64>,
    settings: &GeometryMatchSettings,
) -> HashSet<u64> {
    let mut empty_ids = HashSet::new();
    for entry in patch.entries.iter().filter(|e| e.type_id == UNIT_ID) {
        if !patch_unit_ids.contains(&entry.file_id) {
            continue;
        }
        if let Some(sig) = build_unit_signature(entry, settings)
            && is_empty_signature(&sig)
        {
            empty_ids.insert(entry.file_id);
        }
    }
    empty_ids
}

fn is_empty_signature(sig: &UnitGeometrySignature) -> bool {
    sig.vertex_count <= EMPTY_PLACEHOLDER_MAX_VERTICES
        || sig.diagonal < EMPTY_PLACEHOLDER_MAX_DIAGONAL
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT_FILE_ID: u64 = 0x1234;
    const STREAM_TABLE_OFFSET: usize = 0x100;
    const STREAM_OFFSET: usize = 0x120;
    const MESH_TABLE_OFFSET: usize = 0x400;
    const MESH_OFFSET: usize = 0x420;
    const SECTION_OFFSET: usize = 0x520;

    #[test]
    fn uses_original_source_geometry_instead_of_mod_geometry() {
        let patch = archive_with_unit(&[(100.0, 0.0, 0.0), (110.0, 0.0, 0.0)]);
        let source = archive_with_unit(&[(1.0, 0.0, 0.0), (2.0, 0.0, 0.0)]);
        let context = build_match_context(
            &patch,
            &source,
            &StreamToc::default(),
            &GeometryMatchSettings::default(),
        );

        assert_eq!(
            context.source_signatures[&UNIT_FILE_ID].points,
            vec![(1.0, 0.0, 0.0), (2.0, 0.0, 0.0)]
        );
    }

    #[test]
    fn does_not_fall_back_to_mod_geometry_when_source_geometry_is_missing() {
        let patch = archive_with_unit(&[(100.0, 0.0, 0.0), (110.0, 0.0, 0.0)]);
        let source = archive_with_empty_unit();
        let context = build_match_context(
            &patch,
            &source,
            &StreamToc::default(),
            &GeometryMatchSettings::default(),
        );

        assert!(!context.source_signatures.contains_key(&UNIT_FILE_ID));
    }

    #[test]
    fn treats_low_vertex_units_as_empty_placeholders() {
        assert!(is_empty_signature(&signature(100, 0.5)));
        assert!(!is_empty_signature(&signature(101, 0.5)));
    }

    #[test]
    fn treats_tiny_units_as_empty_placeholders() {
        assert!(is_empty_signature(&signature(1_000, 0.000_099)));
        assert!(!is_empty_signature(&signature(1_000, 0.000_1)));
    }

    fn signature(vertex_count: usize, diagonal: f64) -> UnitGeometrySignature {
        UnitGeometrySignature {
            file_id: 1,
            points: Vec::new(),
            sample_points: Vec::new(),
            vertex_count,
            center: (0.0, 0.0, 0.0),
            extents: (diagonal, 0.0, 0.0),
            diagonal,
            axis_quantiles: Vec::new(),
            radial_quantiles: Vec::new(),
        }
    }

    fn archive_with_unit(points: &[(f32, f32, f32)]) -> StreamToc {
        let mut archive = StreamToc::default();
        archive.entries.push(unit_entry(points));
        archive
    }

    fn archive_with_empty_unit() -> StreamToc {
        let mut archive = StreamToc::default();
        archive
            .entries
            .push(crate::archive::TocEntry::new(UNIT_FILE_ID, UNIT_ID));
        archive
    }

    fn unit_entry(points: &[(f32, f32, f32)]) -> crate::archive::TocEntry {
        let mut entry = crate::archive::TocEntry::new(UNIT_FILE_ID, UNIT_ID);
        entry.toc_data = unit_toc_data(points.len());
        entry.gpu_data = point_bytes(points).into();
        entry
    }

    fn unit_toc_data(vertex_count: usize) -> Vec<u8> {
        let mut data = vec![0; 0x600];
        configure_stream_layout(&mut data, vertex_count);
        configure_mesh_layout(&mut data, vertex_count);
        data
    }

    fn configure_stream_layout(data: &mut [u8], vertex_count: usize) {
        write_u32(data, 0x5c, STREAM_TABLE_OFFSET as u32);
        write_u32(data, STREAM_TABLE_OFFSET, 1);
        write_u32(data, STREAM_TABLE_OFFSET + 4, 0x20);
        write_u64(data, STREAM_OFFSET + 328, 1);
        write_u32(data, STREAM_OFFSET + 12, 2);
        write_u32(data, STREAM_OFFSET + 352, vertex_count as u32);
        write_u32(data, STREAM_OFFSET + 356, 12);
    }

    fn configure_mesh_layout(data: &mut [u8], vertex_count: usize) {
        write_u32(data, 0x64, MESH_TABLE_OFFSET as u32);
        write_u32(data, MESH_TABLE_OFFSET, 1);
        write_u32(data, MESH_TABLE_OFFSET + 4, 0x20);
        write_u32(data, MESH_OFFSET + 120, 1);
        write_u32(
            data,
            MESH_OFFSET + 124,
            (SECTION_OFFSET - MESH_OFFSET) as u32,
        );
        write_u32(data, SECTION_OFFSET + 8, vertex_count as u32);
    }

    fn point_bytes(points: &[(f32, f32, f32)]) -> Vec<u8> {
        points
            .iter()
            .flat_map(|point| [point.0, point.1, point.2])
            .flat_map(f32::to_le_bytes)
            .collect()
    }

    fn write_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u64(data: &mut [u8], offset: usize, value: u64) {
        data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
}
