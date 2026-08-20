//! LEGACY TOC editing without loading or rewriting GPU/stream sidecars.

use super::{HEADER_BASE, TOC_ENTRY_SIZE, TOC_FILE_TYPE_SIZE, TocFileType};
use crate::constants::LEGACY_MAGIC;
use byteorder::{ByteOrder, LittleEndian as LE};
use std::collections::{HashMap, HashSet};
use std::io::Write;

#[derive(Debug, Clone)]
pub struct TocOnlyEntry {
    pub file_id: u64,
    pub type_id: u64,
    pub unknown1: u64,
    pub unknown2: u64,
    pub unknown3: u32,
    pub unknown4: u32,
    pub toc_data: Vec<u8>,
    pub stream_offset: u64,
    pub gpu_offset: u64,
    pub stream_size: u32,
    pub gpu_size: u32,
}

#[derive(Debug, Clone)]
pub struct TocOnlyPackage {
    pub types: Vec<TocFileType>,
    pub entries: Vec<TocOnlyEntry>,
    pub unknown: u32,
    pub unk4_data: [u8; 56],
}

#[derive(Debug, Clone, Copy)]
pub struct TocEntryLocation {
    pub file_id: u64,
    pub type_id: u64,
    pub toc_offset: u64,
    pub toc_size: u32,
    pub stream_offset: u64,
    pub gpu_offset: u64,
    pub stream_size: u32,
    pub gpu_size: u32,
}

impl TocOnlyPackage {
    pub fn parse(data: &[u8]) -> crate::Result<Self> {
        let header = TocHeader::parse(data)?;
        let types = parse_types(data, &header)?;
        let locations = parse_entry_locations(data, &header)?;
        let entries = locations
            .iter()
            .enumerate()
            .map(|(index, location)| parse_entry(data, &header, index, *location))
            .collect::<crate::Result<Vec<_>>>()?;
        Ok(Self {
            types,
            entries,
            unknown: header.unknown,
            unk4_data: header.unk4_data,
        })
    }

    /// Rebuild only the TOC. GPU/stream offsets and sizes stay unchanged.
    pub fn serialize(&self) -> crate::Result<Vec<u8>> {
        let mut output = Vec::with_capacity(self.serialized_len());
        self.write_to(&mut output)?;
        Ok(output)
    }

    pub fn serialized_len(&self) -> usize {
        let groups = group_entries(&self.entries);
        let type_count = refresh_types(&self.types, &groups).len();
        let header_size =
            HEADER_BASE + type_count * TOC_FILE_TYPE_SIZE + self.entries.len() * TOC_ENTRY_SIZE;
        let body_size = self
            .entries
            .iter()
            .map(|entry| entry.toc_data.len())
            .sum::<usize>();
        (header_size + body_size).max(256 * self.entries.len())
    }

    /// Rebuild the TOC directly into a writer while preserving sidecar offsets.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> crate::Result<()> {
        let groups = group_entries(&self.entries);
        let types = refresh_types(&self.types, &groups);
        let header_size =
            HEADER_BASE + types.len() * TOC_FILE_TYPE_SIZE + self.entries.len() * TOC_ENTRY_SIZE;
        write_header(writer, self, &types)?;
        let mut body_offset = header_size as u64;
        for (entry_index, entry) in ordered_entries(&groups, &self.entries).enumerate() {
            write_entry_header(writer, entry, body_offset, entry_index)?;
            body_offset += entry.toc_data.len() as u64;
        }
        for entry in ordered_entries(&groups, &self.entries) {
            writer.write_all(&entry.toc_data)?;
        }
        let written = usize::try_from(body_offset)?;
        write_zeroes(writer, self.serialized_len().saturating_sub(written))
    }
}

#[derive(Debug, Clone)]
pub struct TocHeader {
    pub num_types: usize,
    pub num_files: usize,
    unknown: u32,
    unk4_data: [u8; 56],
}

impl TocHeader {
    pub fn parse(data: &[u8]) -> crate::Result<Self> {
        if data.len() < HEADER_BASE {
            eyre::bail!("TOC is too small: {} bytes", data.len());
        }
        let magic = LE::read_u32(&data[0..4]);
        if magic != LEGACY_MAGIC {
            eyre::bail!("unsupported TOC magic 0x{magic:08x}");
        }
        let mut unk4_data = [0u8; 56];
        unk4_data.copy_from_slice(&data[16..72]);
        Ok(Self {
            num_types: LE::read_u32(&data[4..8]) as usize,
            num_files: LE::read_u32(&data[8..12]) as usize,
            unknown: LE::read_u32(&data[12..16]),
            unk4_data,
        })
    }

