use super::shape::{
    body_variant_pairs, fatter_target_id, set_body_pair_targets, targets_are_near_twins,
};
use super::{
    BodyPairPreassignmentRequest, BodyVariantPair, DEPTH_EXTENT_THRESHOLD,
    NAMED_UNKNOWN_PAIR_SCORE_LIMIT, PAIR_SCORE_LIMIT,
};
use crate::unit::geometry::{
    vector_distance, UnitGeometryIssue, UnitGeometryRemap, UnitGeometrySignature,
};
use crate::unit::names::UnitCustomizationName;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

type TargetPair = (u64, u64);
type BodyPairCandidate = (TargetPair, f64);
type BodyPairCandidateMap = HashMap<BodyVariantPair, Vec<BodyPairCandidate>>;

pub fn apply_body_variant_pair_preassignment(
    request: BodyPairPreassignmentRequest<'_>,
) -> HashSet<u64> {
    let source_pairs = preassignable_source_pairs(&request);
    let target_pairs = unknown_near_twin_target_pairs(&request);
    let candidates = body_pair_candidates(&request, &source_pairs, &target_pairs);

    // 1. Record source pairs with no viable candidates
    for pair in &source_pairs {
        if candidates.get(pair).map(|v| v.is_empty()).unwrap_or(true) {
            record_unmatched_body_pair(request.result, *pair);
        }
    }

    let solvable_pairs: Vec<BodyVariantPair> = source_pairs
        .iter()
        .copied()
        .filter(|p| candidates.get(p).map(|v| !v.is_empty()).unwrap_or(false))
        .collect();
    let assignments = solve_body_pair_assignment(&solvable_pairs, &candidates);

    for pair in &source_pairs {
        if candidates.get(pair).map(|v| !v.is_empty()).unwrap_or(false)
            && !assignments.contains_key(pair)
        {
            record_unmatched_body_pair(request.result, *pair);
        }
    }

    let mut taken_targets = HashSet::new();
    // need owned snapshot of pairs/targets to avoid double-borrow on request.result
    let assignments_vec: Vec<(BodyVariantPair, (u64, u64))> = assignments.into_iter().collect();
    for (pair, targets) in assignments_vec {
        let ordered = orient_body_pair_targets(&request, pair, targets);
        match ordered {
            Some(ordered_targets) => {
                set_body_pair_targets(
                    request.result,
                    request.source_signatures,
                    request.target_signatures,
                    pair,
                    ordered_targets,
                    "body-pair",
                );
                request
                    .result
                    .claimed_target_file_ids
                    .insert(ordered_targets.0);
                request
                    .result
                    .claimed_target_file_ids
                    .insert(ordered_targets.1);
                taken_targets.insert(ordered_targets.0);
                taken_targets.insert(ordered_targets.1);
            }
            None => record_ambiguous_body_pair(request.result, pair, targets),
        }
    }
    taken_targets
}

// ---------- preassignment helpers ---------------------------------------

fn preassignable_source_pairs(req: &BodyPairPreassignmentRequest) -> Vec<BodyVariantPair> {
    let mut out = Vec::new();
    for pair in body_variant_pairs(req.source_names, req.active_source_ids) {
        if !req.source_signatures.contains_key(&pair.stocky_source_id) {
            continue;
        }
        if !req.source_signatures.contains_key(&pair.slim_source_id) {
            continue;
        }
        if has_complete_variant_part_targets(req, pair) {
            continue;
        }
        out.push(pair);
    }
    out
}

fn has_complete_variant_part_targets(
    req: &BodyPairPreassignmentRequest,
    pair: BodyVariantPair,
) -> bool {
    let stocky_name = req
        .source_names
        .get(&pair.stocky_source_id)
        .and_then(|n| n.as_ref());
    let slim_name = req
        .source_names
        .get(&pair.slim_source_id)
        .and_then(|n| n.as_ref());
    has_variant_part_target(req, stocky_name, "Stocky")
        && has_variant_part_target(req, slim_name, "Slim")
}

