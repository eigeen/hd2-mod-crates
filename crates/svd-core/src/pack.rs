use crate::archive::create_internal_package_zip;
use crate::error::{Result, message};
use crate::fs_paths::join_metadata_path;
use crate::hash::content_hash;
use crate::logical_path::logical_file_key;
use crate::metadata::{
    AssetRecord, FileOperation, FileRecord, METADATA_FILE, PackageMetadata, SCHEMA_VERSION,
    VariantMetadata,
};
use crate::pack_progress::PackProgress;
use crate::patch::{compress_full, create_patch};
use crate::scan::{SourceFile, SourceMod, SourceVariant, scan_source};
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PackOptions {
    pub input: PathBuf,
    pub base: String,
    pub output: PathBuf,
    pub zip_output: Option<PathBuf>,
    pub level: i32,
    pub jobs: Option<usize>,
}

/// Builds a directory package from a source mod root.
pub fn pack(options: &PackOptions) -> Result<()> {
    prepare_output_dir(&options.output)?;
    let source = scan_source(&options.input)?;
    let base = find_base_variant(&source, &options.base)?.clone();
    let base_files = logical_file_map(&base.files)?;
    let progress = PackProgress::new(pack_work_units(&source, &base, &base_files)?);

    let mut builder = ThreadPoolBuilder::new();
    if let Some(jobs) = options.jobs {
        builder = builder.num_threads(jobs);
    }
    let pool = builder.build()?;
    pool.install(|| pack_with_pool(options, source, base, base_files, progress))?;
    write_package_zip(options)
}

fn pack_with_pool(
    options: &PackOptions,
    source: SourceMod,
    base: SourceVariant,
    base_files: BTreeMap<String, IndexedFile>,
    progress: PackProgress,
) -> Result<()> {
    copy_assets(&source.assets, &options.output, &progress)?;
    copy_base_files(&base.files, &options.output, &progress)?;

    let assets = build_asset_records(&source.assets, &progress)?;
    let context = PackContext {
        base: &base,
        base_files: &base_files,
        options,
        progress: &progress,
    };
    let variants = build_variant_records(&source, &context)?;
    let metadata = PackageMetadata {
        schema_version: SCHEMA_VERSION,
        mod_info: source.mod_info,
        base_variant: options.base.clone(),
        variants,
        assets,
        manifest_options: source.manifest_options,
    };

    write_metadata(&metadata, &options.output)?;
    progress.inc();
    progress.finish();
    Ok(())
}

fn write_package_zip(options: &PackOptions) -> Result<()> {
    let Some(zip_output) = &options.zip_output else {
        return Ok(());
    };
    create_internal_package_zip(&options.output, zip_output)
}

fn prepare_output_dir(output: &Path) -> Result<()> {
    if output.exists() && output.read_dir()?.next().is_some() {
        return Err(message(format!(
            "output directory must be empty: {}",
            output.display()
        )));
    }
    std::fs::create_dir_all(output)?;
    Ok(())
}

fn find_base_variant<'a>(source: &'a SourceMod, base: &str) -> Result<&'a SourceVariant> {
    source
        .variants
        .iter()
        .find(|variant| variant.name == base)
        .ok_or_else(|| message(format!("base variant not found: {base}")))
}

#[derive(Debug, Clone)]
struct IndexedFile {
    relative_path: String,
    path: PathBuf,
}

struct PackContext<'a> {
    base: &'a SourceVariant,
    base_files: &'a BTreeMap<String, IndexedFile>,
    options: &'a PackOptions,
    progress: &'a PackProgress,
}

fn logical_file_map(files: &[SourceFile]) -> Result<BTreeMap<String, IndexedFile>> {
    let mut map = BTreeMap::new();
    for file in files {
        let key = logical_file_key(&file.relative_path);
        let indexed = IndexedFile {
            relative_path: file.relative_path.clone(),
            path: file.path.clone(),
        };
        if map.insert(key.clone(), indexed).is_some() {
            return Err(message(format!("duplicate logical file key: {key}")));
        }
    }
    Ok(map)
}

fn pack_work_units(
    source: &SourceMod,
    base: &SourceVariant,
    base_files: &BTreeMap<String, IndexedFile>,
) -> Result<u64> {
    let mut units = source.assets.len() + base.files.len() + source.assets.len() + 1;
    for variant in &source.variants {
        units += variant_work_units(variant, base, base_files)?;
    }
    Ok(units as u64)
}

