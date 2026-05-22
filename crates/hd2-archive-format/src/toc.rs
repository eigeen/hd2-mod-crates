use crate::constants::{align_up, GPU_ALIGN, LEGACY_MAGIC, STREAM_ALIGN};
use crate::error::{message, MigratorError, Result};
use byteorder::{ByteOrder, LittleEndian as LE};
use std::collections::BTreeMap;

const TOC_FILE_TYPE_SIZE: usize = 32;
const TOC_ENTRY_SIZE: usize = 80;
const HEADER_BASE: usize = 72;

#[derive(Debug, Clone, Default)]
pub struct TocFileType {
    pub type_id: u64,
    pub num_files: u32,
    pub unk1: u64,
    pub unk2: u32,
    pub unk3: u32,
}

impl TocFileType {
    pub fn new(type_id: u64, num_files: u32) -> Self {
        Self {
            type_id,
            num_files,
            unk1: 0,
            unk2: 16,
            unk3: 64,
        }
    }

    fn unpack(buf: &[u8]) -> Self {
        Self {
            unk1: LE::read_u64(&buf[0..8]),
            type_id: LE::read_u64(&buf[8..16]),
            num_files: LE::read_u64(&buf[16..24]) as u32,
            unk2: LE::read_u32(&buf[24..28]),
            unk3: LE::read_u32(&buf[28..32]),
        }
    }

    fn pack_into(&self, buf: &mut [u8]) {
        LE::write_u64(&mut buf[0..8], self.unk1);
        LE::write_u64(&mut buf[8..16], self.type_id);
        LE::write_u64(&mut buf[16..24], u64::from(self.num_files));
        LE::write_u32(&mut buf[24..28], self.unk2);
        LE::write_u32(&mut buf[28..32], self.unk3);
    }
}

#[derive(Debug, Clone, Default)]
pub struct TocEntry {
    pub file_id: u64,
    pub type_id: u64,
    pub unknown1: u64,
    pub unknown2: u64,
    pub unknown3: u32,
    pub unknown4: u32,
    pub entry_index: u32,
    pub toc_data: Vec<u8>,
    pub gpu_data: Vec<u8>,
    pub stream_data: Vec<u8>,
}