fn has_variant_part_target(
    req: &BodyPairPreassignmentRequest,
    source_name: Option<&UnitCustomizationName>,
    variant: &str,
) -> bool {
    let Some(sname) = source_name else {
        return false;
    };
    req.target_names.values().any(|tname| {
        if let Some(tname) = tname {
            tname.body_variant() == variant && sname.slot == tname.slot
        } else {
            false
        }
    })
}

fn same_part(
    source: Option<&UnitCustomizationName>,
    target: Option<&UnitCustomizationName>,
) -> bool {
    match (source, target) {
        (Some(s), Some(t)) => s.slot == t.slot && s.piece_type == t.piece_type,
        _ => false,
    }
}

fn unknown_near_twin_target_pairs(req: &BodyPairPreassignmentRequest) -> Vec<(u64, u64)> {
    let target_ids: Vec<u64> = req
        .target_signatures
        .keys()
        .copied()
        .filter(|id| !req.result.claimed_target_file_ids.contains(id))
        .filter(|id| {
            req.target_variants
                .get(id)
                .map(String::as_str)
                .unwrap_or("Unknown")
                == "Unknown"
        })
        .collect();
    let mut pairs = Vec::new();
    for (i, &left) in target_ids.iter().enumerate() {
        for &right in target_ids.iter().skip(i + 1) {
            if targets_are_near_twins(req.target_signatures, left, right) {
                pairs.push((left, right));
            }
        }
    }
    pairs
}

fn body_pair_candidates(
    req: &BodyPairPreassignmentRequest,
    source_pairs: &[BodyVariantPair],
    target_pairs: &[TargetPair],
) -> BodyPairCandidateMap {
    let mut out = HashMap::new();
    for &pair in source_pairs {
        let mut scoped = target_pairs.to_vec();
        scoped.extend(named_unknown_near_twin_target_pairs(req, pair));
        let deduped = dedupe_target_pairs(&scoped);
        let mut ranked: Vec<BodyPairCandidate> = deduped
            .into_iter()
            .map(|t| (t, body_pair_score(req, pair, t)))
            .collect();
        ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let filtered: Vec<BodyPairCandidate> = ranked
            .into_iter()
            .filter(|item| body_pair_candidate_allowed(req, pair, item.0, item.1))
            .collect();
        out.insert(pair, filtered);
    }
    out
}

fn body_pair_candidate_allowed(
    req: &BodyPairPreassignmentRequest,
    pair: BodyVariantPair,
    targets: (u64, u64),
    score: f64,
) -> bool {
    if score <= PAIR_SCORE_LIMIT {
        return true;
    }
    if !has_named_part_target_in_pair(req, pair, targets) {
        return false;
    }
    score <= NAMED_UNKNOWN_PAIR_SCORE_LIMIT
}

fn has_named_part_target_in_pair(
    req: &BodyPairPreassignmentRequest,
    pair: BodyVariantPair,
    targets: (u64, u64),
) -> bool {
    let stocky_name = req
        .source_names
        .get(&pair.stocky_source_id)
        .and_then(|n| n.as_ref());
    let slim_name = req
        .source_names
        .get(&pair.slim_source_id)
        .and_then(|n| n.as_ref());
    [targets.0, targets.1].iter().any(|tid| {
        let target_name = req.target_names.get(tid).and_then(|n| n.as_ref());
        named_target_matches_pair(stocky_name, slim_name, target_name)
    })
}

fn named_unknown_near_twin_target_pairs(
    req: &BodyPairPreassignmentRequest,
    pair: BodyVariantPair,
) -> Vec<(u64, u64)> {
    let unknown_ids = unknown_target_ids(req);
    let named_ids = named_part_target_ids(req, pair);
    let mut out = Vec::new();
    for &named in &named_ids {
        for &unknown in &unknown_ids {
            if targets_are_near_twins(req.target_signatures, named, unknown) {
                out.push((named, unknown));
            }
        }
    }
    out
}

