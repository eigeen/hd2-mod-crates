//! `bundles.*.nxa` reassembly for Slim Helldivers 2 installs.
//!
//! Ports `BundleIndex` from `mod_armor_migrator/archive.py`. The Slim install
//! lays every package out across many LZ4-compressed chunks scattered through
//! `bundles.00.nxa` … `bundles.NN.nxa`. The catalogue (which chunk holds which
//! byte of which logical package) lives in `bundles.nxa` (DSAR-compressed).

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

#[derive(Debug)]
pub struct BundleIndex {
    pub data_dir: PathBuf,
    pub packages: HashMap<String, BundlePackage>,
    /// chunk_offsets["bundles.07.nxa"] -> { raw_offset_u64 -> chunk_index }
    pub chunk_offsets: HashMap<String, HashMap<u64, u32>>,
}

impl BundleIndex {
    pub fn from_data_dir(data_dir: &Path) -> crate::Result<Self> {
        let bundle_toc_path = data_dir.join("bundles.nxa");
        let bundle_toc = dsar::decompress_file(&bundle_toc_path)
            .wrap_err_with(|| format!("decompress bundle TOC {}", bundle_toc_path.display()))?;
        let chunk_offsets = read_bundle_chunk_offsets(data_dir)?;
        let packages = read_bundle_packages(&bundle_toc)?;
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            packages,
            chunk_offsets,
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
            let data = self.read_resource(entry, item_size)?;
            let start = entry.original_archive_offset as usize;
            let end = start + data.len();
            if end > out.len() {
                eyre::bail!(
                    "bundle entry overruns package: offset {} + {} > {}",
                    start,
                    data.len(),
                    out.len()
                );
            }
            out[start..end].copy_from_slice(&data);
        }
        Ok(out)
    }

    fn read_resource(&self, entry: &BundleEntry, size: u64) -> crate::Result<Vec<u8>> {
        let bundle_name = format!("bundles.{:02}.nxa", entry.bundle_index);
        let bundle_path = self.data_dir.join(&bundle_name);
        let chunk_offsets = self
            .chunk_offsets
            .get(&bundle_name)
            .ok_or_else(|| eyre::eyre!("missing chunk offsets for {bundle_name}"))?;
        read_bundle_range(&bundle_path, chunk_offsets, entry.start_offset as u64, size)
    }
}

fn basename(path: &str) -> &str {
    let idx = path
        .rfind(|c: char| c == '/' || c == '\\')
        .map(|i| i + 1)
        .unwrap_or(0);
    &path[idx..]
}

fn read_bundle_chunk_offsets(data_dir: &Path) -> crate::Result<HashMap<String, HashMap<u64, u32>>> {
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
        let path = entry.path();
        let offsets = read_single_bundle_offsets(&path)?;
        out.insert(s.to_string(), offsets);
    }
    Ok(out)
}

fn is_bundle_name(name: &str) -> bool {
    // Matches `bundles.\d\d.nxa` — needs prefix + 2 digits + suffix = >= 14 bytes
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

fn read_single_bundle_offsets(path: &Path) -> crate::Result<HashMap<u64, u32>> {
    let mut f = File::open(path).wrap_err_with(|| format!("open {}", path.display()))?;
    f.seek(SeekFrom::Start(8))?;
    let mut buf4 = [0u8; 4];
    f.read_exact(&mut buf4)?;
    let num_chunks = LE::read_u32(&buf4) as usize;
    f.seek(SeekFrom::Start(0x20))?;
    let mut buf = vec![0u8; 0x20 * num_chunks];
    f.read_exact(&mut buf)?;
    let mut out = HashMap::with_capacity(num_chunks);
    for index in 0..num_chunks {
        let off = LE::read_u64(&buf[index * 0x20..index * 0x20 + 8]);
        out.insert(off, index as u32);
    }
    Ok(out)
}

fn read_bundle_packages(bundle_toc: &[u8]) -> crate::Result<HashMap<String, BundlePackage>> {
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

fn bundle_entry_size(package: &BundlePackage, index: usize) -> u64 {
    let entry = &package.entries[index];
    if index + 1 == package.entries.len() {
        package.size - entry.original_archive_offset
    } else {
        package.entries[index + 1].original_archive_offset - entry.original_archive_offset
    }
}

fn read_bundle_range(
    path: &Path,
    chunk_offsets: &HashMap<u64, u32>,
    start_offset: u64,
    size: u64,
) -> crate::Result<Vec<u8>> {
    let mut data: Vec<u8> = Vec::with_capacity(size as usize);
    let mut current: u64 = 0;
    let mut file = File::open(path).wrap_err_with(|| format!("open bundle {}", path.display()))?;
    let num_chunks = {
        file.seek(SeekFrom::Start(8))?;
        let mut buf = [0u8; 4];
        file.read_exact(&mut buf)?;
        LE::read_u32(&buf)
    };
    while current < size {
        let resource =
            read_bundle_resource(&mut file, chunk_offsets, num_chunks, start_offset + current)?;
        let n = resource.len() as u64;
        if n == 0 {
            eyre::bail!(
                "bundle resource read returned zero bytes at offset {}",
                start_offset + current
            );
        }
        data.extend_from_slice(&resource);
        current += n;
    }
    data.truncate(size as usize);
    Ok(data)
}

fn read_bundle_resource(
    file: &mut File,
    chunk_offsets: &HashMap<u64, u32>,
    num_chunks: u32,
    resource_offset: u64,
) -> crate::Result<Vec<u8>> {
    let mut chunk_index = *chunk_offsets
        .get(&resource_offset)
        .ok_or_else(|| eyre::eyre!("no chunk for offset {resource_offset}"))?;
    let mut parts: Vec<Vec<u8>> = Vec::new();
    while chunk_index < num_chunks {
        let (chunk, chunk_type) = read_bundle_chunk(file, chunk_index)?;
        if chunk_type & 0x02 != 0 && !parts.is_empty() {
            break;
        }
        parts.push(chunk);
        chunk_index += 1;
    }
    let total: usize = parts.iter().map(|p| p.len()).sum();
    let mut out = Vec::with_capacity(total);
    for p in parts {
        out.extend_from_slice(&p);
    }
    Ok(out)
}

fn read_bundle_chunk(file: &mut File, chunk_index: u32) -> crate::Result<(Vec<u8>, u8)> {
    file.seek(SeekFrom::Start(0x20 + 0x20 * chunk_index as u64))?;
    let mut desc = [0u8; 0x20];
    file.read_exact(&mut desc)?;
    // Q Q I I B B 6x
    let _unc_off = LE::read_u64(&desc[0..8]);
    let comp_off = LE::read_u64(&desc[8..16]);
    let unc_sz = LE::read_u32(&desc[16..20]) as usize;
    let comp_sz = LE::read_u32(&desc[20..24]) as usize;
    let comp_type = desc[24];
    let chunk_type = desc[25];
    file.seek(SeekFrom::Start(comp_off))?;
    let mut buf = vec![0u8; comp_sz];
    file.read_exact(&mut buf)?;
    let data = if comp_type == 3 {
        lz4_flex::block::decompress(&buf, unc_sz)
            .map_err(|e| crate::error::MigratorError::Lz4(e.to_string()))?
    } else {
        buf
    };
    Ok((data, chunk_type))
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
}