impl TocEntry {
    pub fn new(file_id: u64, type_id: u64) -> Self {
        Self {
            file_id,
            type_id,
            unknown3: 16,
            unknown4: 64,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct StreamToc {
    pub types: Vec<TocFileType>,
    pub entries: Vec<TocEntry>,
    pub unknown: u32,
    pub unk4_data: [u8; 56],
    pub name: String,
}

impl Default for StreamToc {
    fn default() -> Self {
        Self {
            types: Vec::new(),
            entries: Vec::new(),
            unknown: 0,
            unk4_data: [0; 56],
            name: String::new(),
        }
    }
}

impl StreamToc {
    pub fn from_buffers(
        toc_data: &[u8],
        gpu_data: &[u8],
        stream_data: &[u8],
        name: String,
    ) -> Result<Self> {
        if toc_data.len() < HEADER_BASE {
            return Err(message(format!("toc too small: {} bytes", toc_data.len())));
        }
        validate_magic(toc_data)?;
        let num_types = LE::read_u32(&toc_data[4..8]) as usize;
        let num_files = LE::read_u32(&toc_data[8..12]) as usize;
        let unknown = LE::read_u32(&toc_data[12..16]);
        let mut unk4_data = [0u8; 56];
        unk4_data.copy_from_slice(&toc_data[16..72]);

        let types = read_types(toc_data, num_types)?;
        let entries = read_entries(toc_data, gpu_data, stream_data, num_types, num_files)?;

        Ok(Self {
            types,
            entries,
            unknown,
            unk4_data,
            name,
        })
    }

    pub fn serialize(&mut self) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let groups = entry_groups(&self.entries);
        self.types = groups
            .iter()
            .map(|(tid, idxs)| TocFileType::new(*tid, idxs.len() as u32))
            .collect();
        serialize_toc(self, &groups)
    }

    pub fn find(&self, file_id: u64, type_id: u64) -> Option<&TocEntry> {
        self.entries
            .iter()
            .find(|entry| entry.file_id == file_id && entry.type_id == type_id)
    }

    pub fn by_type(&self) -> BTreeMap<u64, Vec<&TocEntry>> {
        let mut out: BTreeMap<u64, Vec<&TocEntry>> = BTreeMap::new();
        for entry in &self.entries {
            out.entry(entry.type_id).or_default().push(entry);
        }
        out
    }
}

pub fn list_file_ids_from_bytes(data: &[u8]) -> Result<BTreeMap<u64, Vec<u64>>> {
    if data.len() < HEADER_BASE {
        return Ok(BTreeMap::new());
    }
    validate_magic(data)?;
    let num_types = LE::read_u32(&data[4..8]) as usize;
    let num_files = LE::read_u32(&data[8..12]) as usize;
    let entries_start = HEADER_BASE + num_types * TOC_FILE_TYPE_SIZE;
    let entries_end = entries_start + num_files * TOC_ENTRY_SIZE;
    if data.len() < entries_end {
        return Err(message("toc truncated"));
    }
    let mut out: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for index in 0..num_files {
        let off = entries_start + index * TOC_ENTRY_SIZE;
        let file_id = LE::read_u64(&data[off..off + 8]);
        let type_id = LE::read_u64(&data[off + 8..off + 16]);
        out.entry(type_id).or_default().push(file_id);
    }
    Ok(out)
}

fn validate_magic(data: &[u8]) -> Result<()> {
    let magic = LE::read_u32(&data[0..4]);
    if magic == LEGACY_MAGIC {
        return Ok(());
    }
    Err(MigratorError::BadMagic {
        expected: LEGACY_MAGIC,
        got: magic,
    })
}

fn read_types(toc_data: &[u8], num_types: usize) -> Result<Vec<TocFileType>> {
    let entries_start = HEADER_BASE + num_types * TOC_FILE_TYPE_SIZE;
    if toc_data.len() < entries_start {
        return Err(message("toc truncated before type table end"));
    }
    let mut types = Vec::with_capacity(num_types);
    for index in 0..num_types {
        let off = HEADER_BASE + index * TOC_FILE_TYPE_SIZE;
        types.push(TocFileType::unpack(&toc_data[off..off + TOC_FILE_TYPE_SIZE]));
    }
    Ok(types)
}

fn read_entries(
    toc_data: &[u8],
    gpu_data: &[u8],
    stream_data: &[u8],
    num_types: usize,
    num_files: usize,
) -> Result<Vec<TocEntry>> {
    let entries_start = HEADER_BASE + num_types * TOC_FILE_TYPE_SIZE;
    let bodies_start = entries_start + num_files * TOC_ENTRY_SIZE;
    if toc_data.len() < bodies_start {
        return Err(message("toc truncated before entry table end"));
    }
    let mut entries = Vec::with_capacity(num_files);
    for index in 0..num_files {
        entries.push(read_entry(toc_data, gpu_data, stream_data, entries_start, index)?);
    }
    Ok(entries)
}

fn read_entry(
    toc_data: &[u8],
    gpu_data: &[u8],
    stream_data: &[u8],
    entries_start: usize,
    index: usize,
) -> Result<TocEntry> {
    let off = entries_start + index * TOC_ENTRY_SIZE;
    let hdr = &toc_data[off..off + TOC_ENTRY_SIZE];
    let toc_off = LE::read_u64(&hdr[16..24]) as usize;
    let stream_off = LE::read_u64(&hdr[24..32]) as usize;
    let gpu_off = LE::read_u64(&hdr[32..40]) as usize;
    let toc_size = LE::read_u32(&hdr[56..60]) as usize;
    let stream_size = LE::read_u32(&hdr[60..64]) as usize;
    let gpu_size = LE::read_u32(&hdr[64..68]) as usize;

    Ok(TocEntry {
        file_id: LE::read_u64(&hdr[0..8]),
        type_id: LE::read_u64(&hdr[8..16]),
        unknown1: LE::read_u64(&hdr[40..48]),
        unknown2: LE::read_u64(&hdr[48..56]),
        unknown3: LE::read_u32(&hdr[68..72]),
        unknown4: LE::read_u32(&hdr[72..76]),
        entry_index: LE::read_u32(&hdr[76..80]),
        toc_data: slice_vec(toc_data, toc_off, toc_size, "toc body")?,
        gpu_data: slice_vec(gpu_data, gpu_off, gpu_size, "gpu body")?,
        stream_data: slice_vec(stream_data, stream_off, stream_size, "stream body")?,
    })
}

fn slice_vec(data: &[u8], offset: usize, size: usize, label: &str) -> Result<Vec<u8>> {
    if size == 0 {
        return Ok(Vec::new());
    }
    let end = offset
        .checked_add(size)
        .ok_or_else(|| message(format!("{label} offset overflow")))?;
    data.get(offset..end)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| message(format!("{label} out of bounds")))
}

fn entry_groups(entries: &[TocEntry]) -> Vec<(u64, Vec<usize>)> {
    let mut groups: Vec<(u64, Vec<usize>)> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        match groups.iter_mut().find(|(type_id, _)| *type_id == entry.type_id) {
            Some((_, indexes)) => indexes.push(index),
            None => groups.push((entry.type_id, vec![index])),
        }
    }
    groups
}

