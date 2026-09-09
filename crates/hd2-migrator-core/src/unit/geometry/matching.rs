use super::context::UnitMatchContext;
use super::scoring::score_signatures;
use super::{GeometryMatchSettings, UnitGeometryIssue, UnitGeometryRemap, UnitGeometrySignature};
use crate::archive::StreamToc;
use crate::constants::UNIT_ID;
use crate::unit::body_shape::{
    apply_body_variant_pair_preassignment, BodyPairPreassignmentRequest,
};
use crate::unit::names::UnitCustomizationName;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const NAMED_SLOT_SCORE_LIMIT: f64 = 2.0;

// ---------- match assignment --------------------------------------------

pub(super) fn record_missing_patch_units(
    result: &mut UnitGeometryRemap,
    context: &UnitMatchContext,
    active_patch_unit_ids: &BTreeSet<u64>,
) {
    let (missing_ids, reason): (BTreeSet<u64>, &str) = if !context.target_signatures.is_empty() {
        let src_ids: BTreeSet<u64> = context.source_signatures.keys().copied().collect();
        let missing: BTreeSet<u64> = active_patch_unit_ids
            .difference(&src_ids)
            .copied()
            .collect();
        (missing, "patch Unit has no parseable mod geometry")
    } else {
        (
            active_patch_unit_ids.clone(),
            "target archive has no parseable direct Unit geometry",
        )
    };
    for file_id in missing_ids {
        result.missing.push(UnitGeometryIssue {
            source_file_id: file_id,
            reason: reason.to_string(),
            candidates: Vec::new(),
        });
    }
}

pub(super) fn assign_geometry_matches(
    result: &mut UnitGeometryRemap,
    context: &UnitMatchContext,
    active_patch_unit_ids: &BTreeSet<u64>,
    settings: &GeometryMatchSettings,
) {
    let mut taken_targets = apply_body_variant_pair_preassignment(BodyPairPreassignmentRequest {
        result,
        source_signatures: &context.source_signatures,
        target_signatures: &context.target_signatures,
        source_names: &context.source_names,
        target_names: &context.target_names,
        target_variants: &context.target_variants,
        active_source_ids: active_patch_unit_ids,
    });
    taken_targets.extend(result.claimed_target_file_ids.iter().copied());

    for variant in assignment_variants(context, active_patch_unit_ids) {
        let remaining_source_ids =
            remaining_source_ids(result, context, active_patch_unit_ids, &variant);
        let rankings = rank_all_sources_for_variant(context, &variant);
        let trusted_source_ids = trusted_exact_source_ids(context, &rankings, &variant);
        record_patch_rankings(
            result,
            context,
            &rankings,
            &remaining_source_ids,
            settings,
            &variant,
            &trusted_source_ids,
        );
        let assignable_source_ids = unblocked_source_ids(result, &remaining_source_ids);
        let assignments = optimal_variant_assignments(
            &rankings,
            &assignable_source_ids,
            &taken_targets,
            settings,
            &trusted_source_ids,
        );
        for source_id in assignment_order(&rankings, &assignable_source_ids) {
            if is_blocked_patch_source(result, source_id, &remaining_source_ids) {
                continue;
            }
            let target_id = match assignments.get(&source_id) {
                Some(&id) => id,
                None => {
                    if should_defer_unassigned_source(context, source_id, &variant) {
                        continue;
                    }
                    record_unassigned_source(result, source_id, &variant);
                    continue;
                }
            };
            let score = rankings[&source_id]
                .iter()
                .find_map(|(cid, s)| if *cid == target_id { Some(*s) } else { None })
                .unwrap_or(f64::INFINITY);
            taken_targets.insert(target_id);
            result.claimed_target_file_ids.insert(target_id);
            record_assigned_match(
                result,
                source_id,
                target_id,
                score,
                &rankings[&source_id],
                &variant,
            );
        }
    }
    record_unmatched_sources(result, active_patch_unit_ids);
}

fn remaining_source_ids(
    result: &UnitGeometryRemap,
    context: &UnitMatchContext,
    active_patch_unit_ids: &BTreeSet<u64>,
    variant: &str,
) -> BTreeSet<u64> {
    active_patch_unit_ids
        .iter()
        .copied()
        .filter(|id| source_needs_variant_match(result, context, *id, variant))
        .collect()
}

