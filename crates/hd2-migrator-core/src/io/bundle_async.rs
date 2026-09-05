//! Async `bundles.*.nxa` reassembly over a [`DataSource`].
//!
//! Mirrors the sync [`crate::archive::BundleIndex`] but fetches bytes through
//! the `DataSource` trait so the browser path can stream `bundles.NN.nxa`
//! payloads via `file.slice(offset, len)` instead of loading the multi-GB
//! files into memory.
//!
//! All chunk parsing reuses the pure helpers in `crate::archive::bundle`
//! ([`parse_bundle_packages`], [`parse_chunk_descriptor_table`],
//! [`plan_chunk_walk`], [`decompress_chunk`]) so the two drivers cannot drift.

use crate::archive::bundle::{
    basename, bundle_entry_size, decompress_chunk, parse_bundle_packages, parse_chunk_count,
    parse_chunk_descriptor_table, plan_chunk_walk, BundleChunkHeader, BundlePackage,
};
use crate::archive::dsar;
use crate::io::DataSource;
use eyre::WrapErr;
use std::collections::HashMap;

pub struct BundleSlicer {
    pub packages: HashMap<String, BundlePackage>,
    pub headers: HashMap<String, BundleChunkHeader>,
}

impl BundleSlicer {
    pub async fn open<S: DataSource + ?Sized>(source: &S) -> crate::Result<Self> {
        let bundle_toc_bytes = source
            .read_full("bundles.nxa")
            .await
            .wrap_err("read bundles.nxa")?;
        let bundle_toc = dsar::decompress(&bundle_toc_bytes).wrap_err("decompress bundles.nxa")?;
        let packages = parse_bundle_packages(&bundle_toc)?;
        let chunk_files = source
            .list_bundle_chunks()
            .await
            .wrap_err("list bundle chunk files")?;
        let mut headers = HashMap::with_capacity(chunk_files.len());
        for name in chunk_files {
            let prefix = source
                .read_range(&name, 0, 12)
                .await
                .wrap_err_with(|| format!("read prefix {name}"))?;
            let num_chunks = parse_chunk_count(&prefix)?;
            let table_len = 0x20u64 * num_chunks as u64;
            let table = source
                .read_range(&name, 0x20, table_len)
                .await
                .wrap_err_with(|| format!("read descriptors {name}"))?;
            let header = parse_chunk_descriptor_table(&table)?;
            headers.insert(name, header);
        }
        Ok(Self { packages, headers })
    }

    pub fn has_package(&self, package_name: &str) -> bool {
        self.packages.contains_key(basename(package_name))
    }

    pub async fn load_triple<S: DataSource + ?Sized>(
        &self,
        source: &S,
        package_path: &str,
    ) -> crate::Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let toc = self.load_package(source, package_path).await?;
        let gpu = self
            .load_package(source, &format!("{package_path}.gpu_resources"))
            .await?;
        let stream = self
            .load_package(source, &format!("{package_path}.stream"))
            .await?;
        Ok((toc, gpu, stream))
    }

    pub async fn load_package<S: DataSource + ?Sized>(
        &self,
        source: &S,
        package_name: &str,
    ) -> crate::Result<Vec<u8>> {
        let name = basename(package_name);
        let Some(package) = self.packages.get(name) else {
            return Ok(Vec::new());
        };
        self.reconstruct_package(source, package).await
    }

    async fn reconstruct_package<S: DataSource + ?Sized>(
        &self,
        source: &S,
        package: &BundlePackage,
    ) -> crate::Result<Vec<u8>> {
        let mut out = vec![0u8; package.size as usize];
        for (index, entry) in package.entries.iter().enumerate() {
            let item_size = bundle_entry_size(package, index);
            let bundle_name = format!("bundles.{:02}.nxa", entry.bundle_index);
            let header = self
                .headers
                .get(&bundle_name)
                .ok_or_else(|| eyre::eyre!("missing chunk header for {bundle_name}"))?;
            let data = read_resource_range_async(
                source,
                &bundle_name,
                header,
                entry.start_offset as u64,
                item_size,
            )
            .await?;
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

async fn read_resource_range_async<S: DataSource + ?Sized>(
    source: &S,
    bundle_name: &str,
    header: &BundleChunkHeader,
    start_offset: u64,
    size: u64,
) -> crate::Result<Vec<u8>> {
    let mut data: Vec<u8> = Vec::with_capacity(size as usize);
    let mut current: u64 = 0;
    while current < size {
        let resource_offset = start_offset + current;
        let chunk_indices = plan_chunk_walk(header, resource_offset)?;
        let mut resource: Vec<u8> = Vec::new();
        for chunk_index in chunk_indices {
            let desc = &header.descriptors[chunk_index as usize];
            let compressed = source
                .read_range(bundle_name, desc.comp_off, desc.comp_sz as u64)
                .await
                .wrap_err_with(|| format!("read chunk {chunk_index} from {bundle_name}"))?;
            let decompressed = decompress_chunk(&compressed, desc)?;
            resource.extend_from_slice(&decompressed);
        }
        if resource.is_empty() {
            eyre::bail!("bundle resource read returned zero bytes at offset {resource_offset}");
        }
        current += resource.len() as u64;
        data.extend_from_slice(&resource);
    }
    data.truncate(size as usize);
    Ok(data)
}
