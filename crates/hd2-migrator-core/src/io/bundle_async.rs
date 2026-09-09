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
    BundleChunkHeader, BundlePackage, basename, bundle_entry_size, decompress_chunk,
    parse_bundle_packages, parse_chunk_count, parse_chunk_descriptor_table, plan_chunk_walk,
};
use crate::archive::dsar;
use crate::io::DataSource;
use eyre::WrapErr;
use std::collections::HashMap;
use std::ops::Range;

const MAX_COALESCED_COMPRESSED_READ: u64 = 4 * 1024 * 1024;

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

    /// Reconstruct only the requested logical bytes of a bundled package.
    pub async fn load_package_range<S: DataSource + ?Sized>(
        &self,
        source: &S,
        package_name: &str,
        range: Range<u64>,
    ) -> crate::Result<Vec<u8>> {
        let name = basename(package_name);
        let Some(package) = self.packages.get(name) else {
            return Ok(Vec::new());
        };
        validate_package_range(package, &range)?;
        self.reconstruct_package_range(source, package, range).await
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

    async fn reconstruct_package_range<S: DataSource + ?Sized>(
        &self,
        source: &S,
        package: &BundlePackage,
        range: Range<u64>,
    ) -> crate::Result<Vec<u8>> {
        let output_len = usize::try_from(range.end - range.start)?;
        let mut output = vec![0u8; output_len];
        for (index, entry) in package.entries.iter().enumerate() {
            let entry_start = entry.original_archive_offset;
            let entry_end = entry_start + bundle_entry_size(package, index);
            let Some(overlap) = intersect_ranges(&range, &(entry_start..entry_end)) else {
                continue;
            };
            let bundle_name = format!("bundles.{:02}.nxa", entry.bundle_index);
            let header = self
                .headers
                .get(&bundle_name)
                .ok_or_else(|| eyre::eyre!("missing chunk header for {bundle_name}"))?;
            let source_start = u64::from(entry.start_offset) + overlap.start - entry_start;
            let source_end = source_start + overlap.end - overlap.start;
            let data = read_bundle_range_async(
                source,
                BundleRangeRequest {
                    name: &bundle_name,
                    header,
                    range: source_start..source_end,
                },
            )
            .await?;
            let output_start = usize::try_from(overlap.start - range.start)?;
            let output_end = output_start + data.len();
            output[output_start..output_end].copy_from_slice(&data);
        }
        Ok(output)
    }
}

fn validate_package_range(package: &BundlePackage, range: &Range<u64>) -> crate::Result<()> {
    if range.start > range.end || range.end > package.size {
        eyre::bail!(
            "bundle package range {}..{} exceeds package size {}",
            range.start,
            range.end,
            package.size
        );
    }
    Ok(())
}

fn intersect_ranges(left: &Range<u64>, right: &Range<u64>) -> Option<Range<u64>> {
    let start = left.start.max(right.start);
    let end = left.end.min(right.end);
    (start < end).then_some(start..end)
}

struct BundleRangeRequest<'a> {
    name: &'a str,
    header: &'a BundleChunkHeader,
    range: Range<u64>,
}

#[derive(Debug)]
struct ChunkBatch {
    compressed_range: Range<u64>,
    descriptor_indices: Vec<usize>,
}

async fn read_bundle_range_async<S: DataSource + ?Sized>(
    source: &S,
    request: BundleRangeRequest<'_>,
) -> crate::Result<Vec<u8>> {
    let batches = plan_chunk_batches(request.header, &request.range)?;
    let mut output = vec![0u8; usize::try_from(request.range.end - request.range.start)?];
    let mut copied = 0u64;
    for batch in batches {
        let compressed = source
            .read_range(
                request.name,
                batch.compressed_range.start,
                batch.compressed_range.end - batch.compressed_range.start,
            )
            .await
            .wrap_err_with(|| format!("read chunks from {}", request.name))?;
        for index in batch.descriptor_indices {
            let descriptor = &request.header.descriptors[index];
            let compressed_start =
                usize::try_from(descriptor.comp_off - batch.compressed_range.start)?;
            let compressed_end = compressed_start + descriptor.comp_sz as usize;
            let chunk = compressed
                .get(compressed_start..compressed_end)
                .ok_or_else(|| eyre::eyre!("compressed chunk {index} is out of bounds"))?;
            let decompressed = decompress_chunk(chunk, descriptor)?;
            copied +=
                copy_decompressed_overlap(&mut output, &request.range, descriptor, &decompressed)?;
        }
    }
    if copied != request.range.end - request.range.start {
        eyre::bail!("bundle range is not fully covered by chunk descriptors");
    }
    Ok(output)
}

