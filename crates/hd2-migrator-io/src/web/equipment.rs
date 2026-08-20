use crate::archive;
use crate::constants::UNIT_ID;
use crate::io::{BundleSlicer, DataSource};
use crate::target_exclusions::is_default_excluded_target;
use crate::unit::authority::ArmorMappingTable;
use crate::unit::helmet_authority::HelmetMappingTable;
use crate::web::migration::{PatchBytes, selectable_archive_entries};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EquipmentCategory {
    Armor,
    Helmet,
}

impl EquipmentCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Armor => "Armor",
            Self::Helmet => "Helmet",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebEquipmentOption {
    pub category: EquipmentCategory,
    pub hash: String,
    pub name: String,
    pub excluded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebDetectedSource {
    pub id: String,
    pub category: EquipmentCategory,
    pub unit_hits: usize,
    pub candidates: Vec<WebEquipmentOption>,
    pub resolved_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebEquipmentInspection {
    pub sources: Vec<WebDetectedSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebMigrationMapping {
    pub category: EquipmentCategory,
    pub source_hash: String,
    pub target_hash: String,
}

pub fn list_equipment_options() -> crate::Result<Vec<WebEquipmentOption>> {
    let mut options = Vec::new();
    for category in [EquipmentCategory::Armor, EquipmentCategory::Helmet] {
        options.extend(options_for_category(category)?);
    }
    Ok(options)
}

/// Inspect both equipment categories from one lightweight TOC scan.
pub fn inspect_equipment(patch: &PatchBytes) -> crate::Result<WebEquipmentInspection> {
    let unit_ids = patch_unit_ids(&patch.toc)?;
    Ok(WebEquipmentInspection {
        sources: detected_sources(&unit_ids)?,
    })
}

/// Refine ambiguous source groups using candidate archive TOCs from game data.
pub async fn inspect_equipment_with_source<S: DataSource + ?Sized>(
    patch: &PatchBytes,
    source: &S,
) -> crate::Result<WebEquipmentInspection> {
    let unit_ids = patch_unit_ids(&patch.toc)?;
    let mut inspection = inspect_equipment(patch)?;
    let bundle = if source.exists("bundles.nxa").await? {
        Some(BundleSlicer::open(source).await?)
    } else {
        None
    };
    for detected in &mut inspection.sources {
        if detected.candidates.len() < 2 {
            continue;
        }
        resolve_from_archive_overlap(detected, &unit_ids, source, bundle.as_ref()).await?;
    }
    Ok(inspection)
}

fn options_for_category(category: EquipmentCategory) -> crate::Result<Vec<WebEquipmentOption>> {
    let mut seen_names = HashSet::new();
    Ok(selectable_archive_entries(category.as_str())?
        .into_iter()
        .filter(|entry| seen_names.insert(entry.name.clone()))
        .map(|entry| WebEquipmentOption {
            category,
            hash: entry.hash.clone(),
            name: entry.name.clone(),
            excluded: category == EquipmentCategory::Armor
                && is_default_excluded_target(&entry.hash, &entry.name),
        })
        .collect())
}

fn detected_sources(unit_ids: &HashSet<u64>) -> crate::Result<Vec<WebDetectedSource>> {
    let mut sources = armor_sources(unit_ids)?;
    sources.extend(helmet_sources(unit_ids)?);
    sources.sort_by(|left, right| {
        category_order(left.category)
            .cmp(&category_order(right.category))
            .then_with(|| right.unit_hits.cmp(&left.unit_hits))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(sources)
}

fn armor_sources(unit_ids: &HashSet<u64>) -> crate::Result<Vec<WebDetectedSource>> {
    let table = ArmorMappingTable::bundled()?;
    let options = options_by_name(EquipmentCategory::Armor)?;
    let mut candidates_by_hits = HashMap::<Vec<u64>, Vec<WebEquipmentOption>>::new();
    for (name, parts) in table.entries() {
        let mut hits = parts
            .all_file_ids()
            .into_iter()
            .filter(|id| unit_ids.contains(id))
            .collect::<Vec<_>>();
        hits.sort_unstable();
        hits.dedup();
        if hits.is_empty() {
            continue;
        }
        if let Some(option) = options.get(name) {
            candidates_by_hits
                .entry(hits)
                .or_default()
                .push(option.clone());
        }
    }
    let maximal = maximal_hit_sets(candidates_by_hits.keys().cloned().collect());
    Ok(maximal
        .into_iter()
        .filter_map(|hits| {
            source_from_hits(
                EquipmentCategory::Armor,
                hits.clone(),
                candidates_by_hits.remove(&hits)?,
            )
        })
        .collect())
}

fn helmet_sources(unit_ids: &HashSet<u64>) -> crate::Result<Vec<WebDetectedSource>> {
    let table = HelmetMappingTable::bundled()?;
    let options = options_by_name(EquipmentCategory::Helmet)?;
    let mut by_unit = HashMap::<u64, Vec<WebEquipmentOption>>::new();
    for (name, unit_id) in table.entries() {
        if unit_ids.contains(&unit_id)
            && let Some(option) = options.get(name)
        {
            by_unit.entry(unit_id).or_default().push(option.clone());
        }
    }
    Ok(by_unit
        .into_iter()
        .filter_map(|(unit_id, candidates)| {
            source_from_hits(EquipmentCategory::Helmet, vec![unit_id], candidates)
        })
        .collect())
}

fn source_from_hits(
    category: EquipmentCategory,
    hits: Vec<u64>,
    mut candidates: Vec<WebEquipmentOption>,
) -> Option<WebDetectedSource> {
    candidates.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.hash.cmp(&right.hash))
    });
    candidates.dedup_by(|left, right| left.name == right.name);
    if candidates.is_empty() {
        return None;
    }
    let resolved_hash = Some(candidates[0].hash.clone());
    Some(WebDetectedSource {
        id: source_id(category, &hits),
        category,
        unit_hits: hits.len(),
        candidates,
        resolved_hash,
    })
}

fn maximal_hit_sets(hit_sets: Vec<Vec<u64>>) -> Vec<Vec<u64>> {
    hit_sets
        .iter()
        .filter(|candidate| {
            !hit_sets.iter().any(|other| {
                candidate.len() < other.len()
                    && candidate.iter().all(|id| other.binary_search(id).is_ok())
            })
        })
        .cloned()
        .collect()
}

async fn resolve_from_archive_overlap<S: DataSource + ?Sized>(
    detected: &mut WebDetectedSource,
    patch_units: &HashSet<u64>,
    source: &S,
    bundle: Option<&BundleSlicer>,
) -> crate::Result<()> {
    let mut scores = Vec::new();
    for candidate in &detected.candidates {
        let Ok(toc) = load_toc(source, bundle, &candidate.hash).await else {
            continue;
        };
        let archive_units = patch_unit_ids(&toc)?;
        scores.push((
            candidate.hash.clone(),
            patch_units.intersection(&archive_units).count(),
        ));
    }
    if let Some(hash) = unique_best_hash(&scores) {
        detected.resolved_hash = Some(hash.to_string());
    }
    Ok(())
}

fn unique_best_hash(scores: &[(String, usize)]) -> Option<&str> {
    let best = scores
        .iter()
        .map(|(_, score)| *score)
        .max()
        .unwrap_or_default();
    let winners = scores
        .iter()
        .filter(|(_, score)| *score == best)
        .collect::<Vec<_>>();
    (best > 0 && winners.len() == 1).then(|| winners[0].0.as_str())
}

pub(super) async fn load_toc<S: DataSource + ?Sized>(
    source: &S,
    bundle: Option<&BundleSlicer>,
    hash: &str,
) -> crate::Result<Vec<u8>> {
    if !source.exists(hash).await?
        && let Some(bundle) = bundle
        && bundle.has_package(hash)
    {
        return bundle.load_package(source, hash).await;
    }
    source.read_full(hash).await
}

fn options_by_name(
    category: EquipmentCategory,
) -> crate::Result<HashMap<String, WebEquipmentOption>> {
    Ok(options_for_category(category)?
        .into_iter()
        .map(|option| (option.name.clone(), option))
        .collect())
}

pub(super) fn patch_unit_ids(toc: &[u8]) -> crate::Result<HashSet<u64>> {
    Ok(archive::list_file_ids_from_bytes(toc)?
        .remove(&UNIT_ID)
        .unwrap_or_default()
        .into_iter()
        .collect())
}

fn source_id(category: EquipmentCategory, hits: &[u64]) -> String {
    let prefix = match category {
        EquipmentCategory::Armor => "armor",
        EquipmentCategory::Helmet => "helmet",
    };
    let ids = hits
        .iter()
        .map(|id| format!("{id:016x}"))
        .collect::<Vec<_>>()
        .join("-");
    format!("{prefix}:{ids}")
}

fn category_order(category: EquipmentCategory) -> u8 {
    match category {
        EquipmentCategory::Armor => 0,
        EquipmentCategory::Helmet => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_maximal_overlapping_hit_sets() {
        let sets = vec![vec![1], vec![1, 2], vec![3]];
        assert_eq!(maximal_hit_sets(sets), vec![vec![1, 2], vec![3]]);
    }

    #[test]
    fn shared_hits_default_to_the_first_stable_candidate() {
        let candidates = vec![
            option(EquipmentCategory::Armor, "a", "Alpha"),
            option(EquipmentCategory::Armor, "b", "Beta"),
        ];
        let source = source_from_hits(EquipmentCategory::Armor, vec![1], candidates).unwrap();
        assert_eq!(source.resolved_hash.as_deref(), Some("a"));
        assert_eq!(source.candidates.len(), 2);
    }

    #[test]
    fn archive_overlap_requires_one_nonzero_best_candidate() {
        assert_eq!(
            unique_best_hash(&[("a".to_string(), 2), ("b".to_string(), 1)]),
            Some("a")
        );
        assert_eq!(
            unique_best_hash(&[("a".to_string(), 2), ("b".to_string(), 2)]),
            None
        );
        assert_eq!(unique_best_hash(&[("a".to_string(), 0)]), None);
    }

    fn option(category: EquipmentCategory, hash: &str, name: &str) -> WebEquipmentOption {
        WebEquipmentOption {
            category,
            hash: hash.to_string(),
            name: name.to_string(),
            excluded: false,
        }
    }
}
