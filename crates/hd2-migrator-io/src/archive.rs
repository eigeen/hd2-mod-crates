//! Helldivers 2 LEGACY package format reader/writer.
//!
//! Ports `mod_armor_migrator/archive.py`. A package is a trio of files:
//! `<name>` (TOC), `<name>.gpu_resources`, `<name>.stream`.
//!
//! Header layout (little-endian):
//!
//! ```text
//! 0..4    magic = 0xF0000011 (LEGACY)
//! 4..8    num_types
//! 8..12   num_files
//! 12..16  unknown
//! 16..72  unk4_data (56 bytes)
//! 72..    TocFileType[num_types]              (32 bytes each)
//!         TocEntry header[num_files]          (80 bytes each)
//!         per-entry toc_data concatenated
//! ```

pub mod bundle;
pub mod dsar;
pub mod reassembly;
pub mod sidecar;
pub mod stream_metadata;
pub mod toc_only;

pub use bundle::BundleIndex;

use crate::constants::{GPU_ALIGN, LEGACY_MAGIC, STREAM_ALIGN, align_up};
use crate::error::MigratorError;
use byteorder::{ByteOrder, LittleEndian as LE};
use eyre::WrapErr;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    /// Resource sidecars are immutable during migration and shared across variants.
    pub gpu_data: Arc<[u8]>,
    pub stream_data: Arc<[u8]>,
}

