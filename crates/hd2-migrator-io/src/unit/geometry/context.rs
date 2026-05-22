use super::signature::{
    build_archive_signatures, build_patch_unit_signatures, build_unit_signature,
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
        source_signatures: build_patch_unit_signatures(patch, &patch_unit_ids, settings),
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
}
