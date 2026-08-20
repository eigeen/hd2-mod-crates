//! Browser-compatible Unit repatching over an async game-data source.

use super::migration::PatchBytes;
use crate::archive::toc_only::{
    TocEntryLocation, TocHeader, TocOnlyPackage, parse_entry_locations, retain_locations,
};
use crate::archive::{StreamToc, TocEntry, dsar};
use crate::constants::{DSAR_MAGIC, UNIT_ID};
use crate::io::{BundleSlicer, DataSource};
use crate::migrator::mode_a_web::WebProgress;
use crate::unit::culling::{
    CullingPolicy, inspect_unit_culling, replace_patch_culling_with_target,
};
use crate::unit::repatch::{LatestUnitParts, RepatchOutcome, repatch_unit};
use eyre::WrapErr;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MissingUnitPolicy {
    #[default]
    Drop,
    Keep,
    Fail,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UnitRepatchOptions {
    pub missing_unit_policy: MissingUnitPolicy,
    pub culling_policy: CullingPolicy,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitRepatchSummary {
    pub unit_count: usize,
    pub updated_units: usize,
    pub already_current_units: usize,
    pub removed_units: usize,
    pub failed_units: usize,
    pub scanned_archives: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CullingSetSummary {
    pub unit_count: usize,
    pub parsed_unit_count: usize,
    pub culling_unit_count: usize,
    pub culling_mesh_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepatchCullingSummary {
    pub patch: CullingSetSummary,
    pub target: Option<CullingSetSummary>,
}

#[derive(Debug, Clone)]
pub struct UnitRepatchResult {
    pub toc: Vec<u8>,
    pub gpu: Option<Vec<u8>>,
    pub stream: Option<Vec<u8>>,
    pub summary: UnitRepatchSummary,
}

#[derive(Debug, Clone)]
pub struct UnitRepatchPlan {
    pub patch: Option<TocOnlyPackage>,
    pub full_patch: Option<StreamToc>,
    pub original_toc: Vec<u8>,
    pub summary: UnitRepatchSummary,
}

pub fn summarize_patch_culling(patch_toc: &[u8]) -> crate::Result<RepatchCullingSummary> {
    let patch = TocOnlyPackage::parse(patch_toc).wrap_err("parse patch TOC")?;
    Ok(RepatchCullingSummary {
        patch: summarize_package_culling(&patch)?,
        target: None,
    })
}

pub async fn summarize_repatch_culling<S: DataSource + ?Sized>(
    patch_name: &str,
    patch_toc: &[u8],
    source: &S,
) -> crate::Result<RepatchCullingSummary> {
    let patch = TocOnlyPackage::parse(patch_toc).wrap_err("parse patch TOC")?;
    let wanted = patch_unit_ids(&patch);
    let patch_summary = summarize_package_culling(&patch)?;
    let mut lookup = LatestUnitLookup::new(wanted.clone(), patch_name, true);
    lookup.load(source, None).await?;
    Ok(RepatchCullingSummary {
        patch: patch_summary,
        target: Some(summarize_lookup_culling(wanted.len(), &lookup)),
    })
}

fn summarize_package_culling(patch: &TocOnlyPackage) -> crate::Result<CullingSetSummary> {
    let units = patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .collect::<Vec<_>>();
    let counts = units
        .iter()
        .filter_map(|entry| inspect_unit_culling(&entry.toc_data).ok())
        .map(culling_mesh_count);
    Ok(summarize_culling_counts(units.len(), counts))
}

fn summarize_lookup_culling(unit_count: usize, lookup: &LatestUnitLookup) -> CullingSetSummary {
    summarize_culling_counts(unit_count, lookup.culling_counts.values().copied())
}

fn summarize_culling_counts(
    unit_count: usize,
    counts: impl Iterator<Item = usize>,
) -> CullingSetSummary {
    let counts = counts.collect::<Vec<_>>();
    CullingSetSummary {
        unit_count,
        parsed_unit_count: counts.len(),
        culling_unit_count: counts.iter().filter(|count| **count > 0).count(),
        culling_mesh_count: counts.iter().sum(),
    }
}

fn culling_mesh_count(inspection: crate::unit::culling::CullingInspection) -> usize {
    inspection.culling_meshes.len()
}

pub async fn repatch_units<S: DataSource + ?Sized>(
    patch_name: &str,
    patch_toc: &[u8],
    options: UnitRepatchOptions,
    source: &S,
) -> crate::Result<UnitRepatchResult> {
    repatch_units_with_progress(patch_name, patch_toc, options, source, None).await
}

pub async fn repatch_units_with_progress<S: DataSource + ?Sized>(
    patch_name: &str,
    patch_toc: &[u8],
    options: UnitRepatchOptions,
    source: &S,
    progress: Option<&dyn WebProgress>,
) -> crate::Result<UnitRepatchResult> {
    let plan =
        repatch_units_plan_with_progress(patch_name, patch_toc, options, source, progress).await?;
    let toc = match plan.patch {
        Some(patch) => patch.serialize().wrap_err("serialize updated patch TOC")?,
        None => patch_toc.to_vec(),
    };
    Ok(UnitRepatchResult {
        toc,
        gpu: None,
        stream: None,
        summary: plan.summary,
    })
}

pub async fn repatch_units_plan_with_progress<S: DataSource + ?Sized>(
    patch_name: &str,
    patch_toc: &[u8],
    options: UnitRepatchOptions,
    source: &S,
    progress: Option<&dyn WebProgress>,
) -> crate::Result<UnitRepatchPlan> {
    if options.culling_policy == CullingPolicy::Target {
        eyre::bail!("target culling policy requires the patch GPU and stream sidecars");
    }
    let mut patch = TocOnlyPackage::parse(patch_toc).wrap_err("parse patch TOC")?;
    let wanted = patch_unit_ids(&patch);
    if wanted.is_empty() {
        return Ok(UnitRepatchPlan {
            patch: None,
            full_patch: None,
            original_toc: patch_toc.to_vec(),
            summary: no_units_summary(),
        });
    }
    let mut lookup = LatestUnitLookup::new(wanted, patch_name, false);
    lookup.load(source, progress).await?;
    enforce_missing_policy(&lookup.missing, options.missing_unit_policy)?;
    let summary = apply_latest_units(&mut patch, lookup, options.missing_unit_policy);
    let changed = summary.updated_units > 0 || summary.removed_units > 0;
    Ok(UnitRepatchPlan {
        patch: changed.then_some(patch),
        full_patch: None,
        original_toc: patch_toc.to_vec(),
        summary,
    })
}

/// Repatch a complete patch triple, loading GPU sidecars only for target culling.
pub async fn repatch_patch_with_progress<S: DataSource + ?Sized>(
    patch: PatchBytes,
    options: UnitRepatchOptions,
    source: &S,
    progress: Option<&dyn WebProgress>,
) -> crate::Result<UnitRepatchResult> {
    let mut plan = repatch_patch_plan_with_progress(patch, options, source, progress).await?;
    if let Some(mut full_patch) = plan.full_patch.take() {
        let (toc, gpu, stream) = full_patch.serialize();
        return Ok(UnitRepatchResult {
            toc,
            gpu: Some(gpu),
            stream: Some(stream),
            summary: plan.summary,
        });
    }
    let toc = match plan.patch {
        Some(patch) => patch.serialize().wrap_err("serialize updated patch TOC")?,
        None => plan.original_toc,
    };
    Ok(UnitRepatchResult {
        toc,
        gpu: None,
        stream: None,
        summary: plan.summary,
    })
}

pub async fn repatch_patch_plan_with_progress<S: DataSource + ?Sized>(
    patch: PatchBytes,
    options: UnitRepatchOptions,
    source: &S,
    progress: Option<&dyn WebProgress>,
) -> crate::Result<UnitRepatchPlan> {
    if options.culling_policy == CullingPolicy::Patch {
        return repatch_units_plan_with_progress(
            &patch.name,
            &patch.toc,
            options,
            source,
            progress,
        )
        .await;
    }
    repatch_with_target_culling(patch, options, source, progress).await
}

async fn repatch_with_target_culling<S: DataSource + ?Sized>(
    patch: PatchBytes,
    options: UnitRepatchOptions,
    source: &S,
    progress: Option<&dyn WebProgress>,
) -> crate::Result<UnitRepatchPlan> {
    let mut archive =
        StreamToc::from_buffers(&patch.toc, &patch.gpu, &patch.stream, patch.name.clone())?;
    let wanted = unit_ids(&archive);
    if wanted.is_empty() {
        return Ok(UnitRepatchPlan {
            patch: None,
            full_patch: None,
            original_toc: patch.toc,
            summary: no_units_summary(),
        });
    }
    let mut lookup = LatestUnitLookup::new(wanted, &patch.name, true);
    lookup.load(source, progress).await?;
    enforce_missing_policy(&lookup.missing, MissingUnitPolicy::Fail)?;
    let targets = load_target_units(source, &lookup.packages_by_unit).await?;
    let summary =
        apply_target_culling_units(&mut archive, &targets, lookup, options.missing_unit_policy)?;
    Ok(UnitRepatchPlan {
        patch: None,
        full_patch: Some(archive),
        original_toc: patch.toc,
        summary,
    })
}

fn unit_ids(patch: &StreamToc) -> HashSet<u64> {
    patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .map(|entry| entry.file_id)
        .collect()
}

async fn load_target_units<S: DataSource + ?Sized>(
    source: &S,
    packages_by_unit: &HashMap<u64, String>,
) -> crate::Result<HashMap<u64, TocEntry>> {
    let bundle = if source.exists("bundles.nxa").await? {
        Some(BundleSlicer::open(source).await?)
    } else {
        None
    };
    let mut packages = packages_by_unit.values().cloned().collect::<Vec<_>>();
    packages.sort();
    packages.dedup();
    let mut targets = HashMap::new();
    for package in packages {
        let archive = load_full_archive(source, bundle.as_ref(), &package).await?;
        collect_target_units(&archive, packages_by_unit, &package, &mut targets);
    }
    Ok(targets)
}

async fn load_full_archive<S: DataSource + ?Sized>(
    source: &S,
    bundle: Option<&BundleSlicer>,
    package: &str,
) -> crate::Result<StreamToc> {
    let (mut toc, gpu, stream) = if source.exists(package).await? {
        let toc = source.read_full(package).await?;
        let gpu = read_optional_sidecar(source, &format!("{package}.gpu_resources")).await?;
        let stream = read_optional_sidecar(source, &format!("{package}.stream")).await?;
        (toc, gpu, stream)
    } else {
        let bundle = bundle.ok_or_else(|| eyre::eyre!("game archive {package} is unavailable"))?;
        bundle.load_triple(source, package).await?
    };
    if package_magic(&toc) == Some(DSAR_MAGIC) {
        toc = dsar::decompress(&toc)?;
    }
    StreamToc::from_buffers(&toc, &gpu, &stream, package.to_owned())
}

async fn read_optional_sidecar<S: DataSource + ?Sized>(
    source: &S,
    path: &str,
) -> crate::Result<Vec<u8>> {
    if source.exists(path).await? {
        source.read_full(path).await
    } else {
        Ok(Vec::new())
    }
}

fn collect_target_units(
    archive: &StreamToc,
    packages_by_unit: &HashMap<u64, String>,
    package: &str,
    targets: &mut HashMap<u64, TocEntry>,
) {
    for entry in archive
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
    {
        if packages_by_unit
            .get(&entry.file_id)
            .is_some_and(|value| value == package)
        {
            targets.insert(entry.file_id, entry.clone());
        }
    }
}

fn apply_target_culling_units(
    patch: &mut StreamToc,
    targets: &HashMap<u64, TocEntry>,
    lookup: LatestUnitLookup,
    policy: MissingUnitPolicy,
) -> crate::Result<UnitRepatchSummary> {
    let unit_count = patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .count();
    let mut summary = target_summary(unit_count, &lookup);
    let mut output = Vec::with_capacity(patch.entries.len());
    for entry in patch.entries.drain(..) {
        if let Some(entry) =
            update_full_entry(entry, targets, &lookup.missing, policy, &mut summary)?
        {
            output.push(entry);
        }
    }
    patch.entries = output;
    Ok(summary)
}

fn target_summary(unit_count: usize, lookup: &LatestUnitLookup) -> UnitRepatchSummary {
    UnitRepatchSummary {
        unit_count,
        updated_units: 0,
        already_current_units: 0,
        removed_units: 0,
        failed_units: 0,
        scanned_archives: lookup.scanned_archives,
        warnings: lookup.warnings.clone(),
    }
}

fn update_full_entry(
    mut entry: TocEntry,
    targets: &HashMap<u64, TocEntry>,
    missing: &HashSet<u64>,
    policy: MissingUnitPolicy,
    summary: &mut UnitRepatchSummary,
) -> crate::Result<Option<TocEntry>> {
    if entry.type_id != UNIT_ID {
        return Ok(Some(entry));
    }
    if missing.contains(&entry.file_id) {
        return Ok(handle_missing_full_entry(entry, policy, summary));
    }
    let target = targets.get(&entry.file_id).ok_or_else(|| {
        eyre::eyre!(
            "loaded target Unit 0x{:016x} has no full sidecar data",
            entry.file_id
        )
    })?;
    let latest = LatestUnitParts::parse(&target.toc_data)?;
    match repatch_unit(&mut entry.toc_data, &latest)? {
        RepatchOutcome::Updated { .. } => summary.updated_units += 1,
        RepatchOutcome::AlreadyCurrent => summary.already_current_units += 1,
    }
    Ok(Some(
        replace_patch_culling_with_target(&entry, target)
            .wrap_err_with(|| format!("replace repatched Unit 0x{:016x} culling", entry.file_id))?,
    ))
}

fn handle_missing_full_entry(
    entry: TocEntry,
    policy: MissingUnitPolicy,
    summary: &mut UnitRepatchSummary,
) -> Option<TocEntry> {
    if matches!(policy, MissingUnitPolicy::Drop) {
        summary.removed_units += 1;
        summary
            .warnings
            .push(format!("removed missing Unit {:016x}", entry.file_id));
        return None;
    }
    summary
        .warnings
        .push(format!("kept missing Unit {:016x}", entry.file_id));
    Some(entry)
}

fn no_units_summary() -> UnitRepatchSummary {
    UnitRepatchSummary {
        unit_count: 0,
        updated_units: 0,
        already_current_units: 0,
        removed_units: 0,
        failed_units: 0,
        scanned_archives: 0,
        warnings: vec!["patch contains no Unit resources".to_string()],
    }
}

struct LatestUnitLookup {
    wanted: HashSet<u64>,
    found: HashMap<u64, LatestUnitParts>,
    missing: HashSet<u64>,
    preferred_archive: Option<String>,
    scanned_archives: usize,
    warnings: Vec<String>,
    packages_by_unit: HashMap<u64, String>,
    culling_counts: HashMap<u64, usize>,
    collect_culling_counts: bool,
}

impl LatestUnitLookup {
    fn new(wanted: HashSet<u64>, patch_name: &str, collect_culling_counts: bool) -> Self {
        Self {
            missing: wanted.clone(),
            wanted,
            found: HashMap::new(),
            preferred_archive: archive_prefix(patch_name),
            scanned_archives: 0,
            warnings: Vec::new(),
            packages_by_unit: HashMap::new(),
            culling_counts: HashMap::new(),
            collect_culling_counts,
        }
    }

    async fn load<S: DataSource + ?Sized>(
        &mut self,
        source: &S,
        progress: Option<&dyn WebProgress>,
    ) -> crate::Result<()> {
        if self.wanted.is_empty() {
            return Ok(());
        }
        if source.exists("bundles.nxa").await? {
            self.load_bundled(source, progress).await
        } else {
            self.load_legacy(source, progress).await
        }
    }

    async fn load_legacy<S: DataSource + ?Sized>(
        &mut self,
        source: &S,
        progress: Option<&dyn WebProgress>,
    ) -> crate::Result<()> {
        let packages = source
            .list_packages()
            .await
            .wrap_err("list game archives")?;
        for package in prioritize(packages, self.preferred_archive.as_deref()) {
            notify_scan_progress(progress, &package)?;
            self.load_legacy_package(source, &package).await?;
            if self.missing.is_empty() {
                break;
            }
        }
        Ok(())
    }

    async fn load_bundled<S: DataSource + ?Sized>(
        &mut self,
        source: &S,
        progress: Option<&dyn WebProgress>,
    ) -> crate::Result<()> {
        let slicer = BundleSlicer::open(source).await?;
        let packages = slicer
            .packages
            .keys()
            .filter_map(|name| archive_basename(name))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        for package in prioritize(packages, self.preferred_archive.as_deref()) {
            notify_scan_progress(progress, &package)?;
            let toc = slicer.load_package(source, &package).await?;
            self.load_package_bytes(&package, &toc);
            if self.missing.is_empty() {
                break;
            }
        }
        Ok(())
    }

    async fn load_legacy_package<S: DataSource + ?Sized>(
        &mut self,
        source: &S,
        package: &str,
    ) -> crate::Result<()> {
        let prefix = source.read_range(package, 0, 72).await?;
        if package_magic(&prefix) == Some(DSAR_MAGIC) {
            return self.load_dsar_package(source, package).await;
        }
        let header = TocHeader::parse(&prefix)?;
        let table = source
            .read_range(package, 0, header.table_size()? as u64)
            .await?;
        let locations = retain_locations(
            &parse_entry_locations(&table, &header)?,
            UNIT_ID,
            &self.missing,
        );
        self.scanned_archives += 1;
        if locations.is_empty() {
            return Ok(());
        }
        let range = location_range(&locations)?;
        let bodies = source.read_range(package, range.0, range.1).await?;
        self.insert_locations(package, &locations, range.0, &bodies);
        Ok(())
    }

    async fn load_dsar_package<S: DataSource + ?Sized>(
        &mut self,
        source: &S,
        package: &str,
    ) -> crate::Result<()> {
        let compressed = source.read_full(package).await?;
        match dsar::decompress(&compressed) {
            Ok(toc) => self.load_package_bytes(package, &toc),
            Err(error) => {
                self.scanned_archives += 1;
                self.warnings.push(format!("{package}: {error}"));
            }
        }
        Ok(())
    }

    fn load_package_bytes(&mut self, package: &str, toc: &[u8]) {
        self.scanned_archives += 1;
        let result = TocHeader::parse(toc)
            .and_then(|header| parse_entry_locations(toc, &header))
            .map(|locations| retain_locations(&locations, UNIT_ID, &self.missing));
        match result {
            Ok(locations) => self.insert_locations(package, &locations, 0, toc),
            Err(error) => self.warnings.push(format!("{package}: {error}")),
        }
    }

    fn insert_locations(
        &mut self,
        package: &str,
        locations: &[TocEntryLocation],
        range_start: u64,
        data: &[u8],
    ) {
        for location in locations {
            self.insert_location(package, location, range_start, data);
        }
    }

    fn insert_location(
        &mut self,
        package: &str,
        location: &TocEntryLocation,
        range_start: u64,
        data: &[u8],
    ) {
        let body = match location_body(location, range_start, data) {
            Ok(body) => body,
            Err(error) => return self.warn_location(package, location.file_id, error),
        };
        let parts = match LatestUnitParts::parse(body) {
            Ok(parts) => parts,
            Err(error) => return self.warn_location(package, location.file_id, error),
        };
        self.record_culling_count(location.file_id, body);
        self.missing.remove(&location.file_id);
        self.found.insert(location.file_id, parts);
        self.packages_by_unit
            .insert(location.file_id, package.to_owned());
    }

    fn record_culling_count(&mut self, file_id: u64, body: &[u8]) {
        if !self.collect_culling_counts {
            return;
        }
        if let Ok(inspection) = inspect_unit_culling(body) {
            self.culling_counts
                .insert(file_id, inspection.culling_meshes.len());
        }
    }

    fn warn_location(&mut self, package: &str, file_id: u64, error: eyre::Report) {
        self.warnings
            .push(format!("{package}/{file_id:016x}: {error}"));
    }
}

fn notify_scan_progress(progress: Option<&dyn WebProgress>, package: &str) -> crate::Result<()> {
    let Some(progress) = progress else {
        return Ok(());
    };
    progress.stage("", &format!("scanning {package}"))
}

#[cfg(test)]
mod progress_tests {
    use super::*;

    #[test]
    fn omitted_culling_policy_defaults_to_patch() {
        let options: UnitRepatchOptions = serde_json::from_value(serde_json::json!({
            "missingUnitPolicy": "keep"
        }))
        .expect("deserialize legacy repatch options");

        assert_eq!(options.culling_policy, CullingPolicy::Patch);
    }

    struct CancelledProgress;

    impl WebProgress for CancelledProgress {
        fn target_started(&self, _name: &str, _hash: &str) -> crate::Result<()> {
            Ok(())
        }

        fn stage(&self, _name: &str, _stage: &str) -> crate::Result<()> {
            eyre::bail!("task cancelled")
        }

        fn target_finished(&self, _name: &str) -> crate::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn scan_progress_can_cancel_repatching() {
        let error = notify_scan_progress(Some(&CancelledProgress), "example.patch_0")
            .expect_err("cancel scan");

        assert!(error.to_string().contains("task cancelled"));
    }
}

fn apply_latest_units(
    patch: &mut TocOnlyPackage,
    lookup: LatestUnitLookup,
    policy: MissingUnitPolicy,
) -> UnitRepatchSummary {
    let unit_count = patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .count();
    let mut summary = UnitRepatchSummary {
        unit_count,
        updated_units: 0,
        already_current_units: 0,
        removed_units: 0,
        failed_units: 0,
        scanned_archives: lookup.scanned_archives,
        warnings: lookup.warnings,
    };
    patch.entries.retain_mut(|entry| {
        update_patch_entry(entry, &lookup.found, &lookup.missing, policy, &mut summary)
    });
    summary
}

fn update_patch_entry(
    entry: &mut crate::archive::toc_only::TocOnlyEntry,
    latest: &HashMap<u64, LatestUnitParts>,
    missing: &HashSet<u64>,
    policy: MissingUnitPolicy,
    summary: &mut UnitRepatchSummary,
) -> bool {
    if entry.type_id != UNIT_ID {
        return true;
    }
    if missing.contains(&entry.file_id) {
        return handle_missing_entry(entry.file_id, policy, summary);
    }
    let Some(parts) = latest.get(&entry.file_id) else {
        return true;
    };
    match repatch_unit(&mut entry.toc_data, parts) {
        Ok(RepatchOutcome::Updated { .. }) => summary.updated_units += 1,
        Ok(RepatchOutcome::AlreadyCurrent) => summary.already_current_units += 1,
        Err(error) => {
            summary.failed_units += 1;
            summary
                .warnings
                .push(format!("{:016x}: {error}", entry.file_id));
        }
    }
    true
}

fn handle_missing_entry(
    file_id: u64,
    policy: MissingUnitPolicy,
    summary: &mut UnitRepatchSummary,
) -> bool {
    if matches!(policy, MissingUnitPolicy::Drop) {
        summary.removed_units += 1;
        summary
            .warnings
            .push(format!("removed missing Unit {file_id:016x}"));
        return false;
    }
    summary
        .warnings
        .push(format!("kept missing Unit {file_id:016x}"));
    true
}

fn enforce_missing_policy(missing: &HashSet<u64>, policy: MissingUnitPolicy) -> crate::Result<()> {
    if missing.is_empty() || !matches!(policy, MissingUnitPolicy::Fail) {
        return Ok(());
    }
    let mut ids = missing
        .iter()
        .map(|id| format!("{id:016x}"))
        .collect::<Vec<_>>();
    ids.sort();
    eyre::bail!(
        "{} Unit(s) are absent from current game data: {}",
        ids.len(),
        ids.join(", ")
    )
}

fn patch_unit_ids(patch: &TocOnlyPackage) -> HashSet<u64> {
    patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .map(|entry| entry.file_id)
        .collect()
}

fn location_range(locations: &[TocEntryLocation]) -> crate::Result<(u64, u64)> {
    let start = locations
        .iter()
        .map(|entry| entry.toc_offset)
        .min()
        .unwrap_or(0);
    let end = locations
        .iter()
        .map(|entry| entry.toc_offset + u64::from(entry.toc_size))
        .max()
        .unwrap_or(start);
    Ok((
        start,
        end.checked_sub(start)
            .ok_or_else(|| eyre::eyre!("Unit range overflow"))?,
    ))
}

fn location_body<'a>(
    location: &TocEntryLocation,
    range_start: u64,
    data: &'a [u8],
) -> crate::Result<&'a [u8]> {
    let start = usize::try_from(location.toc_offset - range_start)?;
    let end = start
        .checked_add(location.toc_size as usize)
        .ok_or_else(|| eyre::eyre!("Unit body range overflow"))?;
    data.get(start..end)
        .ok_or_else(|| eyre::eyre!("Unit body is out of bounds"))
}

fn prioritize(mut packages: Vec<String>, preferred: Option<&str>) -> Vec<String> {
    packages.sort();
    if let Some(index) = preferred.and_then(|name| packages.iter().position(|item| item == name)) {
        packages.swap(0, index);
    }
    packages
}

fn archive_prefix(name: &str) -> Option<String> {
    let prefix = name.get(..16)?;
    is_archive_name(prefix).then(|| prefix.to_ascii_lowercase())
}

fn archive_basename(name: &str) -> Option<String> {
    let basename = name.rsplit(['/', '\\']).next()?;
    is_archive_name(basename).then(|| basename.to_ascii_lowercase())
}

fn is_archive_name(name: &str) -> bool {
    name.len() == 16 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn package_magic(data: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(..4)?.try_into().ok()?))
}