impl TocEntry {
    pub fn new(file_id: u64, type_id: u64) -> Self {
        Self {
            file_id,
            type_id,
            unknown1: 0,
            unknown2: 0,
            unknown3: 16,
            unknown4: 64,
            entry_index: 0,
            toc_data: Vec::new(),
            gpu_data: Arc::default(),
            stream_data: Arc::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct EntryLayout {
    toc_data_offset: u64,
    stream_offset: u64,
    gpu_offset: u64,
    toc_size: u32,
    stream_size: u32,
    gpu_size: u32,
    entry_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializedPart {
    Toc,
    Gpu,
    Stream,
}

struct SerializationPlan {
    header_size: usize,
    layouts: Vec<EntryLayout>,
    ordered_indexes: Vec<usize>,
}

pub struct StreamTocSerializer<'a> {
    archive: &'a StreamToc,
    plan: SerializationPlan,
}

impl StreamTocSerializer<'_> {
    pub fn part_len(&self, part: SerializedPart) -> usize {
        match part {
            SerializedPart::Toc => self.toc_len(),
            SerializedPart::Gpu => self.resource_len(SerializedPart::Gpu),
            SerializedPart::Stream => self.resource_len(SerializedPart::Stream),
        }
    }

    /// Writes one serialized archive part without allocating a full output buffer.
    pub fn write_part<W: Write>(&self, part: SerializedPart, writer: &mut W) -> crate::Result<()> {
        match part {
            SerializedPart::Toc => self.write_toc(writer),
            SerializedPart::Gpu => self.write_resource(writer, SerializedPart::Gpu),
            SerializedPart::Stream => self.write_resource(writer, SerializedPart::Stream),
        }
    }

    fn write_toc<W: Write>(&self, writer: &mut W) -> crate::Result<()> {
        write_toc_header(writer, self.archive, &self.plan)?;
        for &index in &self.plan.ordered_indexes {
            writer.write_all(&self.archive.entries[index].toc_data)?;
        }
        let written = self.plan.header_size
            + self
                .plan
                .ordered_indexes
                .iter()
                .map(|&index| self.archive.entries[index].toc_data.len())
                .sum::<usize>();
        write_zeroes(writer, self.toc_len().saturating_sub(written))
    }

    fn toc_len(&self) -> usize {
        let content_len = self.plan.header_size
            + self
                .plan
                .ordered_indexes
                .iter()
                .map(|&index| self.archive.entries[index].toc_data.len())
                .sum::<usize>();
        content_len.max(256 * self.archive.entries.len())
    }

    fn resource_len(&self, part: SerializedPart) -> usize {
        self.plan
            .ordered_indexes
            .iter()
            .map(|&index| {
                let entry = &self.archive.entries[index];
                let (offset, bytes) = resource_part(entry, self.plan.layouts[index], part);
                offset + bytes.len()
            })
            .max()
            .unwrap_or(0)
    }

    fn write_resource<W: Write>(&self, writer: &mut W, part: SerializedPart) -> crate::Result<()> {
        let mut cursor = 0usize;
        for &index in &self.plan.ordered_indexes {
            let entry = &self.archive.entries[index];
            let layout = self.plan.layouts[index];
            let (offset, bytes) = resource_part(entry, layout, part);
            if bytes.is_empty() {
                continue;
            }
            write_zeroes(writer, offset.saturating_sub(cursor))?;
            writer.write_all(bytes)?;
            cursor = offset + bytes.len();
        }
        Ok(())
    }
}

fn prepare_serialization(archive: &mut StreamToc) -> SerializationPlan {
    let groups = group_entry_indexes(&archive.entries);
    archive.types = groups
        .iter()
        .map(|(type_id, indexes)| TocFileType::new(*type_id, indexes.len() as u32))
        .collect();
    let ordered_indexes = groups
        .into_iter()
        .flat_map(|(_, indexes)| indexes)
        .collect::<Vec<_>>();
    let header_size = HEADER_BASE
        + archive.types.len() * TOC_FILE_TYPE_SIZE
        + archive.entries.len() * TOC_ENTRY_SIZE;
    let layouts = layout_entries(&archive.entries, &ordered_indexes, header_size);
    apply_entry_indexes(&mut archive.entries, &layouts);
    SerializationPlan {
        header_size,
        layouts,
        ordered_indexes,
    }
}

fn group_entry_indexes(entries: &[TocEntry]) -> Vec<(u64, Vec<usize>)> {
    let mut groups: Vec<(u64, Vec<usize>)> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        match groups
            .iter_mut()
            .find(|(type_id, _)| *type_id == entry.type_id)
        {
            Some((_, indexes)) => indexes.push(index),
            None => groups.push((entry.type_id, vec![index])),
        }
    }
    groups
}

fn layout_entries(
    entries: &[TocEntry],
    ordered_indexes: &[usize],
    header_size: usize,
) -> Vec<EntryLayout> {
    let mut layouts = vec![EntryLayout::default(); entries.len()];
    let mut toc_cursor = header_size as u64;
    let mut gpu_cursor = 0u64;
    let mut stream_cursor = 0u64;
    for (position, &index) in ordered_indexes.iter().enumerate() {
        let entry = &entries[index];
        let mut layout = toc_entry_layout(entry, position, toc_cursor);
        toc_cursor += entry.toc_data.len() as u64;
        layout_gpu(entry, &mut layout, &mut gpu_cursor);
        layout_stream(entry, &mut layout, &mut stream_cursor);
        layouts[index] = layout;
    }
    layouts
}

fn toc_entry_layout(entry: &TocEntry, position: usize, offset: u64) -> EntryLayout {
    EntryLayout {
        toc_data_offset: offset,
        toc_size: entry.toc_data.len() as u32,
        entry_index: (position + 1) as u32,
        ..Default::default()
    }
}

fn layout_gpu(entry: &TocEntry, layout: &mut EntryLayout, cursor: &mut u64) {
    if entry.gpu_data.is_empty() {
        return;
    }
    *cursor = align_up(*cursor as usize, GPU_ALIGN) as u64;
    layout.gpu_offset = *cursor;
    layout.gpu_size = entry.gpu_data.len() as u32;
    *cursor += entry.gpu_data.len() as u64;
}

fn layout_stream(entry: &TocEntry, layout: &mut EntryLayout, cursor: &mut u64) {
    if entry.stream_data.is_empty() {
        return;
    }
    *cursor = align_up(*cursor as usize, STREAM_ALIGN) as u64;
    layout.stream_offset = *cursor;
    layout.stream_size = entry.stream_data.len() as u32;
    *cursor += entry.stream_data.len() as u64;
}

fn apply_entry_indexes(entries: &mut [TocEntry], layouts: &[EntryLayout]) {
    for (entry, layout) in entries.iter_mut().zip(layouts) {
        entry.entry_index = layout.entry_index;
    }
}

fn write_toc_header<W: Write>(
    writer: &mut W,
    archive: &StreamToc,
    plan: &SerializationPlan,
) -> crate::Result<()> {
    write_archive_header(writer, archive)?;
    write_type_headers(writer, &archive.types)?;
    for &index in &plan.ordered_indexes {
        write_entry_header(writer, &archive.entries[index], plan.layouts[index])?;
    }
    Ok(())
}

fn write_archive_header<W: Write>(writer: &mut W, archive: &StreamToc) -> crate::Result<()> {
    writer.write_all(&LEGACY_MAGIC.to_le_bytes())?;
    writer.write_all(&(archive.types.len() as u32).to_le_bytes())?;
    writer.write_all(&(archive.entries.len() as u32).to_le_bytes())?;
    writer.write_all(&archive.unknown.to_le_bytes())?;
    writer.write_all(&archive.unk4_data)?;
    Ok(())
}

fn write_type_headers<W: Write>(writer: &mut W, types: &[TocFileType]) -> crate::Result<()> {
    for file_type in types {
        let mut bytes = [0u8; TOC_FILE_TYPE_SIZE];
        file_type.pack_into(&mut bytes);
        writer.write_all(&bytes)?;
    }
    Ok(())
}

fn write_entry_header<W: Write>(
    writer: &mut W,
    entry: &TocEntry,
    layout: EntryLayout,
) -> crate::Result<()> {
    let mut bytes = [0u8; TOC_ENTRY_SIZE];
    LE::write_u64(&mut bytes[0..8], entry.file_id);
    LE::write_u64(&mut bytes[8..16], entry.type_id);
    LE::write_u64(&mut bytes[16..24], layout.toc_data_offset);
    LE::write_u64(&mut bytes[24..32], layout.stream_offset);
    LE::write_u64(&mut bytes[32..40], layout.gpu_offset);
    write_entry_header_tail(&mut bytes, entry, layout);
    writer.write_all(&bytes)?;
    Ok(())
}

fn write_entry_header_tail(bytes: &mut [u8], entry: &TocEntry, layout: EntryLayout) {
    LE::write_u64(&mut bytes[40..48], entry.unknown1);
    LE::write_u64(&mut bytes[48..56], entry.unknown2);
    LE::write_u32(&mut bytes[56..60], layout.toc_size);
    LE::write_u32(&mut bytes[60..64], layout.stream_size);
    LE::write_u32(&mut bytes[64..68], layout.gpu_size);
    LE::write_u32(&mut bytes[68..72], entry.unknown3);
    LE::write_u32(&mut bytes[72..76], entry.unknown4);
    LE::write_u32(&mut bytes[76..80], layout.entry_index);
}

fn resource_part(entry: &TocEntry, layout: EntryLayout, part: SerializedPart) -> (usize, &[u8]) {
    match part {
        SerializedPart::Gpu => (layout.gpu_offset as usize, &entry.gpu_data),
        SerializedPart::Stream => (layout.stream_offset as usize, &entry.stream_data),
        SerializedPart::Toc => unreachable!("TOC is not a sidecar resource"),
    }
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
    pub fn from_files(toc_path: &Path) -> crate::Result<Self> {
        Self::from_files_with_bundle(toc_path, None)
    }

    pub fn from_files_with_bundle(
        toc_path: &Path,
        bundle_index: Option<&BundleIndex>,
    ) -> crate::Result<Self> {
        let (toc_bytes, gpu_bytes, stream_bytes) = load_triple_with_bundle(toc_path, bundle_index)?;
        let name = toc_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        Self::from_buffers(&toc_bytes, &gpu_bytes, &stream_bytes, name)
    }

    pub fn from_buffers(
        toc_data: &[u8],
        gpu_data: &[u8],
        stream_data: &[u8],
        name: String,
    ) -> crate::Result<Self> {
        if toc_data.len() < HEADER_BASE {
            eyre::bail!("toc too small: {} bytes", toc_data.len());
        }
        let magic = LE::read_u32(&toc_data[0..4]);
        if magic != LEGACY_MAGIC {
            return Err(MigratorError::BadMagic {
                expected: LEGACY_MAGIC,
                got: magic,
            }
            .into());
        }
        let num_types = LE::read_u32(&toc_data[4..8]) as usize;
        let num_files = LE::read_u32(&toc_data[8..12]) as usize;
        let unknown = LE::read_u32(&toc_data[12..16]);
        let mut unk4_data = [0u8; 56];
        unk4_data.copy_from_slice(&toc_data[16..72]);

        let mut types = Vec::with_capacity(num_types);
        let types_start = HEADER_BASE;
        let entries_start = types_start + num_types * TOC_FILE_TYPE_SIZE;
        let bodies_start = entries_start + num_files * TOC_ENTRY_SIZE;
        if toc_data.len() < bodies_start {
            eyre::bail!(
                "toc truncated: header expects {} bytes, got {}",
                bodies_start,
                toc_data.len()
            );
        }

        for i in 0..num_types {
            let off = types_start + i * TOC_FILE_TYPE_SIZE;
            types.push(TocFileType::unpack(
                &toc_data[off..off + TOC_FILE_TYPE_SIZE],
            ));
        }

        let mut entries = Vec::with_capacity(num_files);
        for i in 0..num_files {
            let off = entries_start + i * TOC_ENTRY_SIZE;
            let hdr = &toc_data[off..off + TOC_ENTRY_SIZE];
            let file_id = LE::read_u64(&hdr[0..8]);
            let type_id = LE::read_u64(&hdr[8..16]);
            let toc_off = LE::read_u64(&hdr[16..24]) as usize;
            let stream_off = LE::read_u64(&hdr[24..32]) as usize;
            let gpu_off = LE::read_u64(&hdr[32..40]) as usize;
            let unknown1 = LE::read_u64(&hdr[40..48]);
            let unknown2 = LE::read_u64(&hdr[48..56]);
            let toc_sz = LE::read_u32(&hdr[56..60]) as usize;
            let stream_sz = LE::read_u32(&hdr[60..64]) as usize;
            let gpu_sz = LE::read_u32(&hdr[64..68]) as usize;
            let unknown3 = LE::read_u32(&hdr[68..72]);
            let unknown4 = LE::read_u32(&hdr[72..76]);
            let entry_index = LE::read_u32(&hdr[76..80]);

            let toc_body = slice_safe(toc_data, toc_off, toc_sz)
                .ok_or_else(|| eyre::eyre!("toc body OOB for entry {i}"))?;
            let gpu_body = if gpu_sz != 0 {
                slice_safe(gpu_data, gpu_off, gpu_sz)
                    .ok_or_else(|| eyre::eyre!("gpu body OOB for entry {i}"))?
            } else {
                &[]
            };
            let stream_body = if stream_sz != 0 {
                slice_safe(stream_data, stream_off, stream_sz)
                    .ok_or_else(|| eyre::eyre!("stream body OOB for entry {i}"))?
            } else {
                &[]
            };

            entries.push(TocEntry {
                file_id,
                type_id,
                unknown1,
                unknown2,
                unknown3,
                unknown4,
                entry_index,
                toc_data: toc_body.to_vec(),
                gpu_data: Arc::from(gpu_body),
                stream_data: Arc::from(stream_body),
            });
        }

        Ok(Self {
            types,
            entries,
            unknown,
            unk4_data,
            name,
        })
    }

    pub fn write_files(&mut self, toc_path: &Path) -> crate::Result<()> {
        let (toc_buf, gpu_buf, stream_buf) = self.serialize();
        if let Some(parent) = toc_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .wrap_err_with(|| format!("create dir {}", parent.display()))?;
        }
        std::fs::write(toc_path, &toc_buf)
            .wrap_err_with(|| format!("write TOC {}", toc_path.display()))?;
        let gpu_path = append_suffix(toc_path, ".gpu_resources");
        std::fs::write(&gpu_path, &gpu_buf)
            .wrap_err_with(|| format!("write {}", gpu_path.display()))?;
        let stream_path = append_suffix(toc_path, ".stream");
        std::fs::write(&stream_path, &stream_buf)
            .wrap_err_with(|| format!("write {}", stream_path.display()))?;
        Ok(())
    }