fn plan_chunk_batches(
    header: &BundleChunkHeader,
    range: &Range<u64>,
) -> crate::Result<Vec<ChunkBatch>> {
    let mut batches = Vec::<ChunkBatch>::new();
    for (index, descriptor) in header.descriptors.iter().enumerate() {
        let chunk_end = descriptor
            .unc_off
            .checked_add(u64::from(descriptor.unc_sz))
            .ok_or_else(|| eyre::eyre!("bundle chunk range overflow"))?;
        if intersect_ranges(range, &(descriptor.unc_off..chunk_end)).is_none() {
            continue;
        }
        let compressed_end = descriptor
            .comp_off
            .checked_add(u64::from(descriptor.comp_sz))
            .ok_or_else(|| eyre::eyre!("compressed chunk range overflow"))?;
        if let Some(batch) = batches.last_mut()
            && batch.compressed_range.end == descriptor.comp_off
            && compressed_end - batch.compressed_range.start <= MAX_COALESCED_COMPRESSED_READ
        {
            batch.compressed_range.end = compressed_end;
            batch.descriptor_indices.push(index);
            continue;
        }
        batches.push(ChunkBatch {
            compressed_range: descriptor.comp_off..compressed_end,
            descriptor_indices: vec![index],
        });
    }
    Ok(batches)
}