fn serialize_toc(toc: &mut StreamToc, groups: &[(u64, Vec<usize>)]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let ordered_indexes: Vec<usize> = groups
        .iter()
        .flat_map(|(_, indexes)| indexes.iter().copied())
        .collect();
    let header_size =
        HEADER_BASE + toc.types.len() * TOC_FILE_TYPE_SIZE + toc.entries.len() * TOC_ENTRY_SIZE;
    let layouts = layout_entries(&toc.entries, &ordered_indexes, header_size);
    let mut toc_buf = write_header(toc, &ordered_indexes, &layouts, header_size);
    let gpu_buf = write_data_buffer(&toc.entries, &ordered_indexes, &layouts, DataKind::Gpu);
    let stream_buf = write_data_buffer(&toc.entries, &ordered_indexes, &layouts, DataKind::Stream);

    for index in ordered_indexes {
        toc_buf.extend_from_slice(&toc.entries[index].toc_data);
    }
    let min_size = 256 * toc.entries.len();
    if toc_buf.len() < min_size {
        toc_buf.resize(min_size, 0);
    }
    for (index, layout) in layouts.iter().enumerate() {
        toc.entries[index].entry_index = layout.entry_index;
    }
    (toc_buf, gpu_buf, stream_buf)
}

#[derive(Clone, Copy, Default)]
struct EntryLayout {
    toc_offset: u64,
    gpu_offset: u64,
    stream_offset: u64,
    entry_index: u32,
}

fn layout_entries(entries: &[TocEntry], ordered_indexes: &[usize], header_size: usize) -> Vec<EntryLayout> {
    let mut layouts = vec![EntryLayout::default(); entries.len()];
    let mut toc_cursor = header_size as u64;
    let mut gpu_cursor = 0u64;
    let mut stream_cursor = 0u64;
    for (position, index) in ordered_indexes.iter().copied().enumerate() {
        let entry = &entries[index];
        let mut layout = EntryLayout {
            toc_offset: toc_cursor,
            entry_index: (position + 1) as u32,
            ..Default::default()
        };
        toc_cursor += entry.toc_data.len() as u64;
        if !entry.gpu_data.is_empty() {
            gpu_cursor = align_up(gpu_cursor as usize, GPU_ALIGN) as u64;
            layout.gpu_offset = gpu_cursor;
            gpu_cursor += entry.gpu_data.len() as u64;
        }
        if !entry.stream_data.is_empty() {
            stream_cursor = align_up(stream_cursor as usize, STREAM_ALIGN) as u64;
            layout.stream_offset = stream_cursor;
            stream_cursor += entry.stream_data.len() as u64;
        }
        layouts[index] = layout;
    }
    layouts
}

fn write_header(
    toc: &StreamToc,
    ordered_indexes: &[usize],
    layouts: &[EntryLayout],
    header_size: usize,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(header_size);
    out.extend_from_slice(&LEGACY_MAGIC.to_le_bytes());
    out.extend_from_slice(&(toc.types.len() as u32).to_le_bytes());
    out.extend_from_slice(&(toc.entries.len() as u32).to_le_bytes());
    out.extend_from_slice(&toc.unknown.to_le_bytes());
    out.extend_from_slice(&toc.unk4_data);
    for file_type in &toc.types {
        let mut buf = [0u8; TOC_FILE_TYPE_SIZE];
        file_type.pack_into(&mut buf);
        out.extend_from_slice(&buf);
    }
    for index in ordered_indexes {
        out.extend_from_slice(&entry_header(&toc.entries[*index], layouts[*index]));
    }
    out
}

fn entry_header(entry: &TocEntry, layout: EntryLayout) -> [u8; TOC_ENTRY_SIZE] {
    let mut hdr = [0u8; TOC_ENTRY_SIZE];
    LE::write_u64(&mut hdr[0..8], entry.file_id);
    LE::write_u64(&mut hdr[8..16], entry.type_id);
    LE::write_u64(&mut hdr[16..24], layout.toc_offset);
    LE::write_u64(&mut hdr[24..32], layout.stream_offset);
    LE::write_u64(&mut hdr[32..40], layout.gpu_offset);
    LE::write_u64(&mut hdr[40..48], entry.unknown1);
    LE::write_u64(&mut hdr[48..56], entry.unknown2);
    LE::write_u32(&mut hdr[56..60], entry.toc_data.len() as u32);
    LE::write_u32(&mut hdr[60..64], entry.stream_data.len() as u32);
    LE::write_u32(&mut hdr[64..68], entry.gpu_data.len() as u32);
    LE::write_u32(&mut hdr[68..72], entry.unknown3);
    LE::write_u32(&mut hdr[72..76], entry.unknown4);
    LE::write_u32(&mut hdr[76..80], layout.entry_index);
    hdr
}

enum DataKind {
    Gpu,
    Stream,
}

fn write_data_buffer(
    entries: &[TocEntry],
    ordered_indexes: &[usize],
    layouts: &[EntryLayout],
    kind: DataKind,
) -> Vec<u8> {
    let mut out = Vec::new();
    for index in ordered_indexes {
        let entry = &entries[*index];
        let (offset, bytes) = match kind {
            DataKind::Gpu => (layouts[*index].gpu_offset as usize, &entry.gpu_data),
            DataKind::Stream => (layouts[*index].stream_offset as usize, &entry.stream_data),
        };
        if bytes.is_empty() {
            continue;
        }
        let end = offset + bytes.len();
        if out.len() < end {
            out.resize(end, 0);
        }
        out[offset..end].copy_from_slice(bytes);
    }
    out
}