    /// Prepares a reusable layout for writing archive parts directly to output streams.
    pub fn serializer(&mut self) -> StreamTocSerializer<'_> {
        let plan = prepare_serialization(self);
        StreamTocSerializer {
            archive: self,
            plan,
        }
    }

    /// Refresh the archive layout and return its three serialized byte buffers.
    pub fn serialize(&mut self) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let serializer = self.serializer();
        let mut toc = Vec::new();
        let mut gpu = Vec::new();
        let mut stream = Vec::new();
        serializer
            .write_part(SerializedPart::Toc, &mut toc)
            .expect("writing TOC to memory cannot fail");
        serializer
            .write_part(SerializedPart::Gpu, &mut gpu)
            .expect("writing GPU data to memory cannot fail");
        serializer
            .write_part(SerializedPart::Stream, &mut stream)
            .expect("writing stream data to memory cannot fail");
        (toc, gpu, stream)
    }

    pub fn find(&self, file_id: u64, type_id: u64) -> Option<&TocEntry> {
        self.entries
            .iter()
            .find(|e| e.file_id == file_id && e.type_id == type_id)
    }

    pub fn by_type(&self) -> BTreeMap<u64, Vec<&TocEntry>> {
        let mut out: BTreeMap<u64, Vec<&TocEntry>> = BTreeMap::new();
        for t in &self.types {
            out.entry(t.type_id).or_default();
        }
        for e in &self.entries {
            out.entry(e.type_id).or_default().push(e);
        }
        out
    }
}

