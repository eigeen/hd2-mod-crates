//! Bounded, ordered prefetching of bundled archive directories.

use crate::archive::toc_only::{TocEntryLocation, TocHeader, parse_entry_locations};
use crate::io::{BundleSlicer, DataSource};
use futures_util::{Stream, StreamExt, stream};

const TOC_HEADER_SIZE: u64 = 72;
pub(super) const DIRECTORY_PREFETCH_LIMIT: usize = 8;

#[derive(Debug)]
pub(super) struct BundledDirectory {
    pub(super) package: String,
    pub(super) package_size: u64,
    pub(super) contents: DirectoryContents,
}

#[derive(Debug)]
pub(super) enum DirectoryContents {
    Parsed(Vec<TocEntryLocation>),
    Invalid(eyre::Report),
}

/// Start at most [`DIRECTORY_PREFETCH_LIMIT`] directory reads and retain package order.
pub(super) fn prefetch_directories<'a, S: DataSource + ?Sized>(
    source: &'a S,
    slicer: &'a BundleSlicer,
    packages: &'a [String],
) -> impl Stream<Item = crate::Result<BundledDirectory>> + 'a {
    stream::iter(
        packages
            .iter()
            .map(|package| read_directory(source, slicer, package)),
    )
    .buffered(DIRECTORY_PREFETCH_LIMIT)
}

async fn read_directory<S: DataSource + ?Sized>(
    source: &S,
    slicer: &BundleSlicer,
    package: &str,
) -> crate::Result<BundledDirectory> {
    let Some(package_size) = slicer.packages.get(package).map(|metadata| metadata.size) else {
        return Ok(invalid_directory(
            package,
            0,
            eyre::eyre!("bundled package is missing"),
        ));
    };
    if package_size < TOC_HEADER_SIZE {
        return Ok(invalid_directory(
            package,
            package_size,
            eyre::eyre!("TOC is too small: {package_size} bytes"),
        ));
    }
    let prefix = slicer
        .load_package_range(source, package, 0..TOC_HEADER_SIZE)
        .await?;
    let header = match TocHeader::parse(&prefix) {
        Ok(header) => header,
        Err(error) => return Ok(invalid_directory(package, package_size, error)),
    };
    let table_size = match header
        .table_size()
        .and_then(|size| Ok(u64::try_from(size)?))
    {
        Ok(size) => size,
        Err(error) => return Ok(invalid_directory(package, package_size, error)),
    };
    if table_size > package_size {
        return Ok(invalid_directory(
            package,
            package_size,
            eyre::eyre!("TOC header table is truncated"),
        ));
    }
    let mut table = prefix;
    if table_size > TOC_HEADER_SIZE {
        let tail = slicer
            .load_package_range(source, package, TOC_HEADER_SIZE..table_size)
            .await?;
        table.extend_from_slice(&tail);
    }
    let locations = match parse_entry_locations(&table, &header) {
        Ok(locations) => locations,
        Err(error) => return Ok(invalid_directory(package, package_size, error)),
    };
    Ok(BundledDirectory {
        package: package.to_string(),
        package_size,
        contents: DirectoryContents::Parsed(locations),
    })
}