fn variant_work_units(
    variant: &SourceVariant,
    base: &SourceVariant,
    base_files: &BTreeMap<String, IndexedFile>,
) -> Result<usize> {
    if variant.name == base.name {
        return Ok(base.files.len());
    }
    let target_files = logical_file_map(&variant.files)?;
    Ok(union_paths(base_files, &target_files).len())
}

fn copy_assets(assets: &[SourceFile], output: &Path, progress: &PackProgress) -> Result<()> {
    progress.set_message("copying assets");
    assets.par_iter().try_for_each(|asset| {
        let out_path = join_metadata_path(&output.join("assets"), &asset.relative_path)?;
        copy_file(&asset.path, &out_path)?;
        progress.inc();
        Ok(())
    })
}

fn copy_base_files(files: &[SourceFile], output: &Path, progress: &PackProgress) -> Result<()> {
    progress.set_message("copying base files");
    files.par_iter().try_for_each(|file| {
        let out_path = join_metadata_path(&output.join("base"), &file.relative_path)?;
        copy_file(&file.path, &out_path)?;
        progress.inc();
        Ok(())
    })
}

fn copy_file(input: &Path, output: &Path) -> Result<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(input, output)?;
    Ok(())
}

fn build_asset_records(assets: &[SourceFile], progress: &PackProgress) -> Result<Vec<AssetRecord>> {
    progress.set_message("hashing assets");
    assets
        .par_iter()
        .map(|asset| {
            let record = asset_record(asset)?;
            progress.inc();
            Ok(record)
        })
        .collect()
}

fn asset_record(asset: &SourceFile) -> Result<AssetRecord> {
    let bytes = std::fs::read(&asset.path)?;
    Ok(AssetRecord {
        relative_path: asset.relative_path.clone(),
        size: bytes.len() as u64,
        hash: content_hash(&bytes),
    })
}

fn build_variant_records(
    source: &SourceMod,
    context: &PackContext<'_>,
) -> Result<Vec<VariantMetadata>> {
    let mut variants = Vec::new();
    for variant in &source.variants {
        let files = if variant.name == context.base.name {
            build_base_records(&context.base.files, context.progress)?
        } else {
            build_target_records(variant, context)?
        };
        variants.push(VariantMetadata {
            name: variant.name.clone(),
            files,
        });
    }
    Ok(variants)
}

fn build_base_records(files: &[SourceFile], progress: &PackProgress) -> Result<Vec<FileRecord>> {
    progress.set_message("indexing base variant");
    files
        .par_iter()
        .map(|file| {
            let record = base_record(file)?;
            progress.inc();
            Ok(record)
        })
        .collect()
}

fn base_record(file: &SourceFile) -> Result<FileRecord> {
    let bytes = std::fs::read(&file.path)?;
    Ok(FileRecord {
        relative_path: file.relative_path.clone(),
        base_relative_path: None,
        operation: FileOperation::Base,
        source_size: None,
        target_size: Some(bytes.len() as u64),
        base_hash: None,
        target_hash: Some(content_hash(&bytes)),
        stored_path: Some(format!("base/{}", file.relative_path)),
        zstd: None,
    })
}

fn build_target_records(
    variant: &SourceVariant,
    context: &PackContext<'_>,
) -> Result<Vec<FileRecord>> {
    let target_files = logical_file_map(&variant.files)?;
    let paths: Vec<_> = union_paths(context.base_files, &target_files)
        .into_iter()
        .collect();

    context
        .progress
        .set_message(format!("packing {}", variant.name));
    paths
        .par_iter()
        .map(|key| {
            let record = build_file_record(
                variant,
                context.base_files.get(key),
                target_files.get(key),
                context.options,
            )?;
            context.progress.inc();
            Ok(record)
        })
        .collect()
}

fn union_paths(
    base_files: &BTreeMap<String, IndexedFile>,
    target_files: &BTreeMap<String, IndexedFile>,
) -> BTreeSet<String> {
    base_files
        .keys()
        .chain(target_files.keys())
        .cloned()
        .collect()
}

fn build_file_record(
    variant: &SourceVariant,
    base_file: Option<&IndexedFile>,
    target_file: Option<&IndexedFile>,
    options: &PackOptions,
) -> Result<FileRecord> {
    match (base_file, target_file) {
        (Some(base_file), Some(target_file)) => {
            diff_existing_file(variant, base_file, target_file, options)
        }
        (None, Some(target_file)) => store_full_file(variant, target_file, options),
        (Some(base_file), None) => delete_record(base_file),
        (None, None) => Err(message("file path vanished during packing")),
    }
}

