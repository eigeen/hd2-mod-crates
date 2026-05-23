//! `bundles.*.nxa` reassembly for Slim Helldivers 2 installs.
//!
//! Ports `BundleIndex` from `mod_armor_migrator/archive.py`. The Slim install
//! lays every package out across many LZ4-compressed chunks scattered through
//! `bundles.00.nxa` … `bundles.NN.nxa`. The catalogue (which chunk holds which
//! byte of which logical package) lives in `bundles.nxa` (DSAR-compressed).
//!
//! Parsing helpers ([`parse_bundle_packages`], [`parse_chunk_descriptor_table`],
//! [`parse_chunk_count`], [`plan_chunk_walk`], [`decompress_chunk`]) are pure
//! byte-in / data-out functions shared between the synchronous [`BundleIndex`]
//! (used by the CLI) and the async `BundleSlicer` (used by the web/wasm driver
//! in `crate::io::bundle_async`).

use super::dsar;
use byteorder::{ByteOrder, LittleEndian as LE};
use eyre::WrapErr;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct BundleEntry {
    pub original_archive_offset: u64,
    pub start_offset: u32,
    pub bundle_index: u8,
}

#[derive(Debug, Clone)]
pub struct BundlePackage {
    pub size: u64,
    pub entries: Vec<BundleEntry>,
}

#[derive(Debug, Clone, Copy)]
pub struct ChunkDescriptor {
    pub unc_off: u64,
    pub comp_off: u64,
    pub unc_sz: u32,
    pub comp_sz: u32,
    pub comp_type: u8,
    pub chunk_type: u8,
}

#[derive(Debug, Clone)]
pub struct BundleChunkHeader {
    pub descriptors: Vec<ChunkDescriptor>,
    /// Maps a chunk's `unc_off` to its index in `descriptors`.
    pub offsets: HashMap<u64, u32>,
}

#[derive(Debug)]
pub struct BundleIndex {
    pub data_dir: PathBuf,
    pub packages: HashMap<String, BundlePackage>,
    pub headers: HashMap<String, BundleChunkHeader>,
}

impl BundleIndex {
    pub fn from_data_dir(data_dir: &Path) -> crate::Result<Self> {
        let bundle_toc_path = data_dir.join("bundles.nxa");
        let bundle_toc = dsar::decompress_file(&bundle_toc_path)
            .wrap_err_with(|| format!("decompress bundle TOC {}", bundle_toc_path.display()))?;
        let packages = parse_bundle_packages(&bundle_toc)?;
        let headers = read_bundle_headers_sync(data_dir)?;
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            packages,
            headers,
        })
    }

    pub fn has_package(&self, package_name: &str) -> bool {
        self.packages.contains_key(basename(package_name))
    }

    pub fn load_package(&self, package_name: &str) -> crate::Result<Vec<u8>> {
        let name = basename(package_name);
        let Some(package) = self.packages.get(name) else {
            return Ok(Vec::new());
        };
        self.reconstruct_package(package)
    }

    pub fn load_triple(&self, package_path: &Path) -> crate::Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let path_str = package_path
            .to_str()
            .ok_or_else(|| eyre::eyre!("non-UTF8 path"))?;
        let toc = self.load_package(path_str)?;
        let gpu_name = format!("{}.gpu_resources", path_str);
        let stream_name = format!("{}.stream", path_str);
        let gpu = self.load_package(&gpu_name)?;
        let stream = self.load_package(&stream_name)?;
        Ok((toc, gpu, stream))
    }

    fn reconstruct_package(&self, package: &BundlePackage) -> crate::Result<Vec<u8>> {
        let mut out = vec![0u8; package.size as usize];
        for (index, entry) in package.entries.iter().enumerate() {
            let item_size = bundle_entry_size(package, index);
            let bundle_name = format!("bundles.{:02}.nxa", entry.bundle_index);
            let header = self
                .headers
                .get(&bundle_name)
                .ok_or_else(|| eyre::eyre!("missing chunk header for {bundle_name}"))?;
            let bundle_path = self.data_dir.join(&bundle_name);
            let data = read_resource_range_sync(
                &bundle_path,
                header,
                entry.start_offset as u64,
                item_size,
            )?;
            let start = entry.original_archive_offset as usize;
            let end = start + data.len();
            if end > out.len() {
                eyre::bail!(
                    "bundle entry overruns package: offset {start} + {} > {}",
                    data.len(),
                    out.len()
                );
            }
            out[start..end].copy_from_slice(&data);
        }
        Ok(out)
    }
}