fn source_needs_variant_match(
    result: &UnitGeometryRemap,
    context: &UnitMatchContext,
    source_id: u64,
    variant: &str,
) -> bool {
    if !result.expanded_remap.contains_key(&source_id) {
        return true;
    }
    if context.source_variants.get(&source_id).map(String::as_str) != Some("Any") {
        return false;
    }
    let levels = result.match_levels.get(&source_id).map(String::as_str);
    if match_level_present(levels, "geometry:Any") {
        return false;
    }
    !match_level_present(levels, &format!("geometry:{variant}"))
}

fn match_level_present(levels: Option<&str>, expected: &str) -> bool {
    levels
        .unwrap_or_default()
        .split(',')
        .any(|level| level == expected)
}

fn assignment_variants(
    context: &UnitMatchContext,
    active_patch_unit_ids: &BTreeSet<u64>,
) -> Vec<String> {
    let mut variants: BTreeSet<String> = context
        .target_signatures
        .keys()
        .map(|id| {
            context
                .target_variants
                .get(id)
                .cloned()
                .unwrap_or_else(|| "Any".to_string())
        })
        .collect();
    for &id in active_patch_unit_ids {
        if let Some(v) = context.source_variants.get(&id)
            && v != "Any"
        {
            variants.insert(v.clone());
        }
    }
    if variants.is_empty() {
        variants.insert("Any".to_string());
    }
    let order = ["Unknown", "Any", "Stocky", "Slim"];
    order
        .iter()
        .filter_map(|name| {
            if variants.contains(*name) {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn rank_all_sources_for_variant(
    context: &UnitMatchContext,
    variant: &str,
) -> BTreeMap<u64, Vec<(u64, f64)>> {
    let targets = target_signatures_for_variant(context, variant);
    let mut out = BTreeMap::new();
    for (source_id, signature) in &context.source_signatures {
        if !source_can_apply_to_variant(context, *source_id, variant) {
            continue;
        }
        let source_targets = name_scoped_target_signatures(context, *source_id, &targets);
        let mut ranked: Vec<(u64, f64)> = source_targets
            .iter()
            .map(|(tid, sig)| (*tid, score_signatures(signature, sig)))
            .collect();
        ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        out.insert(*source_id, ranked);
    }
    out
}

fn name_scoped_target_signatures<'a>(
    context: &'a UnitMatchContext,
    source_id: u64,
    targets: &HashMap<u64, &'a UnitGeometrySignature>,
) -> HashMap<u64, &'a UnitGeometrySignature> {
    let source_name = context
        .source_names
        .get(&source_id)
        .and_then(|n| n.as_ref());
    let preferred: HashMap<u64, &'a UnitGeometrySignature> = targets
        .iter()
        .filter(|(tid, _)| {
            names_share_slot_and_piece(
                source_name,
                context.target_names.get(tid).and_then(|n| n.as_ref()),
            )
        })
        .map(|(k, v)| (*k, *v))
        .collect();
    if !preferred.is_empty() {
        return preferred;
    }
    let exact: HashMap<u64, &'a UnitGeometrySignature> = targets
        .iter()
        .filter(|(tid, _)| {
            names_share_part_scope(
                source_name,
                context.target_names.get(tid).and_then(|n| n.as_ref()),
            )
        })
        .map(|(k, v)| (*k, *v))
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    targets
        .iter()
        .filter(|(tid, _)| {
            name_scope_allows(
                source_name,
                context.target_names.get(tid).and_then(|n| n.as_ref()),
            )
        })
        .map(|(k, v)| (*k, *v))
        .collect()
}

fn trusted_exact_source_ids(
    context: &UnitMatchContext,
    rankings: &BTreeMap<u64, Vec<(u64, f64)>>,
    variant: &str,
) -> HashSet<u64> {
    let targets = target_signatures_for_variant(context, variant);
    let mut trusted = HashSet::new();
    for source_id in rankings.keys() {
        if has_unique_trusted_part_target(context, *source_id, &targets) {
            trusted.insert(*source_id);
            continue;
        }
        if has_named_slot_target_under_limit(context, *source_id, &targets, &rankings[source_id]) {
            trusted.insert(*source_id);
        }
    }
    trusted
}

fn has_unique_trusted_part_target(
    context: &UnitMatchContext,
    source_id: u64,
    targets: &HashMap<u64, &UnitGeometrySignature>,
) -> bool {
    let preferred = slot_and_piece_target_ids(context, source_id, targets);
    if !preferred.is_empty() {
        return preferred.len() == 1;
    }
    exact_part_target_ids(context, source_id, targets).len() == 1
}

fn has_named_slot_target_under_limit(
    context: &UnitMatchContext,
    source_id: u64,
    targets: &HashMap<u64, &UnitGeometrySignature>,
    ranked: &[(u64, f64)],
) -> bool {
    if ranked
        .first()
        .map(|(_, score)| *score > NAMED_SLOT_SCORE_LIMIT)
        .unwrap_or(true)
    {
        return false;
    }
    !exact_part_target_ids(context, source_id, targets).is_empty()
}

fn slot_and_piece_target_ids(
    context: &UnitMatchContext,
    source_id: u64,
    targets: &HashMap<u64, &UnitGeometrySignature>,
) -> HashSet<u64> {
    let source_name = context
        .source_names
        .get(&source_id)
        .and_then(|n| n.as_ref());
    targets
        .keys()
        .copied()
        .filter(|tid| {
            names_share_slot_and_piece(
                source_name,
                context.target_names.get(tid).and_then(|n| n.as_ref()),
            )
        })
        .collect()
}

fn exact_part_target_ids(
    context: &UnitMatchContext,
    source_id: u64,
    targets: &HashMap<u64, &UnitGeometrySignature>,
) -> HashSet<u64> {
    let source_name = context
        .source_names
        .get(&source_id)
        .and_then(|n| n.as_ref());
    targets
        .keys()
        .copied()
        .filter(|tid| {
            names_share_part_scope(
                source_name,
                context.target_names.get(tid).and_then(|n| n.as_ref()),
            )
        })
        .collect()
}

pub(crate) fn name_scope_allows(
    source: Option<&UnitCustomizationName>,
    target: Option<&UnitCustomizationName>,
) -> bool {
    match (source, target) {
        (Some(s), Some(t)) => names_share_part_scope(Some(s), Some(t)),
        _ => true,
    }
}

pub(crate) fn names_share_part_scope(
    source: Option<&UnitCustomizationName>,
    target: Option<&UnitCustomizationName>,
) -> bool {
    match (source, target) {
        (Some(s), Some(t)) => s.slot == t.slot,
        _ => false,
    }
}

pub(crate) fn names_share_slot_and_piece(
    source: Option<&UnitCustomizationName>,
    target: Option<&UnitCustomizationName>,
) -> bool {
    match (source, target) {
        (Some(s), Some(t)) => s.slot == t.slot && s.piece_type == t.piece_type,
        _ => false,
    }
}

fn target_signatures_for_variant<'a>(
    context: &'a UnitMatchContext,
    variant: &str,
) -> HashMap<u64, &'a UnitGeometrySignature> {
    context
        .target_signatures
        .iter()
        .filter(|(file_id, _)| {
            let target_variant = context
                .target_variants
                .get(file_id)
                .map(String::as_str)
                .unwrap_or("Unknown");
            target_can_receive_variant(target_variant, variant)
        })
        .map(|(k, v)| (*k, v))
        .collect()
}

