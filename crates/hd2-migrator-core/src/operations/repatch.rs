//! Browser-compatible Unit repatching over an async game-data source.

use super::migration::PatchBytes;
use crate::archive::dsar;
use crate::archive::toc_only::{
    TocEntryLocation, TocHeader, TocOnlyPackage, parse_entry_locations, retain_locations,
};
use crate::constants::{DSAR_MAGIC, UNIT_ID};
use crate::io::{BundleSlicer, DataSource};
use crate::migrator::mode_a_web::WebProgress;
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitRepatchSummary {
    pub unit_count: usize,
    pub updated_units: usize,
    pub converted_formats: usize,
    pub refreshed_lod_groups: usize,
    pub already_current_units: usize,
    pub removed_units: usize,
    pub scanned_archives: usize,
    pub warnings: Vec<String>,
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
    pub original_toc: Vec<u8>,
    pub summary: UnitRepatchSummary,
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
    let mut patch = TocOnlyPackage::parse(patch_toc).wrap_err("parse patch TOC")?;
    validate_repatch_package(&patch)?;
    let wanted = patch_unit_ids(&patch);
    if wanted.is_empty() {
        return Ok(UnitRepatchPlan {
            patch: None,
            original_toc: patch_toc.to_vec(),
            summary: no_units_summary(),
        });
    }
    let mut lookup = LatestUnitLookup::new(wanted, patch_name);
    lookup.load(source, progress).await?;
    enforce_missing_policy(&lookup.missing, options.missing_unit_policy)?;
    let summary = apply_latest_units(&mut patch, lookup, options.missing_unit_policy)?;
    let changed = summary.updated_units > 0 || summary.removed_units > 0;
    Ok(UnitRepatchPlan {
        patch: changed.then_some(patch),
        original_toc: patch_toc.to_vec(),
        summary,
    })
}

/// Update legacy Unit TOC metadata while leaving sidecars and all other Unit data untouched.
pub async fn repatch_patch_with_progress<S: DataSource + ?Sized>(
    patch: PatchBytes,
    options: UnitRepatchOptions,
    source: &S,
    progress: Option<&dyn WebProgress>,
) -> crate::Result<UnitRepatchResult> {
    let plan = repatch_patch_plan_with_progress(patch, options, source, progress).await?;
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
    repatch_units_plan_with_progress(&patch.name, &patch.toc, options, source, progress).await
}