fn unknown_target_ids(req: &BodyPairPreassignmentRequest) -> Vec<u64> {
    req.target_signatures
        .keys()
        .copied()
        .filter(|id| !req.result.claimed_target_file_ids.contains(id))
        .filter(|id| {
            req.target_variants
                .get(id)
                .map(String::as_str)
                .unwrap_or("Unknown")
                == "Unknown"
        })
        .collect()
}

fn named_part_target_ids(req: &BodyPairPreassignmentRequest, pair: BodyVariantPair) -> Vec<u64> {
    let stocky_name = req
        .source_names
        .get(&pair.stocky_source_id)
        .and_then(|n| n.as_ref());
    let slim_name = req
        .source_names
        .get(&pair.slim_source_id)
        .and_then(|n| n.as_ref());
    req.target_names
        .iter()
        .filter(|(id, _)| !req.result.claimed_target_file_ids.contains(id))
        .filter(|(_, name)| named_target_matches_pair(stocky_name, slim_name, name.as_ref()))
        .map(|(id, _)| *id)
        .collect()
}

fn named_target_matches_pair(
    stocky_name: Option<&UnitCustomizationName>,
    slim_name: Option<&UnitCustomizationName>,
    target_name: Option<&UnitCustomizationName>,
) -> bool {
    let Some(t) = target_name else {
        return false;
    };
    let variant = t.body_variant();
    if variant != "Stocky" && variant != "Slim" {
        return false;
    }
    same_part(stocky_name, Some(t)) || same_part(slim_name, Some(t))
}

fn dedupe_target_pairs(pairs: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut seen: HashSet<BTreeSet<u64>> = HashSet::new();
    let mut out = Vec::new();
    for &pair in pairs {
        let key: BTreeSet<u64> = [pair.0, pair.1].iter().copied().collect();
        if seen.insert(key) {
            out.push(pair);
        }
    }
    out
}

fn body_pair_score(
    req: &BodyPairPreassignmentRequest,
    pair: BodyVariantPair,
    targets: (u64, u64),
) -> f64 {
    let stocky = &req.source_signatures[&pair.stocky_source_id];
    let slim = &req.source_signatures[&pair.slim_source_id];
    let left = &req.target_signatures[&targets.0];
    let right = &req.target_signatures[&targets.1];
    let direct = shape_score(stocky, left) + shape_score(slim, right);
    let swapped = shape_score(stocky, right) + shape_score(slim, left);
    direct.min(swapped)
}

fn shape_score(source: &UnitGeometrySignature, target: &UnitGeometrySignature) -> f64 {
    let scale = source.diagonal.max(target.diagonal).max(1e-6);
    let extent_score = vector_distance(sorted_extents(source), sorted_extents(target)) / scale;
    let diagonal_score = ((source.diagonal + 1e-6) / (target.diagonal + 1e-6))
        .ln()
        .abs();
    let count_score = ((source.vertex_count as f64 + 1.0) / (target.vertex_count as f64 + 1.0))
        .ln()
        .abs();
    extent_score + 0.25 * diagonal_score + 0.05 * count_score
}

fn sorted_extents(signature: &UnitGeometrySignature) -> (f64, f64, f64) {
    let mut e = [
        signature.extents.0,
        signature.extents.1,
        signature.extents.2,
    ];
    e.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    (e[0], e[1], e[2])
}