// ---------- pure parsers (shared by sync + async drivers) ---------------

/// Reads the chunk count stored at offset 8 of a `bundles.NN.nxa` file. The
/// input must contain at least 12 bytes from the bundle file's start.
pub fn parse_chunk_count(prefix: &[u8]) -> crate::Result<u32> {
    if prefix.len() < 12 {
        eyre::bail!("bundle prefix too small: {}", prefix.len());
    }
    Ok(LE::read_u32(&prefix[8..12]))
}

/// Parses the chunk-descriptor table located at `0x20..0x20 + 0x20 * num_chunks`
/// of a `bundles.NN.nxa` file. `table` must be exactly that range.
pub fn parse_chunk_descriptor_table(table: &[u8]) -> crate::Result<BundleChunkHeader> {
    if table.len() % 0x20 != 0 {
        eyre::bail!(
            "chunk descriptor table size {} not a multiple of 0x20",
            table.len()
        );
    }
    let num_chunks = table.len() / 0x20;
    let mut descriptors = Vec::with_capacity(num_chunks);
    let mut offsets = HashMap::with_capacity(num_chunks);
    for index in 0..num_chunks {
        let desc = parse_chunk_descriptor(&table[index * 0x20..(index + 1) * 0x20]);
        offsets.insert(desc.unc_off, index as u32);
        descriptors.push(desc);
    }
    Ok(BundleChunkHeader {
        descriptors,
        offsets,
    })
}

/// Parses the decompressed `bundles.nxa` table-of-contents into package entries.
pub fn parse_bundle_packages(bundle_toc: &[u8]) -> crate::Result<HashMap<String, BundlePackage>> {
    if bundle_toc.len() < 0x14 {
        eyre::bail!("bundle TOC too small: {}", bundle_toc.len());
    }
    let num_packages = LE::read_u32(&bundle_toc[0x10..0x14]) as usize;
    let mut out = HashMap::with_capacity(num_packages);
    for index in 0..num_packages {
        let (name, size, entries) = read_bundle_package(bundle_toc, index)?;
        out.insert(name, BundlePackage { size, entries });
    }
    Ok(out)
}

/// Plans the sequence of chunks that make up a single resource starting at
/// `resource_offset` (an `unc_off` from the descriptor table). A resource ends
/// at the next chunk whose `chunk_type & 0x02` boundary flag is set, or at the
/// end of the bundle if no such chunk follows.
pub fn plan_chunk_walk(
    header: &BundleChunkHeader,
    resource_offset: u64,
) -> crate::Result<Vec<u32>> {
    let start = *header
        .offsets
        .get(&resource_offset)
        .ok_or_else(|| eyre::eyre!("no chunk at offset {resource_offset}"))?;
    let mut out = Vec::new();
    for index in (start as usize)..header.descriptors.len() {
        let desc = &header.descriptors[index];
        if desc.chunk_type & 0x02 != 0 && !out.is_empty() {
            break;
        }
        out.push(index as u32);
    }
    Ok(out)
}

/// Decompresses a single chunk's compressed payload using the descriptor's
/// declared compression type. `comp_type == 3` is LZ4 block; anything else is
/// treated as a literal pass-through.
pub fn decompress_chunk(compressed: &[u8], desc: &ChunkDescriptor) -> crate::Result<Vec<u8>> {
    if desc.comp_type == 3 {
        lz4_flex::block::decompress(compressed, desc.unc_sz as usize)
            .map_err(|e| crate::error::MigratorError::Lz4(e.to_string()).into())
    } else {
        Ok(compressed.to_vec())
    }
}

/// Computes the byte size of bundle entry `index` from the package's size and
/// the next entry's `original_archive_offset`.
pub fn bundle_entry_size(package: &BundlePackage, index: usize) -> u64 {
    let entry = &package.entries[index];
    if index + 1 == package.entries.len() {
        package.size - entry.original_archive_offset
    } else {
        package.entries[index + 1].original_archive_offset - entry.original_archive_offset
    }
}

/// Filename basename (after the last `/` or `\`). Bundle TOC keys store
/// package names without their directory prefix.
pub fn basename(path: &str) -> &str {
    let idx = path.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    &path[idx..]
}

// ---------- sync chunk reads (native CLI path) --------------------------