fn diff_existing_file(
    variant: &SourceVariant,
    base_file: &IndexedFile,
    target_file: &IndexedFile,
    options: &PackOptions,
) -> Result<FileRecord> {
    let base = std::fs::read(&base_file.path)?;
    let target = std::fs::read(&target_file.path)?;
    let base_hash = content_hash(&base);
    let target_hash = content_hash(&target);
    let base_relative_path = base_relative_path(base_file, target_file);

    if base == target {
        return same_as_base_record(target_file, base_relative_path, &base, target_hash);
    }
    if target.is_empty() {
        return empty_record(
            &target_file.relative_path,
            base_relative_path,
            Some(&base),
            target_hash,
        );
    }

    let (patch, params) = create_patch(&base, &target, options.level)?;
    let stored_path = format!(
        "patches/{variant}/{relative_path}.zstpatch",
        variant = variant.name,
        relative_path = target_file.relative_path
    );
    write_payload(&options.output, &stored_path, &patch)?;

    Ok(FileRecord {
        relative_path: target_file.relative_path.clone(),
        base_relative_path,
        operation: FileOperation::Patch,
        source_size: Some(base.len() as u64),
        target_size: Some(target.len() as u64),
        base_hash: Some(base_hash),
        target_hash: Some(target_hash),
        stored_path: Some(stored_path),
        zstd: Some(params),
    })
}

fn same_as_base_record(
    target_file: &IndexedFile,
    base_relative_path: Option<String>,
    base: &[u8],
    target_hash: String,
) -> Result<FileRecord> {
    Ok(FileRecord {
        relative_path: target_file.relative_path.clone(),
        base_relative_path,
        operation: FileOperation::SameAsBase,
        source_size: Some(base.len() as u64),
        target_size: Some(base.len() as u64),
        base_hash: Some(content_hash(base)),
        target_hash: Some(target_hash),
        stored_path: None,
        zstd: None,
    })
}

fn store_full_file(
    variant: &SourceVariant,
    target_file: &IndexedFile,
    options: &PackOptions,
) -> Result<FileRecord> {
    let target = std::fs::read(&target_file.path)?;
    let target_hash = content_hash(&target);
    if target.is_empty() {
        return empty_record(&target_file.relative_path, None, None, target_hash);
    }

    let payload = compress_full(&target, options.level)?;
    let stored_path = format!(
        "full/{variant}/{relative_path}.zst",
        variant = variant.name,
        relative_path = target_file.relative_path
    );
    write_payload(&options.output, &stored_path, &payload)?;

    Ok(FileRecord {
        relative_path: target_file.relative_path.clone(),
        base_relative_path: None,
        operation: FileOperation::Full,
        source_size: None,
        target_size: Some(target.len() as u64),
        base_hash: None,
        target_hash: Some(target_hash),
        stored_path: Some(stored_path),
        zstd: None,
    })
}

fn empty_record(
    relative_path: &str,
    base_relative_path: Option<String>,
    base: Option<&[u8]>,
    target_hash: String,
) -> Result<FileRecord> {
    Ok(FileRecord {
        relative_path: relative_path.to_owned(),
        base_relative_path,
        operation: FileOperation::Empty,
        source_size: base.map(|bytes| bytes.len() as u64),
        target_size: Some(0),
        base_hash: base.map(content_hash),
        target_hash: Some(target_hash),
        stored_path: None,
        zstd: None,
    })
}

fn delete_record(base_file: &IndexedFile) -> Result<FileRecord> {
    let base = std::fs::read(&base_file.path)?;
    Ok(FileRecord {
        relative_path: base_file.relative_path.clone(),
        base_relative_path: None,
        operation: FileOperation::Delete,
        source_size: Some(base.len() as u64),
        target_size: None,
        base_hash: Some(content_hash(&base)),
        target_hash: None,
        stored_path: None,
        zstd: None,
    })
}

fn write_payload(root: &Path, relative_path: &str, bytes: &[u8]) -> Result<()> {
    let path = join_metadata_path(root, relative_path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn base_relative_path(base_file: &IndexedFile, target_file: &IndexedFile) -> Option<String> {
    if base_file.relative_path == target_file.relative_path {
        return None;
    }
    Some(base_file.relative_path.clone())
}

fn write_metadata(metadata: &PackageMetadata, output: &Path) -> Result<()> {
    let path = output.join(METADATA_FILE);
    let bytes = serde_json::to_vec_pretty(metadata)?;
    std::fs::write(path, bytes)?;
    Ok(())
}