fn solve_body_pair_assignment(
    source_pairs: &[BodyVariantPair],
    candidates: &BodyPairCandidateMap,
) -> BTreeMap<BodyVariantPair, TargetPair> {
    let mut sorted_pairs: Vec<BodyVariantPair> = source_pairs.to_vec();
    sorted_pairs.sort_by(|a, b| {
        let sa = best_pair_score(candidates, *a);
        let sb = best_pair_score(candidates, *b);
        sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut assigned: BTreeMap<BodyVariantPair, (u64, u64)> = BTreeMap::new();
    let mut used: HashSet<u64> = HashSet::new();
    for pair in sorted_pairs {
        if let Some(values) = candidates.get(&pair) {
            for (targets, _) in values {
                if used.contains(&targets.0) || used.contains(&targets.1) {
                    continue;
                }
                assigned.insert(pair, *targets);
                used.insert(targets.0);
                used.insert(targets.1);
                break;
            }
        }
    }
    assigned
}

fn best_pair_score(candidates: &BodyPairCandidateMap, pair: BodyVariantPair) -> f64 {
    candidates
        .get(&pair)
        .and_then(|v| v.first())
        .map(|(_, score)| *score)
        .unwrap_or(f64::INFINITY)
}

fn orient_body_pair_targets(
    req: &BodyPairPreassignmentRequest,
    _pair: BodyVariantPair,
    targets: (u64, u64),
) -> Option<(u64, u64)> {
    if let Some(named) = named_body_pair_orientation(req, targets) {
        return Some(named);
    }
    if let Some(fatter) = fatter_target_id(req.target_signatures, targets) {
        return Some((fatter, other_target(targets, fatter)));
    }
    if let Some(stockier) = stockier_depth_target_id(req.target_signatures, targets) {
        return Some((stockier, other_target(targets, stockier)));
    }
    None
}

fn named_body_pair_orientation(
    req: &BodyPairPreassignmentRequest,
    targets: (u64, u64),
) -> Option<(u64, u64)> {
    if let Some(stocky_id) = target_id_with_variant(req, targets, "Stocky") {
        return Some((stocky_id, other_target(targets, stocky_id)));
    }
    if let Some(slim_id) = target_id_with_variant(req, targets, "Slim") {
        return Some((other_target(targets, slim_id), slim_id));
    }
    None
}

fn target_id_with_variant(
    req: &BodyPairPreassignmentRequest,
    targets: (u64, u64),
    variant: &str,
) -> Option<u64> {
    for tid in [targets.0, targets.1] {
        if let Some(Some(name)) = req.target_names.get(&tid) {
            if name.body_variant() == variant {
                return Some(tid);
            }
        }
    }
    None
}

fn stockier_depth_target_id(
    target_signatures: &HashMap<u64, UnitGeometrySignature>,
    targets: (u64, u64),
) -> Option<u64> {
    let left = &target_signatures[&targets.0];
    let right = &target_signatures[&targets.1];
    let left_depth = left.extents.2;
    let right_depth = right.extents.2;
    if (left_depth - right_depth).abs() < DEPTH_EXTENT_THRESHOLD {
        return None;
    }
    Some(if left_depth > right_depth {
        targets.0
    } else {
        targets.1
    })
}

fn other_target(targets: (u64, u64), target_id: u64) -> u64 {
    if targets.0 == target_id {
        targets.1
    } else {
        targets.0
    }
}

fn record_ambiguous_body_pair(
    result: &mut UnitGeometryRemap,
    pair: BodyVariantPair,
    targets: (u64, u64),
) {
    let reason = "ambiguous Stocky/Slim body-pair target orientation".to_string();
    for sid in [pair.stocky_source_id, pair.slim_source_id] {
        result.ambiguous.push(UnitGeometryIssue {
            source_file_id: sid,
            reason: reason.clone(),
            candidates: vec![targets.0, targets.1],
        });
    }
}

fn record_unmatched_body_pair(result: &mut UnitGeometryRemap, pair: BodyVariantPair) {
    let reason = "no safe Stocky/Slim body-pair target candidates".to_string();
    for sid in [pair.stocky_source_id, pair.slim_source_id] {
        result.ambiguous.push(UnitGeometryIssue {
            source_file_id: sid,
            reason: reason.clone(),
            candidates: Vec::new(),
        });
    }
}