fn target_can_receive_variant(target_variant: &str, requested: &str) -> bool {
    if target_variant == requested {
        return true;
    }
    matches!(requested, "Stocky" | "Slim") && target_variant == "Unknown"
}

fn source_can_apply_to_variant(context: &UnitMatchContext, source_id: u64, variant: &str) -> bool {
    let source_variant = context
        .source_variants
        .get(&source_id)
        .map(String::as_str)
        .unwrap_or("Any");
    if source_variant == "Any" {
        return variant != "Unknown";
    }
    source_variant == variant
}

fn record_patch_rankings(
    result: &mut UnitGeometryRemap,
    context: &UnitMatchContext,
    rankings: &BTreeMap<u64, Vec<(u64, f64)>>,
    patch_unit_ids: &BTreeSet<u64>,
    settings: &GeometryMatchSettings,
    variant: &str,
    trusted_source_ids: &HashSet<u64>,
) {
    let ids: Vec<u64> = patch_unit_ids
        .iter()
        .copied()
        .filter(|id| rankings.contains_key(id))
        .collect();
    for source_id in ids {
        let ranked = &rankings[&source_id];
        let current: Vec<(u64, f64)> = result.rankings.remove(&source_id).unwrap_or_default();
        let merged = merge_rankings(&current, ranked);
        result.rankings.insert(source_id, merged);
        let issue = ranking_issue(ranked, settings, trusted_source_ids.contains(&source_id));
        if let Some(reason) = issue {
            if should_defer_variant_issue(context, source_id, variant, reason) {
                continue;
            }
            let candidates: Vec<u64> = ranked.iter().take(3).map(|(id, _)| *id).collect();
            let full_reason = format!("{} for {} target variant", reason, variant);
            if reason == "ambiguous geometry match" {
                result.ambiguous.push(UnitGeometryIssue {
                    source_file_id: source_id,
                    reason: full_reason,
                    candidates,
                });
            } else {
                result.missing.push(UnitGeometryIssue {
                    source_file_id: source_id,
                    reason: full_reason,
                    candidates,
                });
            }
        }
    }
}