fn copy_decompressed_overlap(
    output: &mut [u8],
    requested: &Range<u64>,
    descriptor: &crate::archive::bundle::ChunkDescriptor,
    chunk: &[u8],
) -> crate::Result<u64> {
    let chunk_range = descriptor.unc_off..descriptor.unc_off + u64::from(descriptor.unc_sz);
    let Some(overlap) = intersect_ranges(requested, &chunk_range) else {
        return Ok(0);
    };
    let chunk_start = usize::try_from(overlap.start - chunk_range.start)?;
    let output_start = usize::try_from(overlap.start - requested.start)?;
    let len = usize::try_from(overlap.end - overlap.start)?;
    let chunk_bytes = chunk
        .get(chunk_start..chunk_start + len)
        .ok_or_else(|| eyre::eyre!("decompressed chunk is shorter than declared"))?;
    output[output_start..output_start + len].copy_from_slice(chunk_bytes);
    Ok(len as u64)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::bundle::{BundleEntry, ChunkDescriptor};
    use crate::io::IoFuture;
    use std::cell::RefCell;

    struct RecordingSource {
        bundle: Vec<u8>,
        reads: RefCell<Vec<(String, u64, u64)>>,
    }

    impl DataSource for RecordingSource {
        fn read_full<'a>(&'a self, _path: &'a str) -> IoFuture<'a, Vec<u8>> {
            Box::pin(async { unreachable!("range test does not read complete files") })
        }

        fn read_range<'a>(&'a self, path: &'a str, offset: u64, len: u64) -> IoFuture<'a, Vec<u8>> {
            Box::pin(async move {
                self.reads
                    .borrow_mut()
                    .push((path.to_string(), offset, len));
                let start = usize::try_from(offset)?;
                let end = start + usize::try_from(len)?;
                Ok(self.bundle[start..end].to_vec())
            })
        }

        fn exists<'a>(&'a self, _path: &'a str) -> IoFuture<'a, bool> {
            Box::pin(async { Ok(false) })
        }

        fn list_bundle_chunks<'a>(&'a self) -> IoFuture<'a, Vec<String>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn list_packages<'a>(&'a self) -> IoFuture<'a, Vec<String>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    #[test]
    fn package_range_reads_only_intersecting_chunks_and_coalesces_neighbors() {
        let slicer = range_fixture_slicer();
        let source = RecordingSource {
            bundle: b"--abcdefghijkl".to_vec(),
            reads: RefCell::new(Vec::new()),
        };

        let output = pollster::block_on(slicer.load_package_range(&source, "archive", 2..7))
            .expect("load package range");

        assert_eq!(output, b"cdefg");
        assert_eq!(
            *source.reads.borrow(),
            vec![("bundles.00.nxa".to_string(), 2, 8)]
        );
    }

    #[test]
    fn package_range_rejects_bytes_past_package_end() {
        let slicer = range_fixture_slicer();
        let source = RecordingSource {
            bundle: b"--abcdefghijkl".to_vec(),
            reads: RefCell::new(Vec::new()),
        };

        let error = pollster::block_on(slicer.load_package_range(&source, "archive", 0..13))
            .expect_err("reject invalid range");

        assert!(error.to_string().contains("exceeds package size"));
        assert!(source.reads.borrow().is_empty());
    }

    #[test]
    fn chunk_batch_coalescing_respects_allocation_limit() {
        const TWO_MIB: u32 = 2 * 1024 * 1024;
        let descriptors = (0..3)
            .map(|index| ChunkDescriptor {
                unc_off: index * u64::from(TWO_MIB),
                comp_off: index * u64::from(TWO_MIB),
                unc_sz: TWO_MIB,
                comp_sz: TWO_MIB,
                comp_type: 0,
                chunk_type: 0,
            })
            .collect::<Vec<_>>();
        let header = BundleChunkHeader {
            descriptors,
            offsets: HashMap::new(),
        };

        let batches =
            plan_chunk_batches(&header, &(0..3 * u64::from(TWO_MIB))).expect("plan chunk batches");

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].compressed_range, 0..4 * 1024 * 1024);
        assert_eq!(
            batches[1].compressed_range,
            4 * 1024 * 1024..6 * 1024 * 1024
        );
    }

    #[test]
    #[ignore = "requires HD2_TEST_DATA_DIR or a local Helldivers 2 install"]
    fn package_ranges_match_sync_game_package_reconstruction() {
        let data_dir = std::env::var_os("HD2_TEST_DATA_DIR")
            .map(std::path::PathBuf::from)
            .expect("set HD2_TEST_DATA_DIR");
        let source = crate::io::NativeDataSource::new(&data_dir);
        let slicer = pollster::block_on(BundleSlicer::open(&source)).expect("open game bundles");
        let sync_index =
            crate::archive::BundleIndex::from_data_dir(&data_dir).expect("open sync game bundles");
        let mut names = slicer
            .packages
            .iter()
            .filter(|(name, package)| is_archive_name(name) && package.size >= 72)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        names.sort();
        names.truncate(3);
        names.extend([
            "9ba626afa44a3aa3".to_string(),
            "57cebd7e5f985d45".to_string(),
        ]);
        names.sort();
        names.dedup();
        for name in names {
            compare_package_ranges(&source, &slicer, &sync_index, &name);
        }
    }

    fn range_fixture_slicer() -> BundleSlicer {
        let package = BundlePackage {
            size: 12,
            entries: vec![BundleEntry {
                original_archive_offset: 0,
                start_offset: 100,
                bundle_index: 0,
            }],
        };
        let descriptors = (0..3)
            .map(|index| ChunkDescriptor {
                unc_off: 100 + index * 4,
                comp_off: 2 + index * 4,
                unc_sz: 4,
                comp_sz: 4,
                comp_type: 0,
                chunk_type: 0,
            })
            .collect::<Vec<_>>();
        let offsets = descriptors
            .iter()
            .enumerate()
            .map(|(index, descriptor)| (descriptor.unc_off, index as u32))
            .collect();
        BundleSlicer {
            packages: HashMap::from([("archive".to_string(), package)]),
            headers: HashMap::from([(
                "bundles.00.nxa".to_string(),
                BundleChunkHeader {
                    descriptors,
                    offsets,
                },
            )]),
        }
    }

    fn is_archive_name(name: &str) -> bool {
        name.len() == 16 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    fn compare_package_ranges(
        source: &crate::io::NativeDataSource,
        slicer: &BundleSlicer,
        sync_index: &crate::archive::BundleIndex,
        name: &str,
    ) {
        let complete = sync_index.load_package(name).expect("sync package oracle");
        let header = crate::archive::toc_only::TocHeader::parse(&complete).expect("TOC header");
        for range in sample_ranges(
            complete.len() as u64,
            header.table_size().expect("table size"),
        ) {
            let partial =
                pollster::block_on(slicer.load_package_range(source, name, range.clone()))
                    .expect("reconstruct package range");
            assert_eq!(
                partial,
                complete[range.start as usize..range.end as usize],
                "range mismatch for {name} at {range:?}"
            );
        }
    }

    fn sample_ranges(size: u64, table_size: usize) -> Vec<Range<u64>> {
        let middle_start = size / 2;
        let middle_end = middle_start.saturating_add(4096).min(size);
        vec![0..72, 72..table_size as u64, middle_start..middle_end]
    }
}
