//! Unit repatching behavior was inspired by hd2-repatcher, created by Evie / RaidingForPants.
//! This project provides an independent Rust/WASM implementation.

use byteorder::{ByteOrder, LittleEndian as LE};

const VERSION_OFFSET: usize = 0x2c;
const LOD_GROUP_OFFSET_FIELD: usize = 0x30;
const OFFSET_TABLE_START: usize = 0x34;
const OFFSET_TABLE_COUNT: usize = 16;
const LAYOUT_LIST_OFFSET_FIELD: usize = 0x5c;
const LEGACY_STREAM_FORMAT_VERSION: u32 = 10_800_437;
const CURRENT_STREAM_FORMAT_VERSION: u32 = 10_800_438;
const STREAM_COMPONENT_CAPACITY: usize = 16;
const STREAM_COMPONENTS_OFFSET: usize = 8;
const STREAM_COMPONENT_SIZE: usize = 20;
const STREAM_COMPONENT_FORMAT_OFFSET: usize = 4;
const STREAM_NUM_COMPONENTS_OFFSET: usize =
    STREAM_COMPONENTS_OFFSET + STREAM_COMPONENT_CAPACITY * STREAM_COMPONENT_SIZE;
const STREAM_NUM_COMPONENTS_SIZE: usize = 8;

