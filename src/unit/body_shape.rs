//! Body-shape tie-breakers for Stocky/Slim Unit geometry matches.
//!
//! Ports mod_armor_migrator/unit_body_shape.py. Used by
//! [crate::unit::geometry::build_unit_geometry_remap] both *before* the main
//! greedy assignment (pair preassignment based on named Stocky/Slim source
//! pairs) and *after* (tie-breaking by directed expansion when both target
//! Units are tagged Unknown).

mod preassignment;
mod shape;

use crate::unit::geometry::{UnitGeometryRemap, UnitGeometrySignature};
use crate::unit::names::UnitCustomizationName;
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyVariantPair {
    pub stocky_source_id: u64,
    pub slim_source_id: u64,
}

pub struct BodyPairPreassignmentRequest<'a> {
    pub result: &'a mut UnitGeometryRemap,
    pub source_signatures: &'a HashMap<u64, UnitGeometrySignature>,
    pub target_signatures: &'a HashMap<u64, UnitGeometrySignature>,
    pub source_names: &'a HashMap<u64, Option<UnitCustomizationName>>,
    pub target_names: &'a HashMap<u64, Option<UnitCustomizationName>>,
    pub target_variants: &'a HashMap<u64, String>,
    pub active_source_ids: &'a BTreeSet<u64>,
}

pub(super) const EXPANSION_SAMPLE_COUNT: usize = 512;
pub(super) const EXPANSION_THRESHOLD: f64 = 0.00005;
pub(super) const PAIR_SCORE_LIMIT: f64 = 1.0;
pub(super) const NAMED_UNKNOWN_PAIR_SCORE_LIMIT: f64 = 2.5;
pub(super) const DEPTH_EXTENT_THRESHOLD: f64 = 0.0005;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyType {
    Stocky,
    Slim,
    Any,
    Unknown,
}

impl BodyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BodyType::Stocky => "Stocky",
            BodyType::Slim => "Slim",
            BodyType::Any => "Any",
            BodyType::Unknown => "Unknown",
        }
    }

    pub fn from_str_normalize(s: &str) -> Self {
        match s {
            "Stocky" => BodyType::Stocky,
            "Slim" => BodyType::Slim,
            "Any" => BodyType::Any,
            _ => BodyType::Unknown,
        }
    }
}

#[allow(dead_code)]
pub fn detect_body_type(toc_data: &[u8], _gpu_data: &[u8]) -> BodyType {
    BodyType::from_str_normalize(crate::unit::names::body_variant(toc_data))
}

pub fn apply_body_variant_pair_preassignment(
    request: BodyPairPreassignmentRequest<'_>,
) -> HashSet<u64> {
    preassignment::apply_body_variant_pair_preassignment(request)
}

pub fn apply_body_variant_pair_tiebreak(
    result: &mut UnitGeometryRemap,
    source_signatures: &HashMap<u64, UnitGeometrySignature>,
    target_signatures: &HashMap<u64, UnitGeometrySignature>,
    source_names: &HashMap<u64, Option<UnitCustomizationName>>,
    target_variants: &HashMap<u64, String>,
    active_source_ids: &BTreeSet<u64>,
) {
    shape::apply_body_variant_pair_tiebreak(
        result,
        source_signatures,
        target_signatures,
        source_names,
        target_variants,
        active_source_ids,
    );
}