    pub fn table_size(&self) -> crate::Result<usize> {
        let type_bytes = self.num_types.checked_mul(TOC_FILE_TYPE_SIZE);
        let entry_bytes = self.num_files.checked_mul(TOC_ENTRY_SIZE);
        HEADER_BASE
            .checked_add(type_bytes.ok_or_else(|| eyre::eyre!("type table overflow"))?)
            .and_then(|size| size.checked_add(entry_bytes?))
            .ok_or_else(|| eyre::eyre!("TOC table size overflow"))
    }
}

pub fn parse_entry_locations(
    data: &[u8],
    header: &TocHeader,
) -> crate::Result<Vec<TocEntryLocation>> {
    let entries_start = HEADER_BASE + header.num_types * TOC_FILE_TYPE_SIZE;
    let entries_end = header.table_size()?;
    if data.len() < entries_end {
        eyre::bail!("TOC header table is truncated");
    }
    Ok((0..header.num_files)
        .map(|index| parse_location(&data[entries_start + index * TOC_ENTRY_SIZE..]))
        .collect())
}

pub fn retain_locations(
    locations: &[TocEntryLocation],
    type_id: u64,
    wanted: &HashSet<u64>,
) -> Vec<TocEntryLocation> {
    locations
        .iter()
        .copied()
        .filter(|entry| entry.type_id == type_id && wanted.contains(&entry.file_id))
        .collect()
}

fn parse_types(data: &[u8], header: &TocHeader) -> crate::Result<Vec<TocFileType>> {
    if data.len() < header.table_size()? {
        eyre::bail!("TOC header table is truncated");
    }
    Ok((0..header.num_types)
        .map(|index| {
            let start = HEADER_BASE + index * TOC_FILE_TYPE_SIZE;
            TocFileType::unpack(&data[start..start + TOC_FILE_TYPE_SIZE])
        })
        .collect())
}

fn parse_entry(
    data: &[u8],
    header: &TocHeader,
    index: usize,
    location: TocEntryLocation,
) -> crate::Result<TocOnlyEntry> {
    let start = HEADER_BASE + header.num_types * TOC_FILE_TYPE_SIZE + index * TOC_ENTRY_SIZE;
    let raw = &data[start..start + TOC_ENTRY_SIZE];
    let toc_start = usize::try_from(location.toc_offset)?;
    let toc_end = toc_start
        .checked_add(location.toc_size as usize)
        .ok_or_else(|| eyre::eyre!("TOC body size overflow"))?;
    let toc_data = data
        .get(toc_start..toc_end)
        .ok_or_else(|| eyre::eyre!("TOC body is out of bounds for entry {index}"))?;
    Ok(TocOnlyEntry {
        file_id: location.file_id,
        type_id: location.type_id,
        unknown1: LE::read_u64(&raw[40..48]),
        unknown2: LE::read_u64(&raw[48..56]),
        unknown3: LE::read_u32(&raw[68..72]),
        unknown4: LE::read_u32(&raw[72..76]),
        toc_data: toc_data.to_vec(),
        stream_offset: LE::read_u64(&raw[24..32]),
        gpu_offset: LE::read_u64(&raw[32..40]),
        stream_size: LE::read_u32(&raw[60..64]),
        gpu_size: LE::read_u32(&raw[64..68]),
    })
}

fn parse_location(raw: &[u8]) -> TocEntryLocation {
    TocEntryLocation {
        file_id: LE::read_u64(&raw[0..8]),
        type_id: LE::read_u64(&raw[8..16]),
        toc_offset: LE::read_u64(&raw[16..24]),
        stream_offset: LE::read_u64(&raw[24..32]),
        gpu_offset: LE::read_u64(&raw[32..40]),
        toc_size: LE::read_u32(&raw[56..60]),
        stream_size: LE::read_u32(&raw[60..64]),
        gpu_size: LE::read_u32(&raw[64..68]),
    }
}

fn group_entries(entries: &[TocOnlyEntry]) -> Vec<(u64, Vec<usize>)> {
    let mut groups = Vec::<(u64, Vec<usize>)>::new();
    for (index, entry) in entries.iter().enumerate() {
        match groups
            .iter_mut()
            .find(|(type_id, _)| *type_id == entry.type_id)
        {
            Some((_, indices)) => indices.push(index),
            None => groups.push((entry.type_id, vec![index])),
        }
    }
    groups
}

