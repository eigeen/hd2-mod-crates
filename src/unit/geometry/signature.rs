use super::parsing::parse_unit_points;
use super::scoring::{axis_quantiles, bounding_box_stats, downsample_points, radial_quantiles};
use super::{GeometryMatchSettings, UnitGeometrySignature};
use crate::archive::{StreamToc, TocEntry};
use crate::constants::UNIT_ID;
use std::collections::{BTreeSet, HashMap};

pub fn build_archive_signatures(
    toc: &StreamToc,
    settings: &GeometryMatchSettings,
) -> HashMap<u64, UnitGeometrySignature> {
    let mut out = HashMap::new();
    for entry in toc.entries.iter().filter(|e| e.type_id == UNIT_ID) {
        if let Some(sig) = build_unit_signature(entry, settings) {
            out.insert(entry.file_id, sig);
        }
    }
    out
}

pub fn build_patch_unit_signatures(
    patch: &StreamToc,
    source_unit_ids: &BTreeSet<u64>,
    settings: &GeometryMatchSettings,
) -> HashMap<u64, UnitGeometrySignature> {
    let mut out = HashMap::new();
    for entry in patch.entries.iter().filter(|e| e.type_id == UNIT_ID) {
        if !source_unit_ids.contains(&entry.file_id) {
            continue;
        }
        if let Some(sig) = build_unit_signature(entry, settings) {
            out.insert(entry.file_id, sig);
        }
    }
    out
}

pub fn build_unit_signature(
    entry: &TocEntry,
    settings: &GeometryMatchSettings,
) -> Option<UnitGeometrySignature> {
    let points = parse_unit_points(entry);
    if points.is_empty() {
        return None;
    }
    let sample_points = downsample_points(&points, settings.sample_count);
    let (center, extents, diagonal) = bounding_box_stats(&points);
    let axis_quantiles = axis_quantiles(&points, &settings.quantiles);
    let radial_quantiles = radial_quantiles(&points, center, &settings.quantiles);
    Some(UnitGeometrySignature {
        file_id: entry.file_id,
        vertex_count: points.len(),
        points,
        sample_points,
        center,
        extents,
        diagonal,
        axis_quantiles,
        radial_quantiles,
    })
}
