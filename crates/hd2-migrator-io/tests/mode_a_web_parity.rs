//! Parity test: `mode_a_web::run` over `NativeDataSource` must produce
//! byte-identical patches to the synchronous `mode_a::run` for the same inputs.
//!
//! This test is **gated by environment variables** so CI/dev machines without a
//! real HD2 install simply skip it instead of failing:
//!
//! - `HD2_TEST_DATA_DIR`: path to the game `data/` directory (legacy or Slim).
//! - `HD2_TEST_PATCH`: path to the mod patch's TOC file (with `.gpu_resources`
//!   and `.stream` sidecars next to it).
//! - `HD2_TEST_TARGETS`: comma-separated archive hash list (exact hashes only,
//!   no name filters). For a 1:1 build mapping pick one hash.
//!
//! Example invocation:
//! ```text
//! HD2_TEST_DATA_DIR="C:/.../Helldivers 2/data" \
//! HD2_TEST_PATCH="C:/mods/some-mod/<hash>.patch_0" \
//! HD2_TEST_TARGETS="abcdef0123456789" \
//! cargo test -p hd2-migrator-io --test mode_a_web_parity -- --nocapture
//! ```

use hd2_migrator_io::io::NativeDataSource;
use hd2_migrator_io::migrator::{migrate_all, mode_a_web, MigrateAllOpts};
use hd2_migrator_io::padding;
use hd2_migrator_io::web::{PatchBytes, WebMigrateOptions};
use hd2_migrator_io::ArchiveIndex;
use std::path::PathBuf;

const PATCH_SUFFIX: &str = "9ba626afa44a3aa3.patch_0";

#[test]
fn mode_a_native_vs_async_parity() {
    let Some(data_dir) = std::env::var_os("HD2_TEST_DATA_DIR").map(PathBuf::from) else {
        eprintln!("[skip] HD2_TEST_DATA_DIR not set");
        return;
    };
    let Some(patch_path) = std::env::var_os("HD2_TEST_PATCH").map(PathBuf::from) else {
        eprintln!("[skip] HD2_TEST_PATCH not set");
        return;
    };
    let target_hashes: Vec<String> = std::env::var("HD2_TEST_TARGETS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    if target_hashes.is_empty() {
        eprintln!("[skip] HD2_TEST_TARGETS not set");
        return;
    }

    let native_outputs = run_native(&data_dir, &patch_path, &target_hashes);
    let web_outputs = run_web(&data_dir, &patch_path, &target_hashes);

    assert_eq!(
        native_outputs.len(),
        web_outputs.len(),
        "report count mismatch: native={} web={}",
        native_outputs.len(),
        web_outputs.len(),
    );

    for (target_hash, native_bytes) in &native_outputs {
        let web_bytes = web_outputs
            .get(target_hash)
            .unwrap_or_else(|| panic!("web result missing for target {target_hash}"));
        assert_eq!(
            native_bytes.toc, web_bytes.toc,
            "TOC bytes differ for {target_hash}"
        );
        assert_eq!(
            native_bytes.gpu, web_bytes.gpu,
            "GPU bytes differ for {target_hash}"
        );
        assert_eq!(
            native_bytes.stream, web_bytes.stream,
            "Stream bytes differ for {target_hash}"
        );
    }
}

struct ArchiveBytes {
    toc: Vec<u8>,
    gpu: Vec<u8>,
    stream: Vec<u8>,
}

fn run_native(
    data_dir: &PathBuf,
    patch_path: &PathBuf,
    target_hashes: &[String],
) -> std::collections::HashMap<String, ArchiveBytes> {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_dir = tmp.path().to_path_buf();
    let index = ArchiveIndex::builtin();
    let template = padding::builtin_template();

    let opts = MigrateAllOpts {
        patch_path: patch_path.as_path(),
        data_dir: data_dir.as_path(),
        out_dir: out_dir.as_path(),
        archive_index: index,
        source_hash: None,
        target_hashes: Some(target_hashes),
        category: "Armor",
        patch_suffix: PATCH_SUFFIX,
        empty_unit_template: Some(&template),
        padding_mode: padding::PaddingMode::Sanitized,
        armor_mapping_json: None,
        experimental_partial_remap: false,
        progress: None,
    };
    let reports = migrate_all(opts).expect("native mode_a");

    let mut out = std::collections::HashMap::new();
    for report in reports {
        let out_path = report
            .out_path
            .as_ref()
            .expect("report missing out_path")
            .clone();
        let toc = std::fs::read(&out_path).expect("read native TOC");
        let gpu = std::fs::read(format!("{}.gpu_resources", out_path.display()))
            .unwrap_or_default();
        let stream = std::fs::read(format!("{}.stream", out_path.display()))
            .unwrap_or_default();
        // The native path may merge multiple hashes under one target_name; here
        // we record bytes under each individual hash that contributed.
        for hash in report.target_hash.split(',') {
            out.insert(hash.to_string(), ArchiveBytes {
                toc: toc.clone(),
                gpu: gpu.clone(),
                stream: stream.clone(),
            });
        }
    }
    out
}

fn run_web(
    data_dir: &PathBuf,
    patch_path: &PathBuf,
    target_hashes: &[String],
) -> std::collections::HashMap<String, ArchiveBytes> {
    let patch_name = patch_path
        .file_name()
        .expect("patch filename")
        .to_string_lossy()
        .to_string();
    let toc = std::fs::read(patch_path).expect("read patch TOC");
    let gpu = std::fs::read(format!("{}.gpu_resources", patch_path.display()))
        .unwrap_or_default();
    let stream = std::fs::read(format!("{}.stream", patch_path.display()))
        .unwrap_or_default();
    let patch_bytes = PatchBytes {
        name: patch_name,
        toc,
        gpu,
        stream,
    };

    let options = WebMigrateOptions {
        source_hash: None,
        target_hashes: target_hashes.to_vec(),
        patch_suffix: Some(PATCH_SUFFIX.to_string()),
        no_padding: false,
        unmatched_unit_policy: hd2_migrator_io::web::UnmatchedUnitPolicy::Drop,
    };

    let data_source = NativeDataSource::new(data_dir);
    let results = pollster::block_on(mode_a_web::run(
        &patch_bytes,
        &options,
        &data_source,
        "Armor",
        None,
    ))
    .expect("web mode_a_web");

    let mut out = std::collections::HashMap::new();
    for mut result in results {
        let (toc, gpu, stream) = result.patch.serialize();
        out.insert(result.target_hash.clone(), ArchiveBytes { toc, gpu, stream });
    }
    out
}
