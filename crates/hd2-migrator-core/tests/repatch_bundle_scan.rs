//! Optional real-Mod regression comparing partial bundle reads with full reconstruction.

use hd2_migrator_core::archive::BundleIndex;
use hd2_migrator_core::io::{DataSource, IoFuture, NativeDataSource};
use hd2_migrator_core::operations::{PatchBytes, UnitRepatchOptions, repatch_patch_with_progress};
use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Instant;

#[test]
#[ignore = "requires HD2_TEST_DATA_DIR and HD2_TEST_REPATCH_TOC"]
fn partial_scan_matches_full_reconstruction_for_real_mod() {
    let data_dir = configured_path("HD2_TEST_DATA_DIR");
    let patch_path = configured_path("HD2_TEST_REPATCH_TOC");
    let mut stream_path = patch_path.as_os_str().to_os_string();
    stream_path.push(".stream");
    let patch = PatchBytes {
        name: patch_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string(),
        toc: std::fs::read(&patch_path).expect("read binary patch TOC"),
        gpu: Vec::new(),
        stream: std::fs::read(stream_path).expect("read binary patch stream"),
    };
    let started = Instant::now();
    let partial = pollster::block_on(repatch_patch_with_progress(
        patch.clone(),
        UnitRepatchOptions::default(),
        &NativeDataSource::new(&data_dir),
        None,
    ))
    .expect("partial bundle scan");
    eprintln!("partial scan: {:?}", started.elapsed());
    let source = FullPackageSource {
        index: BundleIndex::from_data_dir(&data_dir).expect("open bundle index"),
        cached: RefCell::new(None),
    };
    let started = Instant::now();
    let complete = pollster::block_on(repatch_patch_with_progress(
        patch,
        UnitRepatchOptions::default(),
        &source,
        None,
    ))
    .expect("full reconstruction scan");
    eprintln!("full scan: {:?}", started.elapsed());
    assert_eq!(partial.toc, complete.toc);
    assert_eq!(
        serde_json::to_value(partial.summary).unwrap(),
        serde_json::to_value(complete.summary).unwrap(),
    );
}

fn configured_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {name}"))
}

struct FullPackageSource {
    index: BundleIndex,
    cached: RefCell<Option<(String, Vec<u8>)>>,
}

impl DataSource for FullPackageSource {
    fn read_full<'a>(&'a self, path: &'a str) -> IoFuture<'a, Vec<u8>> {
        Box::pin(async move { self.index.load_package(path) })
    }

    fn read_range<'a>(&'a self, path: &'a str, offset: u64, len: u64) -> IoFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let mut cached = self.cached.borrow_mut();
            if cached.as_ref().is_none_or(|(name, _)| name != path) {
                *cached = Some((path.to_string(), self.index.load_package(path)?));
            }
            let bytes = &cached.as_ref().expect("cached package").1;
            let end = usize::try_from(
                offset
                    .checked_add(len)
                    .ok_or_else(|| eyre::eyre!("range overflow"))?,
            )?;
            let range = bytes
                .get(usize::try_from(offset)?..end)
                .ok_or_else(|| eyre::eyre!("range exceeds reconstructed package"))?;
            Ok(range.to_vec())
        })
    }

    fn exists<'a>(&'a self, _path: &'a str) -> IoFuture<'a, bool> {
        // Present reconstructed packages through the legacy range-reading interface.
        Box::pin(async { Ok(false) })
    }

    fn list_bundle_chunks<'a>(&'a self) -> IoFuture<'a, Vec<String>> {
        Box::pin(async { eyre::bail!("full reconstruction exposes logical packages only") })
    }

    fn list_packages<'a>(&'a self) -> IoFuture<'a, Vec<String>> {
        Box::pin(async {
            Ok(self
                .index
                .packages
                .keys()
                .filter(|name| {
                    name.len() == 16 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                .cloned()
                .collect())
        })
    }
}
