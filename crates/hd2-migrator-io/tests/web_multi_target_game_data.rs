//! End-to-end coverage for the large web fixture against an installed game.

use hd2_migrator_io::io::NativeDataSource;
use hd2_migrator_io::web::{
    ParallelVariantPatchCallbacks, PatchBytes, UnmatchedUnitPolicy, VariantMigrationCallbacks,
    VariantPatchOutput, WebMigrationMapping, WebMigrationVariant, WebOutputFile,
    WebUnifiedMigrateOptions, inspect_equipment_with_source, list_equipment_options,
    migrate_variants_to_patch_sink_parallel, migrate_variants_to_sink,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const DEFAULT_VARIANT_COUNT: usize = 20;
const PATCH_NAME: &str = "9ba626afa44a3aa3.patch_0";

#[test]
#[ignore = "requires HD2_TEST_DATA_DIR or a local Helldivers 2 install"]
fn streams_twenty_large_patch_variants_without_retaining_outputs() {
    let data_dir = game_data_dir();
    let source = NativeDataSource::new(&data_dir);
    let patch = load_large_fixture();
    let variant_count = requested_variant_count();
    let options = pollster::block_on(migration_options(&patch, &source, variant_count));
    let mut output_paths = HashSet::new();
    let mut output_bytes = 0usize;

    let callbacks = VariantMigrationCallbacks::new(None, |file: WebOutputFile| {
        output_bytes += file.bytes.len();
        assert!(output_paths.insert(file.path));
        Ok(())
    });
    let summary = pollster::block_on(migrate_variants_to_sink(patch, options, &source, callbacks))
        .expect("migrate streamed web variants");

    assert_eq!(summary.migrated_count, variant_count);
    assert_eq!(output_paths.len(), variant_count * 3);
    assert!(output_bytes > 0);
}

#[test]
#[ignore = "requires HD2_TEST_DATA_DIR or a local Helldivers 2 install"]
fn computes_twenty_large_patch_variants_with_rayon() {
    let data_dir = game_data_dir();
    let source = NativeDataSource::new(&data_dir);
    let patch = load_large_fixture();
    let variant_count = requested_variant_count();
    let options = pollster::block_on(migration_options(&patch, &source, variant_count));
    let mut output_bytes = 0usize;
    let mut output_count = 0usize;

    let callbacks = ParallelVariantPatchCallbacks::new(None, |mut output: VariantPatchOutput| {
        let (toc, gpu, stream) = output.patch.serialize();
        output_bytes += toc.len() + gpu.len() + stream.len();
        output_count += 1;
        Ok(())
    });
    let summary = pollster::block_on(migrate_variants_to_patch_sink_parallel(
        patch, options, &source, callbacks,
    ))
    .expect("migrate Rayon variants");

    assert_eq!(summary.migrated_count, variant_count);
    assert_eq!(output_count, variant_count);
    assert!(output_bytes > 0);
}

async fn migration_options(
    patch: &PatchBytes,
    source: &NativeDataSource,
    variant_count: usize,
) -> WebUnifiedMigrateOptions {
    let inspection = inspect_equipment_with_source(patch, source)
        .await
        .expect("inspect large fixture");
    let detected = inspection
        .sources
        .into_iter()
        .find(|candidate| candidate.resolved_hash.is_some())
        .expect("resolved equipment source");
    let source_hash = detected.resolved_hash.expect("resolved source hash");
    let targets = list_equipment_options()
        .expect("equipment options")
        .into_iter()
        .filter(|target| target.category == detected.category)
        .filter(|target| !target.excluded && target.hash != source_hash)
        .take(variant_count)
        .collect::<Vec<_>>();
    assert_eq!(targets.len(), variant_count);

    WebUnifiedMigrateOptions {
        variants: targets
            .into_iter()
            .map(|target| WebMigrationVariant {
                mappings: vec![WebMigrationMapping {
                    category: detected.category,
                    source_hash: source_hash.clone(),
                    target_hash: target.hash,
                }],
            })
            .collect(),
        patch_suffix: None,
        no_padding: false,
        unmatched_unit_policy: UnmatchedUnitPolicy::Keep,
        unit_behavior: Default::default(),
    }
}

fn game_data_dir() -> PathBuf {
    std::env::var_os("HD2_TEST_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Helldivers 2\data")
        })
}

fn load_large_fixture() -> PatchBytes {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("test_files")
        .join("SSD'S Stylized Dune 15086 0.1 2026-08-13T05-50Z IzUPRhJHc");
    PatchBytes {
        name: PATCH_NAME.to_string(),
        toc: read_patch_file(&fixture, ""),
        gpu: read_patch_file(&fixture, ".gpu_resources"),
        stream: read_patch_file(&fixture, ".stream"),
    }
}

fn read_patch_file(fixture: &Path, suffix: &str) -> Vec<u8> {
    std::fs::read(fixture.join(format!("{PATCH_NAME}{suffix}"))).expect("read fixture file")
}

fn requested_variant_count() -> usize {
    std::env::var("HD2_E2E_VARIANT_COUNT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_VARIANT_COUNT)
}