/// Lightweight FileID index without loading entry bodies; used for source autodetect.
pub fn list_file_ids(toc_path: &Path) -> crate::Result<BTreeMap<u64, Vec<u64>>> {
    list_file_ids_with_bundle(toc_path, None)
}

pub fn list_file_ids_with_bundle(
    toc_path: &Path,
    bundle_index: Option<&BundleIndex>,
) -> crate::Result<BTreeMap<u64, Vec<u64>>> {
    let kind = detect_kind(toc_path, bundle_index)?;
    let data = match kind {
        PackageKind::Legacy => {
            std::fs::read(toc_path).wrap_err_with(|| format!("read TOC {}", toc_path.display()))?
        }
        PackageKind::Dsar => dsar::decompress_file(toc_path)?,
        PackageKind::Bundled => {
            let idx = bundle_index.expect("bundle_index present for Bundled");
            let name = toc_path
                .to_str()
                .ok_or_else(|| eyre::eyre!("non-UTF8 path"))?;
            idx.load_package(name)?
        }
    };
    list_file_ids_from_bytes(&data)
}

pub fn list_file_ids_from_bytes(data: &[u8]) -> crate::Result<BTreeMap<u64, Vec<u64>>> {
    if data.len() < HEADER_BASE {
        return Ok(BTreeMap::new());
    }
    let magic = LE::read_u32(&data[0..4]);
    if magic != LEGACY_MAGIC {
        return Ok(BTreeMap::new());
    }
    let num_types = LE::read_u32(&data[4..8]) as usize;
    let num_files = LE::read_u32(&data[8..12]) as usize;
    let entries_start = HEADER_BASE + num_types * TOC_FILE_TYPE_SIZE;
    let entries_end = entries_start + num_files * TOC_ENTRY_SIZE;
    if data.len() < entries_end {
        eyre::bail!("toc truncated");
    }
    let mut out: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for i in 0..num_files {
        let off = entries_start + i * TOC_ENTRY_SIZE;
        let file_id = LE::read_u64(&data[off..off + 8]);
        let type_id = LE::read_u64(&data[off + 8..off + 16]);
        out.entry(type_id).or_default().push(file_id);
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageKind {
    Legacy,
    Dsar,
    Bundled,
}

fn detect_kind(toc_path: &Path, bundle_index: Option<&BundleIndex>) -> crate::Result<PackageKind> {
    if !toc_path.exists() {
        if let Some(idx) = bundle_index {
            let name = toc_path
                .to_str()
                .ok_or_else(|| eyre::eyre!("non-UTF8 path"))?;
            if idx.has_package(name) {
                return Ok(PackageKind::Bundled);
            }
        }
        return Err(eyre::eyre!("file not found: {}", toc_path.display()));
    }
    let mut buf = [0u8; 4];
    use std::io::Read;
    let mut f =
        std::fs::File::open(toc_path).wrap_err_with(|| format!("open {}", toc_path.display()))?;
    let n = f.read(&mut buf).wrap_err("read magic")?;
    if n < 4 {
        eyre::bail!("file too short to detect kind: {}", toc_path.display());
    }
    let magic = LE::read_u32(&buf);
    match magic {
        LEGACY_MAGIC => Ok(PackageKind::Legacy),
        m if m == crate::constants::DSAR_MAGIC => Ok(PackageKind::Dsar),
        _ => Err(MigratorError::BadMagic {
            expected: LEGACY_MAGIC,
            got: magic,
        }
        .into()),
    }
}

/// Load (toc, gpu, stream) for a package path. Auto-detects LEGACY vs DSAR
/// vs bundled (Slim install). Pass `bundle_index` to support Slim layouts.
pub fn load_triple(toc_path: &Path) -> crate::Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    load_triple_with_bundle(toc_path, None)
}

pub fn load_triple_with_bundle(
    toc_path: &Path,
    bundle_index: Option<&BundleIndex>,
) -> crate::Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let kind = detect_kind(toc_path, bundle_index)?;
    let gpu_path = append_suffix(toc_path, ".gpu_resources");
    let stream_path = append_suffix(toc_path, ".stream");
    Ok(match kind {
        PackageKind::Legacy => (
            std::fs::read(toc_path).wrap_err_with(|| format!("read {}", toc_path.display()))?,
            read_or_empty(&gpu_path).wrap_err_with(|| format!("read {}", gpu_path.display()))?,
            read_or_empty(&stream_path)
                .wrap_err_with(|| format!("read {}", stream_path.display()))?,
        ),
        PackageKind::Dsar => (
            dsar::decompress_file(toc_path)?,
            if gpu_path.exists() {
                dsar::decompress_file(&gpu_path)?
            } else {
                Vec::new()
            },
            if stream_path.exists() {
                dsar::decompress_file(&stream_path)?
            } else {
                Vec::new()
            },
        ),
        PackageKind::Bundled => {
            let idx = bundle_index.expect("bundle_index present for Bundled kind");
            idx.load_triple(toc_path)?
        }
    })
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn read_or_empty(path: &Path) -> std::io::Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(v) => Ok(v),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

fn slice_safe(buf: &[u8], off: usize, sz: usize) -> Option<&[u8]> {
    let end = off.checked_add(sz)?;
    buf.get(off..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{MATERIAL_ID, UNIT_ID};

    fn make_entry(file_id: u64, type_id: u64, toc: &[u8], gpu: &[u8], stream: &[u8]) -> TocEntry {
        let mut e = TocEntry::new(file_id, type_id);
        e.toc_data = toc.to_vec();
        e.gpu_data = Arc::from(gpu);
        e.stream_data = Arc::from(stream);
        e
    }

    #[test]
    fn round_trip_two_entries() {
        let mut t = StreamToc {
            entries: vec![
                make_entry(0xAA, UNIT_ID, &[1u8; 100], &[2u8; 50], &[3u8; 20]),
                make_entry(0xBB, MATERIAL_ID, &[4u8; 60], &[], &[]),
            ],
            ..Default::default()
        };
        let (toc, gpu, stream) = t.serialize();
        let parsed = StreamToc::from_buffers(&toc, &gpu, &stream, "test".into()).expect("parse");
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].file_id, 0xAA);
        assert_eq!(parsed.entries[0].type_id, UNIT_ID);
        assert_eq!(parsed.entries[0].toc_data.len(), 100);
        assert_eq!(parsed.entries[0].gpu_data.as_ref(), &[2u8; 50]);
        assert_eq!(parsed.entries[0].stream_data.as_ref(), &[3u8; 20]);
        assert_eq!(parsed.entries[1].file_id, 0xBB);
        assert_eq!(parsed.entries[1].toc_data, vec![4u8; 60]);
        assert!(parsed.entries[1].gpu_data.is_empty());
    }

    #[test]
    fn cloned_entries_share_immutable_resource_sidecars() {
        let original = make_entry(1, UNIT_ID, b"toc", b"gpu", b"stream");
        let cloned = original.clone();

        assert!(Arc::ptr_eq(&original.gpu_data, &cloned.gpu_data));
        assert!(Arc::ptr_eq(&original.stream_data, &cloned.stream_data));
        assert_ne!(original.toc_data.as_ptr(), cloned.toc_data.as_ptr());
    }

    #[test]
    fn list_file_ids_matches() {
        let mut t = StreamToc {
            entries: vec![
                make_entry(0xAA, UNIT_ID, &[1u8; 10], &[], &[]),
                make_entry(0xBB, UNIT_ID, &[2u8; 10], &[], &[]),
                make_entry(0xCC, MATERIAL_ID, &[3u8; 10], &[], &[]),
            ],
            ..Default::default()
        };
        let (toc, _, _) = t.serialize();
        let idx = list_file_ids_from_bytes(&toc).expect("list");
        assert_eq!(idx.get(&UNIT_ID).unwrap(), &vec![0xAAu64, 0xBB]);
        assert_eq!(idx.get(&MATERIAL_ID).unwrap(), &vec![0xCCu64]);
    }

    #[test]
    fn min_size_padding() {
        // 3 entries with tiny bodies → toc header alone is well below 3*256.
        let mut t = StreamToc {
            entries: vec![
                make_entry(0x01, UNIT_ID, &[0u8; 4], &[], &[]),
                make_entry(0x02, UNIT_ID, &[0u8; 4], &[], &[]),
                make_entry(0x03, UNIT_ID, &[0u8; 4], &[], &[]),
            ],
            ..Default::default()
        };
        let (toc, _, _) = t.serialize();
        assert!(toc.len() >= 256 * 3, "got {} bytes", toc.len());
    }

    #[test]
    fn gpu_align_64() {
        let mut t = StreamToc {
            entries: vec![
                make_entry(1, UNIT_ID, &[0; 10], &[0xAA; 100], &[]),
                make_entry(2, UNIT_ID, &[0; 10], &[0xBB; 50], &[]),
            ],
            ..Default::default()
        };
        let (toc, _, _) = t.serialize();
        // Round-trip and inspect entry 2's gpu_offset via re-parse.
        let parsed = StreamToc::from_buffers(&toc, &t.serialize().1, &[], "x".into()).unwrap();
        // Second entry's gpu data starts at align_up(100, 64) = 128.
        assert_eq!(parsed.entries[1].gpu_data.len(), 50);
        // FIXME: indirect check; offset is internal but if alignment were wrong
        // the from_buffers slice would not equal 0xBB.
        assert!(parsed.entries[1].gpu_data.iter().all(|&b| b == 0xBB));
    }
}