#[derive(Debug, Clone)]
pub struct LatestUnitParts {
    version: u32,
    lod_group: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepatchOutcome {
    Updated {
        converted_formats: usize,
        refreshed_lod_group: bool,
    },
    AlreadyCurrent,
}

impl LatestUnitParts {
    pub fn parse(unit: &[u8]) -> crate::Result<Self> {
        require_range(unit, VERSION_OFFSET, 12, "latest Unit header")?;
        let (lod_start, lod_end) = lod_group_range(unit, "latest Unit")?;
        Ok(Self {
            version: LE::read_u32(&unit[VERSION_OFFSET..]),
            lod_group: unit[lod_start..lod_end].to_vec(),
        })
    }
}

/// Apply version-dependent Unit updates from the original repatcher safely.
///
/// The verified 10800437 -> 10800438 transition preserves the Mod's LOD group.
/// Other forward transitions retain the original repatcher's LOD refresh.
pub fn repatch_unit(unit: &mut Vec<u8>, latest: &LatestUnitParts) -> crate::Result<RepatchOutcome> {
    require_range(unit, VERSION_OFFSET, 12, "mod Unit header")?;
    let version = LE::read_u32(&unit[VERSION_OFFSET..]);
    validate_forward_transition(version, latest.version)?;
    let mut updated = unit.clone();
    let converted_formats = update_formats_for_transition(&mut updated, version, latest.version)?;
    let refreshed_lod_group =
        refresh_lod_for_transition(&mut updated, version, latest.version, &latest.lod_group)?;
    if version == latest.version && !refreshed_lod_group {
        return Ok(RepatchOutcome::AlreadyCurrent);
    }
    LE::write_u32(&mut updated[VERSION_OFFSET..], latest.version);
    *unit = updated;
    Ok(RepatchOutcome::Updated {
        converted_formats,
        refreshed_lod_group,
    })
}

fn validate_forward_transition(version: u32, latest_version: u32) -> crate::Result<()> {
    if version <= latest_version {
        return Ok(());
    }
    eyre::bail!("refusing to downgrade Unit version {version} -> {latest_version}")
}

fn update_formats_for_transition(
    unit: &mut [u8],
    version: u32,
    latest_version: u32,
) -> crate::Result<usize> {
    if version >= CURRENT_STREAM_FORMAT_VERSION || latest_version < CURRENT_STREAM_FORMAT_VERSION {
        return Ok(0);
    }
    update_legacy_stream_formats(unit)
}

fn refresh_lod_for_transition(
    unit: &mut Vec<u8>,
    version: u32,
    latest_version: u32,
    latest_lod_group: &[u8],
) -> crate::Result<bool> {
    if version >= LEGACY_STREAM_FORMAT_VERSION && latest_version == CURRENT_STREAM_FORMAT_VERSION {
        return Ok(false);
    }
    replace_lod_group(unit, latest_lod_group)
}

fn replace_lod_group(unit: &mut Vec<u8>, latest_lod_group: &[u8]) -> crate::Result<bool> {
    require_range(
        unit,
        OFFSET_TABLE_START,
        OFFSET_TABLE_COUNT * 4,
        "mod Unit offset table",
    )?;
    let (lod_start, lod_end) = lod_group_range(unit, "mod Unit")?;
    if unit[lod_start..lod_end] == *latest_lod_group {
        return Ok(false);
    }
    let size_delta = latest_lod_group.len() as i64 - (lod_end - lod_start) as i64;
    adjust_offsets_after_lod(unit, lod_start as u32, size_delta)?;
    unit.splice(lod_start..lod_end, latest_lod_group.iter().copied());
    Ok(true)
}

fn adjust_offsets_after_lod(unit: &mut [u8], lod_start: u32, delta: i64) -> crate::Result<()> {
    if delta == 0 {
        return Ok(());
    }
    for index in 0..OFFSET_TABLE_COUNT {
        let start = OFFSET_TABLE_START + index * 4;
        let offset = LE::read_u32(&unit[start..]);
        if offset == 0 || offset <= lod_start {
            continue;
        }
        let adjusted = u32::try_from(i64::from(offset) + delta)
            .map_err(|_| eyre::eyre!("Unit offset adjustment overflow"))?;
        LE::write_u32(&mut unit[start..], adjusted);
    }
    Ok(())
}

fn lod_group_range(unit: &[u8], label: &str) -> crate::Result<(usize, usize)> {
    let start = LE::read_u32(&unit[LOD_GROUP_OFFSET_FIELD..]) as usize;
    let end = LE::read_u32(&unit[LOD_GROUP_OFFSET_FIELD + 4..]) as usize;
    if end < start {
        eyre::bail!("{label} has a reversed LOD group range");
    }
    require_range(unit, start, end - start, &format!("{label} LOD group"))?;
    Ok((start, end))
}

fn update_legacy_stream_formats(unit: &mut [u8]) -> crate::Result<usize> {
    require_range(unit, LAYOUT_LIST_OFFSET_FIELD, 4, "layout list pointer")?;
    let list_start = LE::read_u32(&unit[LAYOUT_LIST_OFFSET_FIELD..]) as usize;
    if list_start == 0 {
        return Ok(0);
    }
    require_range(unit, list_start, 4, "layout list")?;
    let layout_count = LE::read_u32(&unit[list_start..]) as usize;
    let offsets_start = list_start
        .checked_add(4)
        .ok_or_else(|| eyre::eyre!("layout offset table overflow"))?;
    let offsets_size = layout_count
        .checked_mul(4)
        .ok_or_else(|| eyre::eyre!("layout offset table size overflow"))?;
    require_range(unit, offsets_start, offsets_size, "layout offsets")?;
    let mut converted_formats = 0;
    for index in 0..layout_count {
        converted_formats += update_layout_formats(unit, list_start, offsets_start + index * 4)?;
    }
    Ok(converted_formats)
}

fn update_layout_formats(
    unit: &mut [u8],
    list_start: usize,
    offset_field: usize,
) -> crate::Result<usize> {
    let (components_start, component_count) =
        stream_component_layout(unit, list_start, offset_field)?;
    let mut converted_formats = 0;
    for component_index in 0..component_count {
        converted_formats += update_component_format(unit, components_start, component_index)?;
    }
    Ok(converted_formats)
}

fn stream_component_layout(
    unit: &[u8],
    list_start: usize,
    offset_field: usize,
) -> crate::Result<(usize, usize)> {
    let relative = LE::read_u32(&unit[offset_field..]) as usize;
    let record_start = list_start
        .checked_add(relative)
        .ok_or_else(|| eyre::eyre!("layout offset overflow"))?;
    let components_start = record_start
        .checked_add(STREAM_COMPONENTS_OFFSET)
        .ok_or_else(|| eyre::eyre!("stream component offset overflow"))?;
    require_range(
        unit,
        components_start,
        STREAM_COMPONENT_CAPACITY * STREAM_COMPONENT_SIZE,
        "stream components",
    )?;
    let count_offset = record_start
        .checked_add(STREAM_NUM_COMPONENTS_OFFSET)
        .ok_or_else(|| eyre::eyre!("stream component count offset overflow"))?;
    require_range(
        unit,
        count_offset,
        STREAM_NUM_COMPONENTS_SIZE,
        "stream component count",
    )?;
    let component_count = LE::read_u64(&unit[count_offset..]);
    if component_count > STREAM_COMPONENT_CAPACITY as u64 {
        eyre::bail!(
            "stream component count exceeds {STREAM_COMPONENT_CAPACITY}: {component_count}"
        );
    }
    Ok((components_start, component_count as usize))
}

fn update_component_format(
    unit: &mut [u8],
    components_start: usize,
    component_index: usize,
) -> crate::Result<usize> {
    let format_offset =
        components_start + component_index * STREAM_COMPONENT_SIZE + STREAM_COMPONENT_FORMAT_OFFSET;
    let format = LE::read_u32(&unit[format_offset..]);
    let Some(updated) = upgraded_stream_format(format) else {
        if matches!(format, 0..=4) {
            return Ok(0);
        }
        eyre::bail!(
            "unsupported legacy stream component format {format} at component {component_index}"
        );
    };
    if updated == format {
        return Ok(0);
    }
    LE::write_u32(&mut unit[format_offset..], updated);
    Ok(1)
}

fn upgraded_stream_format(format: u32) -> Option<u32> {
    match format {
        20 => Some(24),
        24 => Some(28),
        25 => Some(29),
        26 => Some(30),
        29 => Some(33),
        31 => Some(35),
        _ => None,
    }
}

fn require_range(data: &[u8], start: usize, len: usize, label: &str) -> crate::Result<()> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| eyre::eyre!("{label} range overflow"))?;
    if end > data.len() {
        eyre::bail!("{label} is out of bounds: {start}..{end} > {}", data.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_current_is_unchanged() {
        let bytes = old_unit_with_layout(CURRENT_STREAM_FORMAT_VERSION, &[28, 30]);
        let latest = LatestUnitParts::parse(&bytes).expect("latest parts");
        let mut unit = bytes.clone();
        assert_eq!(
            repatch_unit(&mut unit, &latest).unwrap(),
            RepatchOutcome::AlreadyCurrent
        );
        assert_eq!(unit, bytes);
    }

    #[test]
    fn upgrades_only_verified_formats_and_preserves_reserved_slots() {
        let latest = latest_unit(CURRENT_STREAM_FORMAT_VERSION);
        let latest = LatestUnitParts::parse(&latest).expect("latest parts");
        let mut old = old_unit_with_layout(
            LEGACY_STREAM_FORMAT_VERSION,
            &[0, 1, 2, 3, 4, 20, 24, 25, 26, 29, 31],
        );
        let reserved_offset = component_format_offset(15);
        LE::write_u32(&mut old[reserved_offset..], 31);

        let outcome = repatch_unit(&mut old, &latest).expect("legacy repatch");

        assert_eq!(
            outcome,
            RepatchOutcome::Updated {
                converted_formats: 6,
                refreshed_lod_group: false,
            }
        );
        assert_eq!(
            LE::read_u32(&old[VERSION_OFFSET..]),
            CURRENT_STREAM_FORMAT_VERSION
        );
        assert_eq!(
            active_formats(&old, 11),
            vec![0, 1, 2, 3, 4, 24, 28, 29, 30, 33, 35]
        );
        assert_eq!(LE::read_u32(&old[reserved_offset..]), 31);
    }

    #[test]
    fn rejects_stream_component_count_above_capacity() {
        let latest = latest_unit(CURRENT_STREAM_FORMAT_VERSION);
        let latest = LatestUnitParts::parse(&latest).expect("latest parts");
        let mut old = old_unit_with_layout(LEGACY_STREAM_FORMAT_VERSION, &[]);
        LE::write_u64(&mut old[0x1d8..], 17);

        let error = repatch_unit(&mut old, &latest).expect_err("reject invalid component count");

        assert!(
            error
                .to_string()
                .contains("stream component count exceeds 16")
        );
    }

    #[test]
    fn rejects_unknown_legacy_format_without_updating_version() {
        let latest = LatestUnitParts::parse(&latest_unit(CURRENT_STREAM_FORMAT_VERSION)).unwrap();
        let mut old = old_unit_with_layout(LEGACY_STREAM_FORMAT_VERSION, &[20, 17]);
        let original = old.clone();

        let error = repatch_unit(&mut old, &latest).expect_err("reject unknown format");

        assert!(
            error
                .to_string()
                .contains("unsupported legacy stream component format 17")
        );
        assert_eq!(old, original);
    }

    #[test]
    fn rejects_version_downgrade_without_mutating_unit() {
        let latest = LatestUnitParts::parse(&latest_unit(CURRENT_STREAM_FORMAT_VERSION)).unwrap();
        let mut old = latest_unit(CURRENT_STREAM_FORMAT_VERSION + 1);
        let original = old.clone();

        let error = repatch_unit(&mut old, &latest).expect_err("reject downgrade");

        assert!(
            error
                .to_string()
                .contains("refusing to downgrade Unit version")
        );
        assert_eq!(old, original);
    }

    #[test]
    fn upgrades_version_when_unit_has_no_stream_table() {
        let latest = LatestUnitParts::parse(&latest_unit(CURRENT_STREAM_FORMAT_VERSION)).unwrap();
        let mut old = latest_unit(LEGACY_STREAM_FORMAT_VERSION);

        let outcome = repatch_unit(&mut old, &latest).expect("upgrade header-only Unit");

        assert_eq!(
            outcome,
            RepatchOutcome::Updated {
                converted_formats: 0,
                refreshed_lod_group: false,
            }
        );
        assert_eq!(
            LE::read_u32(&old[VERSION_OFFSET..]),
            CURRENT_STREAM_FORMAT_VERSION
        );
    }

    #[test]
    fn verified_stream_upgrade_preserves_mod_lod_group() {
        let latest = unit_with_lod(CURRENT_STREAM_FORMAT_VERSION, &[9; 16], 0xa0);
        let latest = LatestUnitParts::parse(&latest).expect("latest parts");
        let mut old = old_unit_with_layout(LEGACY_STREAM_FORMAT_VERSION, &[20]);
        set_lod_group(&mut old, 0x220, &[1; 8]);

        let outcome = repatch_unit(&mut old, &latest).expect("verified upgrade");

        assert_eq!(
            outcome,
            RepatchOutcome::Updated {
                converted_formats: 1,
                refreshed_lod_group: false,
            }
        );
        assert_eq!(&old[0x220..0x228], &[1; 8]);
    }

    #[test]
    fn later_version_refreshes_lod_group_and_adjusts_offsets() {
        let latest = unit_with_lod(CURRENT_STREAM_FORMAT_VERSION + 1, &[9; 16], 0xa0);
        let latest = LatestUnitParts::parse(&latest).expect("latest parts");
        let mut old = unit_with_lod(CURRENT_STREAM_FORMAT_VERSION, &[1; 8], 0xb0);

        let outcome = repatch_unit(&mut old, &latest).expect("original repatcher update");

        assert_eq!(
            outcome,
            RepatchOutcome::Updated {
                converted_formats: 0,
                refreshed_lod_group: true,
            }
        );
        assert_eq!(
            LE::read_u32(&old[VERSION_OFFSET..]),
            CURRENT_STREAM_FORMAT_VERSION + 1
        );
        assert_eq!(LE::read_u32(&old[LOD_GROUP_OFFSET_FIELD + 4..]), 0xb0);
        assert_eq!(LE::read_u32(&old[LOD_GROUP_OFFSET_FIELD + 8..]), 0xb8);
        assert_eq!(&old[0xa0..0xb0], &[9; 16]);
    }

    #[test]
    fn current_later_version_refreshes_stale_lod_group() {
        let version = CURRENT_STREAM_FORMAT_VERSION + 1;
        let latest = LatestUnitParts::parse(&unit_with_lod(version, &[9; 8], 0x90)).unwrap();
        let mut old = unit_with_lod(version, &[1; 8], 0x90);

        let outcome = repatch_unit(&mut old, &latest).expect("refresh stale LOD");

        assert_eq!(
            outcome,
            RepatchOutcome::Updated {
                converted_formats: 0,
                refreshed_lod_group: true,
            }
        );
        assert_eq!(&old[0xa0..0xa8], &[9; 8]);
    }

    fn old_unit_with_layout(version: u32, formats: &[u32]) -> Vec<u8> {
        let mut unit = vec![0u8; 0x280];
        LE::write_u32(&mut unit[VERSION_OFFSET..], version);
        LE::write_u32(&mut unit[LAYOUT_LIST_OFFSET_FIELD..], 0x80);
        LE::write_u32(&mut unit[0x80..], 1);
        LE::write_u32(&mut unit[0x84..], 0x10);
        LE::write_u64(&mut unit[0x1d8..], formats.len() as u64);
        for (index, format) in formats.iter().enumerate() {
            LE::write_u32(&mut unit[component_format_offset(index)..], *format);
        }
        unit
    }

    fn component_format_offset(index: usize) -> usize {
        0x98 + index * STREAM_COMPONENT_SIZE + STREAM_COMPONENT_FORMAT_OFFSET
    }

    fn active_formats(unit: &[u8], count: usize) -> Vec<u32> {
        (0..count)
            .map(|index| LE::read_u32(&unit[component_format_offset(index)..]))
            .collect()
    }

    fn latest_unit(version: u32) -> Vec<u8> {
        let mut unit = vec![0u8; LAYOUT_LIST_OFFSET_FIELD + 4];
        LE::write_u32(&mut unit[VERSION_OFFSET..], version);
        unit
    }

    fn unit_with_lod(version: u32, lod: &[u8], following_offset: u32) -> Vec<u8> {
        let mut unit = vec![0u8; 0x120];
        LE::write_u32(&mut unit[VERSION_OFFSET..], version);
        set_lod_group(&mut unit, 0xa0, lod);
        LE::write_u32(&mut unit[LOD_GROUP_OFFSET_FIELD + 8..], following_offset);
        unit
    }

    fn set_lod_group(unit: &mut [u8], start: usize, lod: &[u8]) {
        let end = start + lod.len();
        LE::write_u32(&mut unit[LOD_GROUP_OFFSET_FIELD..], start as u32);
        LE::write_u32(&mut unit[LOD_GROUP_OFFSET_FIELD + 4..], end as u32);
        unit[start..end].copy_from_slice(lod);
    }
}
