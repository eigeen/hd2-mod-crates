//! Geometry-based Unit matching for armor migration.
//!
//! Ports mod_armor_migrator/unit_geometry.py. Builds vertex-distribution
//! signatures from Unit toc/gpu blobs and uses them to pair source Units to
//! target Units when archive-order pairing is unsafe (Unit slots are not
//! ordinal).
//!
//! The entry point for Mode A is [uild_unit_geometry_remap].

mod context;
mod matching;
mod parsing;
mod scoring;
mod signature;

pub use parsing::parse_unit_points;
pub use scoring::score_signatures;
pub use signature::{build_archive_signatures, build_patch_unit_signatures, build_unit_signature};

pub(crate) use matching::append_match_level;
pub(crate) use scoring::{downsample_points, vector_distance};

use crate::archive::StreamToc;
use crate::constants::UNIT_ID;
use crate::unit::authority::UnitAuthorityMatch;
use std::collections::{BTreeSet, HashMap, HashSet};

pub type Point3 = (f64, f64, f64);
pub type Matrix4 = [f64; 16];

#[derive(Debug, Clone)]
pub struct GeometryMatchSettings {
    pub max_score: f64,
    pub min_margin: f64,
    pub sample_count: usize,
    pub quantiles: Vec<f64>,
}

impl Default for GeometryMatchSettings {
    fn default() -> Self {
        Self {
            max_score: 1.5,
            min_margin: 0.0,
            sample_count: 96,
            quantiles: vec![0.10, 0.25, 0.50, 0.75, 0.90],
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnitGeometrySignature {
    pub file_id: u64,
    pub points: Vec<Point3>,
    pub sample_points: Vec<Point3>,
    pub vertex_count: usize,
    pub center: Point3,
    pub extents: Point3,
    pub diagonal: f64,
    pub axis_quantiles: Vec<f64>,
    pub radial_quantiles: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct UnitGeometryIssue {
    pub source_file_id: u64,
    pub reason: String,
    pub candidates: Vec<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct UnitGeometryRemap {
    pub remap: HashMap<u64, u64>,
    pub expanded_remap: HashMap<u64, Vec<u64>>,
    pub match_levels: HashMap<u64, String>,
    pub scores: HashMap<u64, f64>,
    pub margins: HashMap<u64, f64>,
    pub rankings: HashMap<u64, Vec<(u64, f64)>>,
    pub missing: Vec<UnitGeometryIssue>,
    pub ambiguous: Vec<UnitGeometryIssue>,
    pub extra_unit_file_ids: Vec<u64>,
    pub claimed_target_file_ids: HashSet<u64>,
    pub empty_source_file_ids: HashSet<u64>,
}

impl UnitGeometryRemap {
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.ambiguous.is_empty()
    }
}

pub fn build_unit_geometry_remap(
    patch: &StreamToc,
    source: &StreamToc,
    target: &StreamToc,
    settings: &GeometryMatchSettings,
    authority_matches: &[UnitAuthorityMatch],
) -> crate::Result<UnitGeometryRemap> {
    let context = context::build_match_context(patch, source, target, settings);
    let mut result = UnitGeometryRemap::default();
    result.empty_source_file_ids =
        context::empty_patch_source_unit_ids(patch, &context.patch_unit_ids, settings);
    let active_patch_unit_ids: BTreeSet<u64> = context
        .patch_unit_ids
        .iter()
        .filter(|id| !result.empty_source_file_ids.contains(id))
        .copied()
        .collect();
    apply_authority_matches(
        &mut result,
        target,
        &active_patch_unit_ids,
        authority_matches,
    );
    let geometry_source_ids = unassigned_source_ids(&result, &active_patch_unit_ids);
    matching::record_missing_patch_units(&mut result, &context, &geometry_source_ids);
    matching::assign_geometry_matches(&mut result, &context, &geometry_source_ids, settings);
    crate::unit::body_shape::apply_body_variant_pair_tiebreak(
        &mut result,
        &context.source_signatures,
        &context.target_signatures,
        &context.source_names,
        &context.target_variants,
        &geometry_source_ids,
    );
    result.extra_unit_file_ids = matching::unmatched_target_ids(target, &result);
    Ok(result)
}

fn apply_authority_matches(
    result: &mut UnitGeometryRemap,
    target: &StreamToc,
    active_patch_unit_ids: &BTreeSet<u64>,
    authority_matches: &[UnitAuthorityMatch],
) {
    let target_unit_ids = target_unit_file_ids(target);
    for authority_match in authority_matches {
        if !active_patch_unit_ids.contains(&authority_match.source_file_id) {
            continue;
        }
        if !target_unit_ids.contains(&authority_match.target_file_id) {
            continue;
        }
        result.remap.insert(
            authority_match.source_file_id,
            authority_match.target_file_id,
        );
        result.expanded_remap.insert(
            authority_match.source_file_id,
            vec![authority_match.target_file_id],
        );
        result
            .claimed_target_file_ids
            .insert(authority_match.target_file_id);
        result.match_levels.insert(
            authority_match.source_file_id,
            format!("authority:{}", authority_match.part_label),
        );
    }
}

fn target_unit_file_ids(target: &StreamToc) -> HashSet<u64> {
    target
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .map(|entry| entry.file_id)
        .collect()
}

fn unassigned_source_ids(
    result: &UnitGeometryRemap,
    active_patch_unit_ids: &BTreeSet<u64>,
) -> BTreeSet<u64> {
    active_patch_unit_ids
        .iter()
        .copied()
        .filter(|id| !result.expanded_remap.contains_key(id))
        .collect()
}

pub fn format_unit_geometry_issues(result: &UnitGeometryRemap, limit: usize) -> String {
    let issues: Vec<&UnitGeometryIssue> = result
        .missing
        .iter()
        .chain(result.ambiguous.iter())
        .collect();
    let total = issues.len();
    let shown_limit = issues.len().min(limit);
    let mut parts = Vec::new();
    for issue in issues.iter().take(shown_limit) {
        let suffix = if issue.candidates.is_empty() {
            String::new()
        } else {
            format!(", candidates={:?}", issue.candidates)
        };
        parts.push(format!(
            "{}: {}{}",
            issue.source_file_id, issue.reason, suffix
        ));
    }
    if total > limit {
        parts.push(format!("... {} more", total - limit));
    }
    parts.join("; ")
}