fn should_defer_variant_issue(
    context: &UnitMatchContext,
    source_id: u64,
    variant: &str,
    reason: &str,
) -> bool {
    let _ = reason;
    variant == "Any"
        && context
            .source_variants
            .get(&source_id)
            .map(String::as_str)
            .unwrap_or("Any")
            == "Any"
}

fn should_defer_unassigned_source(
    context: &UnitMatchContext,
    source_id: u64,
    variant: &str,
) -> bool {
    variant == "Any"
        && context
            .source_variants
            .get(&source_id)
            .map(String::as_str)
            .unwrap_or("Any")
            == "Any"
}

fn unblocked_source_ids(result: &UnitGeometryRemap, source_ids: &BTreeSet<u64>) -> BTreeSet<u64> {
    let blocked = issue_file_ids(result);
    source_ids.difference(&blocked).copied().collect()
}

fn merge_rankings(current: &[(u64, f64)], ranked: &[(u64, f64)]) -> Vec<(u64, f64)> {
    let mut merged: HashMap<u64, f64> = current.iter().copied().collect();
    for (tid, score) in ranked.iter().take(3) {
        merged
            .entry(*tid)
            .and_modify(|cur| {
                if *score < *cur {
                    *cur = *score;
                }
            })
            .or_insert(*score);
    }
    let mut out: Vec<(u64, f64)> = merged.into_iter().collect();
    out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(3);
    out
}

fn ranking_issue(
    ranked: &[(u64, f64)],
    settings: &GeometryMatchSettings,
    trusted_exact: bool,
) -> Option<&'static str> {
    if ranked.is_empty() {
        return Some("no target Unit geometry candidates");
    }
    if ranked[0].1 > settings.max_score && !trusted_exact {
        return Some("best geometry match exceeds score threshold");
    }
    if ranked.len() > 1 && ranked[1].1 - ranked[0].1 < settings.min_margin {
        return Some("ambiguous geometry match");
    }
    None
}

mod assignment;
use assignment::{assignment_order, optimal_variant_assignments};

fn is_blocked_patch_source(
    result: &UnitGeometryRemap,
    source_id: u64,
    patch_unit_ids: &BTreeSet<u64>,
) -> bool {
    if !patch_unit_ids.contains(&source_id) {
        return false;
    }
    result
        .missing
        .iter()
        .chain(result.ambiguous.iter())
        .any(|i| i.source_file_id == source_id)
}

fn issue_file_ids(result: &UnitGeometryRemap) -> BTreeSet<u64> {
    result
        .missing
        .iter()
        .chain(result.ambiguous.iter())
        .map(|issue| issue.source_file_id)
        .collect()
}

fn record_unassigned_source(result: &mut UnitGeometryRemap, source_id: u64, variant: &str) {
    result.missing.push(UnitGeometryIssue {
        source_file_id: source_id,
        reason: format!(
            "no unclaimed target Unit geometry candidate for {} target variant",
            variant
        ),
        candidates: Vec::new(),
    });
}

