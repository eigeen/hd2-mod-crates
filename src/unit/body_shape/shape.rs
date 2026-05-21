use super::{BodyVariantPair, EXPANSION_SAMPLE_COUNT, EXPANSION_THRESHOLD};
use crate::unit::geometry::{
    append_match_level, downsample_points, score_signatures, vector_distance, UnitGeometryRemap,
    UnitGeometrySignature,
};
use crate::unit::names::UnitCustomizationName;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub fn apply_body_variant_pair_tiebreak(
    result: &mut UnitGeometryRemap,
    source_signatures: &HashMap<u64, UnitGeometrySignature>,
    target_signatures: &HashMap<u64, UnitGeometrySignature>,
    source_names: &HashMap<u64, Option<UnitCustomizationName>>,
    target_variants: &HashMap<u64, String>,
    active_source_ids: &BTreeSet<u64>,
) {
    for pair in body_variant_pairs(source_names, active_source_ids) {
        if !source_pair_has_stocky_expansion(source_signatures, pair) {
            continue;
        }
        let Some(targets) = assigned_pair_targets(result, pair) else {
            continue;
        };
        let (stocky_target_id, slim_target_id) = targets;
        if !can_compare_targets(target_variants, stocky_target_id, slim_target_id) {
            continue;
        }
        if !targets_are_near_twins(target_signatures, stocky_target_id, slim_target_id) {
            continue;
        }
        let fatter_target_id = fatter_target_id(target_signatures, targets);
        let fatter = match fatter_target_id {
            Some(id) => id,
            None => continue,
        };
        if fatter == stocky_target_id {
            continue;
        }
        let desired = (fatter, stocky_target_id);
        set_body_pair_targets(
            result,
            source_signatures,
            target_signatures,
            pair,
            desired,
            "body-shape",
        );
    }
}

pub(super) fn body_variant_pairs(
    source_names: &HashMap<u64, Option<UnitCustomizationName>>,
    active_source_ids: &BTreeSet<u64>,
) -> Vec<BodyVariantPair> {
    let mut grouped: BTreeMap<(String, String), BTreeMap<String, u64>> = BTreeMap::new();
    for &sid in active_source_ids {
        if let Some(Some(name)) = source_names.get(&sid) {
            let variant = name.body_variant();
            if variant != "Stocky" && variant != "Slim" {
                continue;
            }
            grouped
                .entry((name.slot.clone(), name.piece_type.clone()))
                .or_default()
                .insert(variant.to_string(), sid);
        }
    }
    grouped
        .into_values()
        .filter_map(|m| {
            let stocky = m.get("Stocky")?;
            let slim = m.get("Slim")?;
            Some(BodyVariantPair {
                stocky_source_id: *stocky,
                slim_source_id: *slim,
            })
        })
        .collect()
}

fn source_pair_has_stocky_expansion(
    source_signatures: &HashMap<u64, UnitGeometrySignature>,
    pair: BodyVariantPair,
) -> bool {
    let slim = &source_signatures[&pair.slim_source_id];
    let stocky = &source_signatures[&pair.stocky_source_id];
    is_directed_expansion(slim, stocky)
}

fn assigned_pair_targets(result: &UnitGeometryRemap, pair: BodyVariantPair) -> Option<(u64, u64)> {
    let stocky = result.expanded_remap.get(&pair.stocky_source_id)?;
    let slim = result.expanded_remap.get(&pair.slim_source_id)?;
    if stocky.len() != 1 || slim.len() != 1 {
        return None;
    }
    Some((stocky[0], slim[0]))
}

fn can_compare_targets(
    target_variants: &HashMap<u64, String>,
    stocky_target_id: u64,
    slim_target_id: u64,
) -> bool {
    let st = target_variants
        .get(&stocky_target_id)
        .map(String::as_str)
        .unwrap_or("Unknown");
    let sl = target_variants
        .get(&slim_target_id)
        .map(String::as_str)
        .unwrap_or("Unknown");
    st == "Unknown" && sl == "Unknown"
}

pub(super) fn targets_are_near_twins(
    target_signatures: &HashMap<u64, UnitGeometrySignature>,
    left_id: u64,
    right_id: u64,
) -> bool {
    let left = &target_signatures[&left_id];
    let right = &target_signatures[&right_id];
    let scale = left.diagonal.max(right.diagonal).max(1e-6);
    let center_distance = distance3(left.center, right.center) / scale;
    let extent_distance = distance3(left.extents, right.extents) / scale;
    center_distance < 0.08 && extent_distance < 0.08
}

pub(super) fn fatter_target_id(
    target_signatures: &HashMap<u64, UnitGeometrySignature>,
    targets: (u64, u64),
) -> Option<u64> {
    let left = &target_signatures[&targets.0];
    let right = &target_signatures[&targets.1];
    if is_directed_expansion(left, right) {
        return Some(targets.1);
    }
    if is_directed_expansion(right, left) {
        return Some(targets.0);
    }
    None
}

fn is_directed_expansion(inner: &UnitGeometrySignature, outer: &UnitGeometrySignature) -> bool {
    let outward = signed_expansion_score(inner, outer);
    let inward = signed_expansion_score(outer, inner);
    outward > EXPANSION_THRESHOLD && inward < -EXPANSION_THRESHOLD
}