fn read_bundle_headers_sync(data_dir: &Path) -> crate::Result<HashMap<String, BundleChunkHeader>> {
    let mut out = HashMap::new();
    let read_dir =
        std::fs::read_dir(data_dir).wrap_err_with(|| format!("read dir {}", data_dir.display()))?;
    for entry in read_dir {
        let entry = entry.wrap_err("read_dir entry")?;
        let name = entry.file_name();
        let Some(s) = name.to_str() else { continue };
        if !is_bundle_name(s) {
            continue;
        }
        let header = read_single_bundle_header(&entry.path())?;
        out.insert(s.to_string(), header);
    }
    Ok(out)
}

fn read_single_bundle_header(path: &Path) -> crate::Result<BundleChunkHeader> {
    let mut f = File::open(path).wrap_err_with(|| format!("open {}", path.display()))?;
    let mut prefix = [0u8; 12];
    f.read_exact(&mut prefix)?;
    let num_chunks = parse_chunk_count(&prefix)?;
    f.seek(SeekFrom::Start(0x20))?;
    let mut table = vec![0u8; 0x20 * num_chunks as usize];
    f.read_exact(&mut table)?;
    parse_chunk_descriptor_table(&table)
}

fn read_resource_range_sync(
    path: &Path,
    header: &BundleChunkHeader,
    start_offset: u64,
    size: u64,
) -> crate::Result<Vec<u8>> {
    let mut file = File::open(path).wrap_err_with(|| format!("open bundle {}", path.display()))?;
    let mut data: Vec<u8> = Vec::with_capacity(size as usize);
    let mut current: u64 = 0;
    while current < size {
        let resource_offset = start_offset + current;
        let chunk_indices = plan_chunk_walk(header, resource_offset)?;
        let mut resource: Vec<u8> = Vec::new();
        for chunk_index in chunk_indices {
            let desc = &header.descriptors[chunk_index as usize];
            file.seek(SeekFrom::Start(desc.comp_off))?;
            let mut buf = vec![0u8; desc.comp_sz as usize];
            file.read_exact(&mut buf)?;
            let decompressed = decompress_chunk(&buf, desc)?;
            resource.extend_from_slice(&decompressed);
        }
        if resource.is_empty() {
            eyre::bail!(
                "bundle resource read returned zero bytes at offset {resource_offset}"
            );
        }
        current += resource.len() as u64;
        data.extend_from_slice(&resource);
    }
    data.truncate(size as usize);
    Ok(data)
}

// ---------- pure-byte helpers used by both sync and async loaders -------

fn is_bundle_name(name: &str) -> bool {
    // Matches `bundles.NN.nxa` where NN is exactly two ASCII digits.
    const PREFIX: &str = "bundles.";
    const SUFFIX: &str = ".nxa";
    if name.len() != PREFIX.len() + 2 + SUFFIX.len() {
        return false;
    }
    if !name.starts_with(PREFIX) || !name.ends_with(SUFFIX) {
        return false;
    }
    let middle = &name[PREFIX.len()..PREFIX.len() + 2];
    middle.chars().all(|c| c.is_ascii_digit())
}

fn parse_chunk_descriptor(bytes: &[u8]) -> ChunkDescriptor {
    debug_assert_eq!(bytes.len(), 0x20);
    ChunkDescriptor {
        unc_off: LE::read_u64(&bytes[0..8]),
        comp_off: LE::read_u64(&bytes[8..16]),
        unc_sz: LE::read_u32(&bytes[16..20]),
        comp_sz: LE::read_u32(&bytes[20..24]),
        comp_type: bytes[24],
        chunk_type: bytes[25],
    }
}

fn read_bundle_package(
    bundle_toc: &[u8],
    index: usize,
) -> crate::Result<(String, u64, Vec<BundleEntry>)> {
    let offset = 0x18 + index * 0x18;
    if offset + 0x18 > bundle_toc.len() {
        eyre::bail!("bundle package descriptor OOB at index {index}");
    }
    let size = LE::read_u64(&bundle_toc[offset..offset + 8]);
    let name_off = LE::read_u32(&bundle_toc[offset + 8..offset + 12]) as usize;
    let count = LE::read_u32(&bundle_toc[offset + 12..offset + 16]) as usize;
    let entries_offset = LE::read_u32(&bundle_toc[offset + 16..offset + 20]) as usize;
    let name = read_null_string(bundle_toc, name_off)?;
    let mut entries: Vec<BundleEntry> = (0..count)
        .map(|i| read_bundle_entry(bundle_toc, entries_offset, i))
        .collect::<crate::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.original_archive_offset);
    Ok((name, size, entries))
}