fn no_units_summary() -> UnitRepatchSummary {
    UnitRepatchSummary {
        unit_count: 0,
        updated_units: 0,
        converted_formats: 0,
        refreshed_lod_groups: 0,
        already_current_units: 0,
        removed_units: 0,
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
}

impl LatestUnitLookup {
    fn new(wanted: HashSet<u64>, patch_name: &str) -> Self {
        Self {
            missing: wanted.clone(),
            wanted,
            found: HashMap::new(),
            preferred_archive: archive_prefix(patch_name),
            scanned_archives: 0,
            warnings: Vec::new(),
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
        self.missing.remove(&location.file_id);
        self.found.insert(location.file_id, parts);
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
) -> crate::Result<UnitRepatchSummary> {
    let unit_count = patch
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .count();
    let mut summary = UnitRepatchSummary {
        unit_count,
        updated_units: 0,
        converted_formats: 0,
        refreshed_lod_groups: 0,
        already_current_units: 0,
        removed_units: 0,
        scanned_archives: lookup.scanned_archives,
        warnings: lookup.warnings,
    };
    let context = RepatchEntryContext {
        latest: &lookup.found,
        missing: &lookup.missing,
        policy,
    };
    let entries = std::mem::take(&mut patch.entries);
    for mut entry in entries {
        if update_patch_entry(&mut entry, &context, &mut summary)? {
            patch.entries.push(entry);
        }
    }
    Ok(summary)
}

struct RepatchEntryContext<'a> {
    latest: &'a HashMap<u64, LatestUnitParts>,
    missing: &'a HashSet<u64>,
    policy: MissingUnitPolicy,
}

fn update_patch_entry(
    entry: &mut crate::archive::toc_only::TocOnlyEntry,
    context: &RepatchEntryContext<'_>,
    summary: &mut UnitRepatchSummary,
) -> crate::Result<bool> {
    if entry.type_id != UNIT_ID {
        return Ok(true);
    }
    if context.missing.contains(&entry.file_id) {
        return Ok(handle_missing_entry(entry.file_id, context.policy, summary));
    }
    let Some(parts) = context.latest.get(&entry.file_id) else {
        return Ok(true);
    };
    match repatch_unit(&mut entry.toc_data, parts)
        .wrap_err_with(|| format!("update Unit {:016x}", entry.file_id))?
    {
        RepatchOutcome::Updated {
            converted_formats,
            refreshed_lod_group,
        } => {
            summary.updated_units += 1;
            summary.converted_formats += converted_formats;
            summary.refreshed_lod_groups += usize::from(refreshed_lod_group);
        }
        RepatchOutcome::AlreadyCurrent => summary.already_current_units += 1,
    }
    Ok(true)
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

fn validate_repatch_package(patch: &TocOnlyPackage) -> crate::Result<()> {
    validate_resource_type_ids(patch)?;
    validate_resource_count(patch)?;
    validate_unique_resources(patch)
}

fn validate_resource_type_ids(patch: &TocOnlyPackage) -> crate::Result<()> {
    if let Some(file_type) = patch
        .types
        .iter()
        .find(|file_type| file_type.type_id < 1 << 32)
    {
        eyre::bail!("invalid resource type ID 0x{:016x}", file_type.type_id);
    }
    Ok(())
}

fn validate_resource_count(patch: &TocOnlyPackage) -> crate::Result<()> {
    let declared_files = patch.types.iter().try_fold(0usize, |total, file_type| {
        total
            .checked_add(file_type.num_files as usize)
            .ok_or_else(|| eyre::eyre!("resource count overflow"))
    })?;
    if declared_files != patch.entries.len() {
        eyre::bail!(
            "resource count mismatch: type table declares {declared_files}, header contains {}",
            patch.entries.len()
        );
    }
    Ok(())
}

fn validate_unique_resources(patch: &TocOnlyPackage) -> crate::Result<()> {
    let mut resources = HashSet::new();
    for entry in &patch.entries {
        if !resources.insert((entry.type_id, entry.file_id)) {
            eyre::bail!(
                "duplicate resource {:016x}/{:016x}",
                entry.type_id,
                entry.file_id
            );
        }
    }
    Ok(())
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

#[cfg(test)]
mod behavior_tests {
    use super::*;
    use crate::archive::TocFileType;
    use crate::archive::toc_only::TocOnlyEntry;

    #[test]
    fn drop_policy_removes_only_missing_units() {
        let missing_unit_id = 0x11;
        let mut patch =
            package_with_entries(vec![entry(missing_unit_id, UNIT_ID), entry(0x22, 0x1234)]);
        let lookup = missing_lookup(missing_unit_id);

        let summary = apply_latest_units(&mut patch, lookup, MissingUnitPolicy::Drop).unwrap();

        assert_eq!(summary.unit_count, 1);
        assert_eq!(summary.removed_units, 1);
        assert_eq!(patch.entries.len(), 1);
        assert_eq!(patch.entries[0].type_id, 0x1234);
    }

    #[test]
    fn keep_policy_preserves_missing_units_with_warning() {
        let missing_unit_id = 0x11;
        let mut patch = package_with_entries(vec![entry(missing_unit_id, UNIT_ID)]);
        let lookup = missing_lookup(missing_unit_id);

        let summary = apply_latest_units(&mut patch, lookup, MissingUnitPolicy::Keep).unwrap();

        assert_eq!(summary.removed_units, 0);
        assert_eq!(patch.entries.len(), 1);
        assert!(summary.warnings[0].contains("kept missing Unit"));
    }

    #[test]
    fn fail_policy_lists_missing_units_in_stable_order() {
        let missing = HashSet::from([0x22, 0x11]);

        let error = enforce_missing_policy(&missing, MissingUnitPolicy::Fail).unwrap_err();

        assert!(
            error
                .to_string()
                .ends_with("0000000000000011, 0000000000000022")
        );
    }

    #[test]
    fn repatch_validation_rejects_corrupt_type_counts() {
        let mut patch = package_with_entries(vec![entry(0x11, UNIT_ID)]);
        patch.types = vec![TocFileType::new(UNIT_ID, 2)];

        let error = validate_repatch_package(&patch).unwrap_err();

        assert!(error.to_string().contains("resource count mismatch"));
    }

    #[test]
    fn repatch_validation_rejects_duplicate_resources() {
        let mut patch = package_with_entries(vec![entry(0x11, UNIT_ID), entry(0x11, UNIT_ID)]);
        patch.types = vec![TocFileType::new(UNIT_ID, 2)];

        let error = validate_repatch_package(&patch).unwrap_err();

        assert!(error.to_string().contains("duplicate resource"));
    }

    fn package_with_entries(entries: Vec<TocOnlyEntry>) -> TocOnlyPackage {
        TocOnlyPackage {
            types: Vec::new(),
            entries,
            unknown: 0,
            unk4_data: [0; 56],
        }
    }

    fn entry(file_id: u64, type_id: u64) -> TocOnlyEntry {
        TocOnlyEntry {
            file_id,
            type_id,
            unknown1: 0,
            unknown2: 0,
            unknown3: 0,
            unknown4: 0,
            toc_data: Vec::new(),
            stream_offset: 0,
            gpu_offset: 0,
            stream_size: 0,
            gpu_size: 0,
        }
    }

    fn missing_lookup(file_id: u64) -> LatestUnitLookup {
        LatestUnitLookup {
            wanted: HashSet::from([file_id]),
            found: HashMap::new(),
            missing: HashSet::from([file_id]),
            preferred_archive: None,
            scanned_archives: 1,
            warnings: Vec::new(),
        }
    }
}