fn signed_expansion_score(inner: &UnitGeometrySignature, outer: &UnitGeometrySignature) -> f64 {
    let long_axis = {
        let e = [inner.extents.0, inner.extents.1, inner.extents.2];
        let mut best = 0;
        for i in 1..3 {
            if e[i] > e[best] {
                best = i;
            }
        }
        best
    };
    let inner_points = downsample_points(&inner.points, EXPANSION_SAMPLE_COUNT);
    let outer_points = downsample_points(&outer.points, EXPANSION_SAMPLE_COUNT);
    let mut values: Vec<f64> = Vec::new();
    for p in &inner_points {
        if let Some(v) = point_expansion(*p, &outer_points, inner.center, long_axis) {
            values.push(v);
        }
    }
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn point_expansion(
    point: (f64, f64, f64),
    outer_points: &[(f64, f64, f64)],
    center: (f64, f64, f64),
    long_axis: usize,
) -> Option<f64> {
    let nearest = nearest_point(point, outer_points)?;
    let offset = without_axis(vector_subtract(nearest, point), long_axis);
    let radial = without_axis(vector_subtract(point, center), long_axis);
    let radial_len = vector_length(radial);
    if radial_len < 1e-6 {
        return None;
    }
    let unit_radial = (
        radial.0 / radial_len,
        radial.1 / radial_len,
        radial.2 / radial_len,
    );
    Some(dot(offset, unit_radial))
}

fn nearest_point(point: (f64, f64, f64), points: &[(f64, f64, f64)]) -> Option<(f64, f64, f64)> {
    let mut best: Option<(f64, f64, f64)> = None;
    let mut best_d = f64::INFINITY;
    for &p in points {
        let d = squared_distance(point, p);
        if d < best_d {
            best_d = d;
            best = Some(p);
        }
    }
    best
}

pub(super) fn set_body_pair_targets(
    result: &mut UnitGeometryRemap,
    source_signatures: &HashMap<u64, UnitGeometrySignature>,
    target_signatures: &HashMap<u64, UnitGeometrySignature>,
    pair: BodyVariantPair,
    targets: (u64, u64),
    level: &str,
) {
    result
        .expanded_remap
        .insert(pair.stocky_source_id, vec![targets.0]);
    result
        .expanded_remap
        .insert(pair.slim_source_id, vec![targets.1]);
    result.remap.insert(pair.stocky_source_id, targets.0);
    result.remap.insert(pair.slim_source_id, targets.1);
    refresh_score(
        result,
        source_signatures,
        target_signatures,
        pair.stocky_source_id,
        targets.0,
        level,
    );
    refresh_score(
        result,
        source_signatures,
        target_signatures,
        pair.slim_source_id,
        targets.1,
        level,
    );
}

fn refresh_score(
    result: &mut UnitGeometryRemap,
    source_signatures: &HashMap<u64, UnitGeometrySignature>,
    target_signatures: &HashMap<u64, UnitGeometrySignature>,
    source_id: u64,
    target_id: u64,
    level: &str,
) {
    let score = score_signatures(
        &source_signatures[&source_id],
        &target_signatures[&target_id],
    );
    result.scores.insert(source_id, score);
    let current = result
        .match_levels
        .get(&source_id)
        .cloned()
        .unwrap_or_default();
    result
        .match_levels
        .insert(source_id, append_match_level(&current, level));
}

fn without_axis(v: (f64, f64, f64), axis: usize) -> (f64, f64, f64) {
    match axis {
        0 => (0.0, v.1, v.2),
        1 => (v.0, 0.0, v.2),
        _ => (v.0, v.1, 0.0),
    }
}

fn vector_subtract(left: (f64, f64, f64), right: (f64, f64, f64)) -> (f64, f64, f64) {
    (left.0 - right.0, left.1 - right.1, left.2 - right.2)
}

fn vector_length(v: (f64, f64, f64)) -> f64 {
    (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt()
}

fn dot(left: (f64, f64, f64), right: (f64, f64, f64)) -> f64 {
    left.0 * right.0 + left.1 * right.1 + left.2 * right.2
}

fn squared_distance(left: (f64, f64, f64), right: (f64, f64, f64)) -> f64 {
    let dx = left.0 - right.0;
    let dy = left.1 - right.1;
    let dz = left.2 - right.2;
    dx * dx + dy * dy + dz * dz
}

fn distance3(left: (f64, f64, f64), right: (f64, f64, f64)) -> f64 {
    vector_distance(left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sig(file_id: u64, extents: (f64, f64, f64), diag: f64) -> UnitGeometrySignature {
        UnitGeometrySignature {
            file_id,
            points: Vec::new(),
            sample_points: Vec::new(),
            vertex_count: 1,
            center: (0.0, 0.0, 0.0),
            extents,
            diagonal: diag,
            axis_quantiles: Vec::new(),
            radial_quantiles: Vec::new(),
        }
    }

    #[test]
    fn near_twins_basic() {
        let mut sigs = HashMap::new();
        sigs.insert(1u64, make_sig(1, (1.0, 1.0, 1.0), 1.732));
        sigs.insert(2u64, make_sig(2, (1.0, 1.0, 1.0), 1.732));
        assert!(targets_are_near_twins(&sigs, 1, 2));
    }

    #[test]
    fn near_twins_negative() {
        let mut sigs = HashMap::new();
        sigs.insert(1u64, make_sig(1, (1.0, 1.0, 1.0), 1.732));
        sigs.insert(2u64, make_sig(2, (3.0, 3.0, 3.0), 5.196));
        assert!(!targets_are_near_twins(&sigs, 1, 2));
    }
}