fn read_bundle_entry(
    bundle_toc: &[u8],
    entries_offset: usize,
    index: usize,
) -> crate::Result<BundleEntry> {
    let offset = entries_offset + 0x10 * index;
    if offset + 0x10 > bundle_toc.len() {
        eyre::bail!("bundle entry OOB at index {index}");
    }
    Ok(BundleEntry {
        original_archive_offset: LE::read_u64(&bundle_toc[offset..offset + 8]),
        start_offset: LE::read_u32(&bundle_toc[offset + 8..offset + 12]),
        bundle_index: bundle_toc[offset + 0x0F],
    })
}

fn read_null_string(data: &[u8], offset: usize) -> crate::Result<String> {
    if offset >= data.len() {
        eyre::bail!("name offset OOB: {offset}");
    }
    let end = data[offset..]
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| eyre::eyre!("unterminated name string at {offset}"))?;
    std::str::from_utf8(&data[offset..offset + end])
        .map(|s| s.to_string())
        .map_err(|e| eyre::eyre!("invalid UTF-8 in name: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_strips_path() {
        assert_eq!(basename("/some/path/x.nxa"), "x.nxa");
        assert_eq!(basename("simple"), "simple");
        assert_eq!(basename(r"C:\dir\file.txt"), "file.txt");
    }

    #[test]
    fn bundle_name_matcher() {
        assert!(is_bundle_name("bundles.00.nxa"));
        assert!(is_bundle_name("bundles.42.nxa"));
        assert!(!is_bundle_name("bundles.nxa"));
        assert!(!is_bundle_name("bundles.1.nxa"));
        assert!(!is_bundle_name("foo.bundles.00.nxa"));
        assert!(!is_bundle_name("bundles.00.nxa.bak"));
    }

    #[test]
    fn parse_chunk_count_reads_offset_8() {
        let mut bytes = [0u8; 16];
        LE::write_u32(&mut bytes[8..12], 0x1234);
        assert_eq!(parse_chunk_count(&bytes).unwrap(), 0x1234);
    }

    #[test]
    fn parse_chunk_count_rejects_short_input() {
        assert!(parse_chunk_count(&[0u8; 11]).is_err());
    }

    #[test]
    fn parse_chunk_descriptor_table_round_trips() {
        let mut table = vec![0u8; 0x40]; // 2 chunks
        LE::write_u64(&mut table[0..8], 100); // chunk 0 unc_off
        LE::write_u64(&mut table[8..16], 200); // chunk 0 comp_off
        LE::write_u32(&mut table[16..20], 300); // chunk 0 unc_sz
        LE::write_u32(&mut table[20..24], 50); // chunk 0 comp_sz
        table[24] = 3; // LZ4
        table[25] = 0; // chunk_type
        LE::write_u64(&mut table[0x20..0x20 + 8], 400);
        table[0x20 + 25] = 2; // boundary flag
        let header = parse_chunk_descriptor_table(&table).unwrap();
        assert_eq!(header.descriptors.len(), 2);
        assert_eq!(header.offsets.get(&100), Some(&0));
        assert_eq!(header.offsets.get(&400), Some(&1));
        assert_eq!(header.descriptors[0].unc_sz, 300);
        assert_eq!(header.descriptors[1].chunk_type, 2);
    }

    #[test]
    fn plan_chunk_walk_stops_at_boundary() {
        let header = BundleChunkHeader {
            descriptors: vec![
                ChunkDescriptor { unc_off: 0, comp_off: 0, unc_sz: 0, comp_sz: 0, comp_type: 0, chunk_type: 0 },
                ChunkDescriptor { unc_off: 100, comp_off: 0, unc_sz: 0, comp_sz: 0, comp_type: 0, chunk_type: 0 },
                ChunkDescriptor { unc_off: 200, comp_off: 0, unc_sz: 0, comp_sz: 0, comp_type: 0, chunk_type: 2 },
                ChunkDescriptor { unc_off: 300, comp_off: 0, unc_sz: 0, comp_sz: 0, comp_type: 0, chunk_type: 0 },
            ],
            offsets: [(0, 0), (100, 1), (200, 2), (300, 3)].into_iter().collect(),
        };
        // From chunk 0: takes 0, 1, stops before 2 (boundary).
        assert_eq!(plan_chunk_walk(&header, 0).unwrap(), vec![0, 1]);
        // From chunk 2: takes 2 (the first chunk includes its own boundary flag), 3
        assert_eq!(plan_chunk_walk(&header, 200).unwrap(), vec![2, 3]);
    }
}