fn invalid_directory(package: &str, package_size: u64, error: eyre::Report) -> BundledDirectory {
    BundledDirectory {
        package: package.to_string(),
        package_size,
        contents: DirectoryContents::Invalid(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::bundle::{BundleChunkHeader, BundleEntry, BundlePackage, ChunkDescriptor};
    use crate::archive::toc_only::TocOnlyPackage;
    use crate::io::IoFuture;
    use futures_util::pin_mut;
    use std::cell::Cell;
    use std::collections::{HashMap, HashSet};
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    #[test]
    fn directory_prefetch_is_concurrent_bounded_and_ordered() {
        let fixture = bundled_fixture(12);
        let source = PendingSource::new(fixture.bytes, vec![1; 12], HashSet::new());

        let outputs = pollster::block_on(async {
            let directories = prefetch_directories(&source, &fixture.slicer, &fixture.packages);
            pin_mut!(directories);
            let mut outputs = Vec::new();
            while let Some(directory) = directories.next().await {
                outputs.push(directory.expect("read directory").package);
            }
            outputs
        });

        assert!(source.saw_pending.get());
        assert_eq!(source.max_active.get(), DIRECTORY_PREFETCH_LIMIT);
        assert_eq!(outputs, fixture.packages);
    }

    #[test]
    fn later_ready_error_does_not_overtake_earlier_directory() {
        let fixture = bundled_fixture(2);
        let second_offset = u64::try_from(fixture.toc_size).unwrap();
        let source = PendingSource::new(fixture.bytes, vec![3, 0], HashSet::from([second_offset]));

        pollster::block_on(async {
            let directories = prefetch_directories(&source, &fixture.slicer, &fixture.packages);
            pin_mut!(directories);
            let first = directories
                .next()
                .await
                .expect("first output")
                .expect("first directory succeeds");
            let second = directories
                .next()
                .await
                .expect("second output")
                .expect_err("second directory fails");

            assert_eq!(first.package, fixture.packages[0]);
            assert!(format!("{second:?}").contains("injected range failure"));
        });
        assert!(source.saw_pending.get());
        assert_eq!(source.max_active.get(), 2);
    }

    #[test]
    fn stopping_after_first_drops_pending_reads_and_ignores_buffered_error() {
        let fixture = bundled_fixture(3);
        let second_offset = u64::try_from(fixture.toc_size).unwrap();
        let source = PendingSource::new(
            fixture.bytes,
            vec![3, 0, 100],
            HashSet::from([second_offset]),
        );

        pollster::block_on(async {
            let directories = prefetch_directories(&source, &fixture.slicer, &fixture.packages);
            pin_mut!(directories);
            let first = directories
                .next()
                .await
                .expect("first output")
                .expect("first directory succeeds");

            assert_eq!(first.package, fixture.packages[0]);
        });

        assert_eq!(source.total_started.get(), 3);
        assert_eq!(source.active.get(), 0);
    }

    struct BundledFixture {
        slicer: BundleSlicer,
        packages: Vec<String>,
        bytes: Vec<u8>,
        toc_size: usize,
    }

    fn bundled_fixture(package_count: usize) -> BundledFixture {
        let toc = TocOnlyPackage {
            types: Vec::new(),
            entries: Vec::new(),
            unknown: 0,
            unk4_data: [0; 56],
        }
        .serialize()
        .expect("serialize empty TOC");
        let toc_size = toc.len();
        let packages = (0..package_count)
            .map(|index| format!("{index:016x}"))
            .collect::<Vec<_>>();
        let package_map = packages
            .iter()
            .enumerate()
            .map(|(index, package)| {
                let offset = u64::try_from(index * toc_size).unwrap();
                (
                    package.clone(),
                    BundlePackage {
                        size: toc_size as u64,
                        entries: vec![BundleEntry {
                            original_archive_offset: 0,
                            start_offset: u32::try_from(offset).unwrap(),
                            bundle_index: 0,
                        }],
                    },
                )
            })
            .collect();
        let descriptors = (0..package_count)
            .map(|index| {
                let offset = u64::try_from(index * toc_size).unwrap();
                ChunkDescriptor {
                    unc_off: offset,
                    comp_off: offset,
                    unc_sz: toc_size as u32,
                    comp_sz: toc_size as u32,
                    comp_type: 0,
                    chunk_type: 0,
                }
            })
            .collect();
        BundledFixture {
            slicer: BundleSlicer {
                packages: package_map,
                headers: HashMap::from([(
                    "bundles.00.nxa".to_string(),
                    BundleChunkHeader {
                        descriptors,
                        offsets: HashMap::new(),
                    },
                )]),
            },
            packages,
            bytes: toc.repeat(package_count),
            toc_size,
        }
    }

    struct PendingSource {
        bytes: Vec<u8>,
        pending_polls: Vec<usize>,
        error_offsets: HashSet<u64>,
        active: Cell<usize>,
        max_active: Cell<usize>,
        total_started: Cell<usize>,
        saw_pending: Cell<bool>,
    }

    impl PendingSource {
        fn new(bytes: Vec<u8>, pending_polls: Vec<usize>, error_offsets: HashSet<u64>) -> Self {
            Self {
                bytes,
                pending_polls,
                error_offsets,
                active: Cell::new(0),
                max_active: Cell::new(0),
                total_started: Cell::new(0),
                saw_pending: Cell::new(false),
            }
        }
    }

    impl DataSource for PendingSource {
        fn read_full<'a>(&'a self, _path: &'a str) -> IoFuture<'a, Vec<u8>> {
            Box::pin(async { unreachable!("directory fixture uses range reads") })
        }

        fn read_range<'a>(
            &'a self,
            _path: &'a str,
            offset: u64,
            len: u64,
        ) -> IoFuture<'a, Vec<u8>> {
            let index = usize::try_from(offset).unwrap() / self.bytes_per_package();
            Box::pin(PendingRange {
                source: self,
                offset,
                len,
                remaining_pending_polls: self.pending_polls[index],
                active: false,
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

    impl PendingSource {
        fn bytes_per_package(&self) -> usize {
            self.bytes.len() / self.pending_polls.len()
        }
    }

    struct PendingRange<'a> {
        source: &'a PendingSource,
        offset: u64,
        len: u64,
        remaining_pending_polls: usize,
        active: bool,
    }

    impl Future for PendingRange<'_> {
        type Output = crate::Result<Vec<u8>>;

        fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
            if !self.active {
                self.active = true;
                self.source
                    .total_started
                    .set(self.source.total_started.get() + 1);
                let active = self.source.active.get() + 1;
                self.source.active.set(active);
                self.source
                    .max_active
                    .set(self.source.max_active.get().max(active));
            }
            if self.remaining_pending_polls > 0 {
                self.remaining_pending_polls -= 1;
                self.source.saw_pending.set(true);
                context.waker().wake_by_ref();
                return Poll::Pending;
            }
            self.finish();
            if self.source.error_offsets.contains(&self.offset) {
                return Poll::Ready(Err(eyre::eyre!("injected range failure")));
            }
            let start = usize::try_from(self.offset).unwrap();
            let end = start + usize::try_from(self.len).unwrap();
            Poll::Ready(Ok(self.source.bytes[start..end].to_vec()))
        }
    }

    impl PendingRange<'_> {
        fn finish(&mut self) {
            if self.active {
                self.active = false;
                self.source.active.set(self.source.active.get() - 1);
            }
        }
    }

    impl Drop for PendingRange<'_> {
        fn drop(&mut self) {
            self.finish();
        }
    }
}
