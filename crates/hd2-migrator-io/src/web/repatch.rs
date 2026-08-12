//! Browser-compatible Unit repatching over an async game-data source.

use crate::archive::dsar;
use crate::archive::toc_only::{
    TocEntryLocation, TocHeader, TocOnlyPackage, parse_entry_locations, retain_locations,
};
use crate::constants::{DSAR_MAGIC, UNIT_ID};
use crate::io::{BundleSlicer, DataSource};
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
    pub already_current_units: usize,
    pub removed_units: usize,
    pub failed_units: usize,
    pub scanned_archives: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UnitRepatchResult {
    pub toc: Vec<u8>,
    pub summary: UnitRepatchSummary,
}

pub async fn repatch_units<S: DataSource + ?Sized>(
    patch_name: &str,
    patch_toc: &[u8],
    options: UnitRepatchOptions,
    source: &S,
) -> crate::Result<UnitRepatchResult> {
    let mut patch = TocOnlyPackage::parse(patch_toc).wrap_err("parse patch TOC")?;
    let wanted = patch_unit_ids(&patch);
    if wanted.is_empty() {
        return Ok(no_units_result(patch_toc));
    }
    let mut lookup = LatestUnitLookup::new(wanted, patch_name);
    lookup.load(source).await?;
    enforce_missing_policy(&lookup.missing, options.missing_unit_policy)?;
    let summary = apply_latest_units(&mut patch, lookup, options.missing_unit_policy);
    let toc = serialize_if_changed(&patch, patch_toc, &summary)?;
    Ok(UnitRepatchResult { toc, summary })
}

fn serialize_if_changed(
    patch: &TocOnlyPackage,
    original: &[u8],
    summary: &UnitRepatchSummary,
) -> crate::Result<Vec<u8>> {
    if summary.updated_units == 0 && summary.removed_units == 0 {
        return Ok(original.to_vec());
    }
    patch.serialize().wrap_err("serialize updated patch TOC")
}

fn no_units_result(patch_toc: &[u8]) -> UnitRepatchResult {
    UnitRepatchResult {
        toc: patch_toc.to_vec(),
        summary: UnitRepatchSummary {
            unit_count: 0,
            updated_units: 0,
            already_current_units: 0,
            removed_units: 0,
            failed_units: 0,
            scanned_archives: 0,
            warnings: vec!["patch contains no Unit resources".to_string()],
        },
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

    async fn load<S: DataSource + ?Sized>(&mut self, source: &S) -> crate::Result<()> {
        if self.wanted.is_empty() {
            return Ok(());
        }
        if source.exists("bundles.nxa").await? {
            self.load_bundled(source).await
        } else {
            self.load_legacy(source).await
        }
    }

    async fn load_legacy<S: DataSource + ?Sized>(&mut self, source: &S) -> crate::Result<()> {
        let packages = source
            .list_packages()
            .await
            .wrap_err("list game archives")?;
        for package in prioritize(packages, self.preferred_archive.as_deref()) {
            self.load_legacy_package(source, &package).await?;
            if self.missing.is_empty() {
                break;
            }
        }
        Ok(())
    }

    async fn load_bundled<S: DataSource + ?Sized>(&mut self, source: &S) -> crate::Result<()> {
        let slicer = BundleSlicer::open(source).await?;
        let packages = slicer
            .packages
            .keys()
            .filter_map(|name| archive_basename(name))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        for package in prioritize(packages, self.preferred_archive.as_deref()) {
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
            match location_body(location, range_start, data).and_then(LatestUnitParts::parse) {
                Ok(parts) => {
                    self.missing.remove(&location.file_id);
                    self.found.insert(location.file_id, parts);
                }
                Err(error) => self
                    .warnings
                    .push(format!("{package}/{:016x}: {error}", location.file_id)),
            }
        }
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