fn record_unmatched_sources(result: &mut UnitGeometryRemap, active_patch_unit_ids: &BTreeSet<u64>) {
    let assigned: BTreeSet<u64> = result.expanded_remap.keys().copied().collect();
    let issue_ids = issue_file_ids(result);
    for source_id in active_patch_unit_ids
        .difference(&assigned)
        .copied()
        .collect::<BTreeSet<u64>>()
        .difference(&issue_ids)
        .copied()
    {
        result.missing.push(UnitGeometryIssue {
            source_file_id: source_id,
            reason: "no target Unit geometry candidate after all target variants".to_string(),
            candidates: Vec::new(),
        });
    }
}

fn record_assigned_match(
    result: &mut UnitGeometryRemap,
    source_id: u64,
    target_id: u64,
    score: f64,
    ranked: &[(u64, f64)],
    variant: &str,
) {
    result
        .expanded_remap
        .entry(source_id)
        .or_default()
        .push(target_id);
    result.remap.entry(source_id).or_insert(target_id);
    let current = result
        .match_levels
        .get(&source_id)
        .cloned()
        .unwrap_or_default();
    let level = format!("geometry:{}", variant);
    result
        .match_levels
        .insert(source_id, append_match_level(&current, &level));
    let cur_score = result.scores.get(&source_id).copied();
    if cur_score.map(|s| score < s).unwrap_or(true) {
        result.scores.insert(source_id, score);
        result
            .margins
            .insert(source_id, margin_for_target(ranked, target_id));
    }
}

pub(crate) fn append_match_level(current: &str, level: &str) -> String {
    if current.is_empty() {
        return level.to_string();
    }
    if current.split(',').any(|p| p == level) {
        return current.to_string();
    }
    format!("{},{}", current, level)
}

fn margin_for_target(ranked: &[(u64, f64)], target_id: u64) -> f64 {
    let chosen = ranked
        .iter()
        .find_map(|(cid, s)| if *cid == target_id { Some(*s) } else { None })
        .unwrap_or(0.0);
    let alternatives: Vec<f64> = ranked
        .iter()
        .filter(|(cid, _)| *cid != target_id)
        .map(|(_, s)| *s)
        .collect();
    if alternatives.is_empty() {
        1.0
    } else {
        alternatives[0] - chosen
    }
}

pub(super) fn unmatched_target_ids(target: &StreamToc, result: &UnitGeometryRemap) -> Vec<u64> {
    target
        .entries
        .iter()
        .filter(|e| e.type_id == UNIT_ID)
        .map(|e| e.file_id)
        .filter(|fid| !result.claimed_target_file_ids.contains(fid))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_context(source_variants: &[(u64, &str)]) -> UnitMatchContext {
        UnitMatchContext {
            patch_unit_ids: BTreeSet::new(),
            source_signatures: HashMap::new(),
            target_signatures: HashMap::new(),
            source_names: HashMap::new(),
            target_names: HashMap::new(),
            source_variants: source_variants
                .iter()
                .map(|(id, variant)| (*id, (*variant).to_string()))
                .collect(),
            target_variants: HashMap::new(),
        }
    }

    #[test]
    fn any_source_matched_to_any_target_is_satisfied() {
        let context = empty_context(&[(1, "Any")]);
        let active = BTreeSet::from([1]);
        let mut result = UnitGeometryRemap::default();
        result.expanded_remap.insert(1, vec![11]);
        result.match_levels.insert(1, "geometry:Any".to_string());

        let remaining = remaining_source_ids(&result, &context, &active, "Stocky");

        assert!(remaining.is_empty());
    }

    #[test]
    fn any_source_matched_to_stocky_can_still_match_slim() {
        let context = empty_context(&[(1, "Any")]);
        let active = BTreeSet::from([1]);
        let mut result = UnitGeometryRemap::default();
        result.expanded_remap.insert(1, vec![11]);
        result.match_levels.insert(1, "geometry:Stocky".to_string());

        let remaining = remaining_source_ids(&result, &context, &active, "Slim");

        assert_eq!(remaining, BTreeSet::from([1]));
    }
}
