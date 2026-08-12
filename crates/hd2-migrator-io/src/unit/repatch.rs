//! Unit repatching behavior was inspired by hd2-repatcher, created by Evie / RaidingForPants.
//! This project provides an independent Rust/WASM implementation.

use byteorder::{ByteOrder, LittleEndian as LE};

const VERSION_OFFSET: usize = 0x2c;
const LOD_GROUP_OFFSET_FIELD: usize = 0x30;
const OFFSET_TABLE_START: usize = 0x34;
const OFFSET_TABLE_COUNT: usize = 16;
const LAYOUT_LIST_OFFSET_FIELD: usize = 0x5c;
const FORMAT_LAYOUT_VERSION: u32 = 0x00a4_cd36;
const ITEMS_PER_LAYOUT: usize = 16;
const LAYOUT_HEADER_SIZE: usize = 8;
const LAYOUT_ITEM_SIZE: usize = 20;
const ITEM_FORMAT_OFFSET: usize = 4;

#[derive(Debug, Clone)]
pub struct LatestUnitParts {
    version: [u8; 4],
    lod_group: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepatchOutcome {
    Updated { size_delta: i64 },
    AlreadyCurrent,
}

impl LatestUnitParts {
    pub fn parse(unit: &[u8]) -> crate::Result<Self> {
        require_range(unit, VERSION_OFFSET, 12, "latest Unit header")?;
        let lod_start = LE::read_u32(&unit[LOD_GROUP_OFFSET_FIELD..]) as usize;
        let lod_end = LE::read_u32(&unit[LOD_GROUP_OFFSET_FIELD + 4..]) as usize;
        if lod_end < lod_start {
            eyre::bail!("latest Unit has a reversed LOD group range");
        }
        let lod_group = unit
            .get(lod_start..lod_end)
            .ok_or_else(|| eyre::eyre!("latest Unit LOD group is out of bounds"))?;
        let mut version = [0u8; 4];
        version.copy_from_slice(&unit[VERSION_OFFSET..VERSION_OFFSET + 4]);
        Ok(Self {
            version,
            lod_group: lod_group.to_vec(),
        })
    }
}

/// Update a mod Unit's version-dependent layout formats and vanilla LOD group.
pub fn repatch_unit(unit: &mut Vec<u8>, latest: &LatestUnitParts) -> crate::Result<RepatchOutcome> {
    require_range(
        unit,
        OFFSET_TABLE_START,
        OFFSET_TABLE_COUNT * 4,
        "mod Unit header",
    )?;
    let lod_start = LE::read_u32(&unit[LOD_GROUP_OFFSET_FIELD..]) as usize;
    let lod_end = LE::read_u32(&unit[LOD_GROUP_OFFSET_FIELD + 4..]) as usize;
    validate_lod_range(unit, lod_start, lod_end)?;
    let version_is_current = unit[VERSION_OFFSET..VERSION_OFFSET + 4] == latest.version;
    let lod_is_current = unit[lod_start..lod_end] == latest.lod_group;
    if version_is_current && lod_is_current {
        return Ok(RepatchOutcome::AlreadyCurrent);
    }
    update_legacy_layout_formats(unit)?;
    let size_delta = latest.lod_group.len() as i64 - (lod_end - lod_start) as i64;
    adjust_offsets_after_lod(unit, lod_start as u32, size_delta)?;
    unit.splice(lod_start..lod_end, latest.lod_group.iter().copied());
    unit[VERSION_OFFSET..VERSION_OFFSET + 4].copy_from_slice(&latest.version);
    Ok(RepatchOutcome::Updated { size_delta })
}

fn update_legacy_layout_formats(unit: &mut [u8]) -> crate::Result<()> {
    let version = LE::read_u32(&unit[VERSION_OFFSET..]);
    if version >= FORMAT_LAYOUT_VERSION {
        return Ok(());
    }
    require_range(unit, LAYOUT_LIST_OFFSET_FIELD, 4, "layout list pointer")?;
    let list_start = LE::read_u32(&unit[LAYOUT_LIST_OFFSET_FIELD..]) as usize;
    require_range(unit, list_start, 4, "layout list")?;
    let layout_count = LE::read_u32(&unit[list_start..]) as usize;
    let offsets_start = list_start + 4;
    require_range(unit, offsets_start, layout_count * 4, "layout offsets")?;
    for index in 0..layout_count {
        update_layout_formats(unit, list_start, offsets_start + index * 4)?;
    }
    Ok(())
}

fn update_layout_formats(
    unit: &mut [u8],
    list_start: usize,
    offset_field: usize,
) -> crate::Result<()> {
    let relative = LE::read_u32(&unit[offset_field..]) as usize;
    let layout_start = list_start
        .checked_add(relative)
        .and_then(|value| value.checked_add(LAYOUT_HEADER_SIZE))
        .ok_or_else(|| eyre::eyre!("layout offset overflow"))?;
    require_range(
        unit,
        layout_start,
        ITEMS_PER_LAYOUT * LAYOUT_ITEM_SIZE,
        "layout items",
    )?;
    for item in 0..ITEMS_PER_LAYOUT {
        let format_offset = layout_start + item * LAYOUT_ITEM_SIZE + ITEM_FORMAT_OFFSET;
        let format = LE::read_u32(&unit[format_offset..]);
        if format > 16 {
            let updated = format
                .checked_add(4)
                .ok_or_else(|| eyre::eyre!("layout item format overflow"))?;
            LE::write_u32(&mut unit[format_offset..], updated);
        }
    }
    Ok(())
}

fn adjust_offsets_after_lod(unit: &mut [u8], lod_start: u32, delta: i64) -> crate::Result<()> {
    if delta == 0 {
        return Ok(());
    }
    for index in 0..OFFSET_TABLE_COUNT {
        let start = OFFSET_TABLE_START + index * 4;
        let offset = LE::read_u32(&unit[start..]);
        if offset != 0 && offset > lod_start {
            let adjusted = i64::from(offset) + delta;
            let adjusted = u32::try_from(adjusted)
                .map_err(|_| eyre::eyre!("Unit offset adjustment overflow"))?;
            LE::write_u32(&mut unit[start..], adjusted);
        }
    }
    Ok(())
}

fn validate_lod_range(unit: &[u8], start: usize, end: usize) -> crate::Result<()> {
    if end < start {
        eyre::bail!("mod Unit has a reversed LOD group range");
    }
    require_range(unit, start, end - start, "mod Unit LOD group")
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
    fn replaces_lod_and_adjusts_following_offsets() {
        let latest = latest_unit(0x00b0_0000, 0x80, &[9; 24]);
        let latest = LatestUnitParts::parse(&latest).expect("latest parts");
        let mut old = old_unit(0x00a4_cd36, 0x80, &[1; 8]);
        let outcome = repatch_unit(&mut old, &latest).expect("repatch");
        assert_eq!(outcome, RepatchOutcome::Updated { size_delta: 16 });
        assert_eq!(LE::read_u32(&old[VERSION_OFFSET..]), 0x00b0_0000);
        assert_eq!(LE::read_u32(&old[0x34..]), 0x98);
        assert_eq!(&old[0x80..0x98], &[9; 24]);
    }

    #[test]
    fn shrinks_lod_and_adjusts_following_offsets() {
        let latest = latest_unit(0x00b0_0000, 0x80, &[9; 8]);
        let latest = LatestUnitParts::parse(&latest).expect("latest parts");
        let mut old = old_unit(0x00a4_cd36, 0x80, &[1; 24]);
        let original_len = old.len();
        let outcome = repatch_unit(&mut old, &latest).expect("repatch");
        assert_eq!(outcome, RepatchOutcome::Updated { size_delta: -16 });
        assert_eq!(LE::read_u32(&old[0x34..]), 0x88);
        assert_eq!(&old[0x80..0x88], &[9; 8]);
        assert_eq!(old.len(), original_len - 16);
    }

    #[test]
    fn already_current_is_unchanged() {
        let bytes = latest_unit(0x00b0_0000, 0x80, &[9; 16]);
        let latest = LatestUnitParts::parse(&bytes).expect("latest parts");
        let mut unit = bytes.clone();
        assert_eq!(
            repatch_unit(&mut unit, &latest).unwrap(),
            RepatchOutcome::AlreadyCurrent
        );
        assert_eq!(unit, bytes);
    }

    #[test]
    fn refreshes_lod_when_version_is_already_current() {
        let latest = latest_unit(0x00b0_0000, 0x80, &[9; 16]);
        let latest = LatestUnitParts::parse(&latest).expect("latest parts");
        let mut unit = latest_unit(0x00b0_0000, 0x80, &[1; 8]);
        assert_eq!(
            repatch_unit(&mut unit, &latest).unwrap(),
            RepatchOutcome::Updated { size_delta: 8 }
        );
        assert_eq!(&unit[0x80..0x90], &[9; 16]);
    }

    #[test]
    fn updates_legacy_layout_item_formats() {
        let latest = latest_unit(0x00a4_cd36, 0x80, &[9; 16]);
        let latest = LatestUnitParts::parse(&latest).expect("latest parts");
        let mut old = old_unit(0x00a4_cd35, 0x240, &[1; 16]);
        old.resize(0x280, 0);
        LE::write_u32(&mut old[LAYOUT_LIST_OFFSET_FIELD..], 0x80);
        LE::write_u32(&mut old[0x80..], 1);
        LE::write_u32(&mut old[0x84..], 0x10);
        for item in 0..ITEMS_PER_LAYOUT {
            let format_offset = 0x98 + item * LAYOUT_ITEM_SIZE + ITEM_FORMAT_OFFSET;
            LE::write_u32(&mut old[format_offset..], 17);
        }
        repatch_unit(&mut old, &latest).expect("legacy repatch");
        for item in 0..ITEMS_PER_LAYOUT {
            let format_offset = 0x98 + item * LAYOUT_ITEM_SIZE + ITEM_FORMAT_OFFSET;
            assert_eq!(LE::read_u32(&old[format_offset..]), 21);
        }
    }

    fn latest_unit(version: u32, lod_start: u32, lod: &[u8]) -> Vec<u8> {
        old_unit(version, lod_start, lod)
    }

    fn old_unit(version: u32, lod_start: u32, lod: &[u8]) -> Vec<u8> {
        let lod_end = lod_start + lod.len() as u32;
        let mut unit = vec![0u8; lod_end as usize + 32];
        LE::write_u32(&mut unit[VERSION_OFFSET..], version);
        LE::write_u32(&mut unit[LOD_GROUP_OFFSET_FIELD..], lod_start);
        LE::write_u32(&mut unit[LOD_GROUP_OFFSET_FIELD + 4..], lod_end);
        unit[lod_start as usize..lod_end as usize].copy_from_slice(lod);
        unit
    }
}