fn refresh_types(existing: &[TocFileType], groups: &[(u64, Vec<usize>)]) -> Vec<TocFileType> {
    let existing = existing
        .iter()
        .map(|file_type| (file_type.type_id, file_type))
        .collect::<HashMap<_, _>>();
    groups
        .iter()
        .map(|(type_id, entries)| {
            let mut file_type = existing
                .get(type_id)
                .map(|value| (*value).clone())
                .unwrap_or_else(|| TocFileType::new(*type_id, 0));
            file_type.num_files = entries.len() as u32;
            file_type
        })
        .collect()
}

fn ordered_entries<'a>(
    groups: &'a [(u64, Vec<usize>)],
    entries: &'a [TocOnlyEntry],
) -> impl Iterator<Item = &'a TocOnlyEntry> {
    groups
        .iter()
        .flat_map(|(_, indices)| indices.iter().map(|index| &entries[*index]))
}

fn write_header<W: Write>(
    writer: &mut W,
    package: &TocOnlyPackage,
    types: &[TocFileType],
) -> crate::Result<()> {
    writer.write_all(&LEGACY_MAGIC.to_le_bytes())?;
    writer.write_all(&(types.len() as u32).to_le_bytes())?;
    writer.write_all(&(package.entries.len() as u32).to_le_bytes())?;
    writer.write_all(&package.unknown.to_le_bytes())?;
    writer.write_all(&package.unk4_data)?;
    for file_type in types {
        let mut raw = [0u8; TOC_FILE_TYPE_SIZE];
        file_type.pack_into(&mut raw);
        writer.write_all(&raw)?;
    }
    Ok(())
}

fn write_entry_header<W: Write>(
    writer: &mut W,
    entry: &TocOnlyEntry,
    toc_offset: u64,
    entry_index: usize,
) -> crate::Result<()> {
    let mut raw = [0u8; TOC_ENTRY_SIZE];
    LE::write_u64(&mut raw[0..8], entry.file_id);
    LE::write_u64(&mut raw[8..16], entry.type_id);
    LE::write_u64(&mut raw[16..24], toc_offset);
    LE::write_u64(&mut raw[24..32], entry.stream_offset);
    LE::write_u64(&mut raw[32..40], entry.gpu_offset);
    LE::write_u64(&mut raw[40..48], entry.unknown1);
    LE::write_u64(&mut raw[48..56], entry.unknown2);
    LE::write_u32(&mut raw[56..60], u32::try_from(entry.toc_data.len())?);
    LE::write_u32(&mut raw[60..64], entry.stream_size);
    LE::write_u32(&mut raw[64..68], entry.gpu_size);
    LE::write_u32(&mut raw[68..72], entry.unknown3);
    LE::write_u32(&mut raw[72..76], entry.unknown4);
    LE::write_u32(&mut raw[76..80], u32::try_from(entry_index + 1)?);
    writer.write_all(&raw)?;
    Ok(())
}

fn write_zeroes<W: Write>(writer: &mut W, mut count: usize) -> crate::Result<()> {
    const ZEROES: [u8; 8192] = [0; 8192];
    while count > 0 {
        let chunk_size = count.min(ZEROES.len());
        writer.write_all(&ZEROES[..chunk_size])?;
        count -= chunk_size;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::{StreamToc, TocEntry};
    use crate::constants::UNIT_ID;

    #[test]
    fn round_trip_preserves_sidecar_layout() {
        let mut entry = TocEntry::new(7, UNIT_ID);
        entry.toc_data = vec![1; 128];
        entry.gpu_data = vec![2; 11].into();
        entry.stream_data = vec![3; 13].into();
        let mut original = StreamToc {
            entries: vec![entry],
            ..Default::default()
        };
        let (toc, gpu, stream) = original.serialize();
        let parsed = TocOnlyPackage::parse(&toc).expect("parse TOC-only");
        let updated = parsed.serialize().expect("serialize TOC-only");
        let full = StreamToc::from_buffers(&updated, &gpu, &stream, "test".into())
            .expect("parse with original sidecars");
        assert_eq!(full.entries[0].gpu_data.as_ref(), &[2; 11]);
        assert_eq!(full.entries[0].stream_data.as_ref(), &[3; 13]);
    }

    #[test]
    fn writer_matches_buffered_serialization() {
        let mut entry = TocEntry::new(7, UNIT_ID);
        entry.toc_data = vec![1; 513];
        let mut original = StreamToc {
            entries: vec![entry],
            ..Default::default()
        };
        let (toc, _, _) = original.serialize();
        let package = TocOnlyPackage::parse(&toc).expect("parse TOC-only");
        let mut streamed = Vec::new();

        package.write_to(&mut streamed).expect("stream TOC-only");

        assert_eq!(streamed, package.serialize().expect("serialize TOC-only"));
        assert_eq!(streamed.len(), package.serialized_len());
    }
}
