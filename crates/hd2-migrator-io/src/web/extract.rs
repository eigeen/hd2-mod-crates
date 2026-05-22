use crate::archive::{BundleIndex, dsar};
use crate::constants::{DSAR_MAGIC, LEGACY_MAGIC};
use crate::index::ArchiveIndex;
use crate::web::metadata::{WebArchiveMetadata, WebGameMetadata};
use byteorder::{ByteOrder, LittleEndian as LE};
use eyre::WrapErr;
use std::path::Path;

pub struct ExtractMetadataOptions<'a> {
    pub data_dir: &'a Path,
    pub archive_index: &'a ArchiveIndex,
    pub category: &'a str,
}

pub fn extract_game_metadata(
    options: ExtractMetadataOptions<'_>,
) -> crate::Result<WebGameMetadata> {
    let bundle_index = load_bundle_index(options.data_dir)?;
    let targets = options
        .archive_index
        .category(options.category)
        .ok_or_else(|| eyre::eyre!("category {:?} not found", options.category))?;
    let archives = targets
        .iter()
        .map(|target| read_target_metadata(options.data_dir, bundle_index.as_ref(), target))
        .collect::<crate::Result<Vec<_>>>()?;
    Ok(WebGameMetadata::new(options.category, archives))
}

fn load_bundle_index(data_dir: &Path) -> crate::Result<Option<BundleIndex>> {
    let bundle_toc = data_dir.join("bundles.nxa");
    if !bundle_toc.exists() {
        return Ok(None);
    }
    BundleIndex::from_data_dir(data_dir).map(Some)
}

fn read_target_metadata(
    data_dir: &Path,
    bundle_index: Option<&BundleIndex>,
    target: &crate::index::ArmorEntry,
) -> crate::Result<WebArchiveMetadata> {
    let path = data_dir.join(&target.hash);
    let toc_bytes = read_toc_bytes(&path, bundle_index)
        .wrap_err_with(|| format!("read archive metadata {} ({})", target.name, target.hash))?;
    WebArchiveMetadata::from_toc_bytes(
        target.hash.clone(),
        target.name.clone(),
        &toc_bytes,
    )
}

fn read_toc_bytes(path: &Path, bundle_index: Option<&BundleIndex>) -> crate::Result<Vec<u8>> {
    if path.exists() {
        return read_standalone_toc(path);
    }
    let Some(index) = bundle_index else {
        eyre::bail!("file not found: {}", path.display());
    };
    let name = path.to_str().ok_or_else(|| eyre::eyre!("non-UTF8 path"))?;
    index.load_package(name)
}

fn read_standalone_toc(path: &Path) -> crate::Result<Vec<u8>> {
    let bytes = std::fs::read(path).wrap_err_with(|| format!("read {}", path.display()))?;
    match magic(&bytes) {
        Some(LEGACY_MAGIC) => Ok(bytes),
        Some(DSAR_MAGIC) => dsar::decompress(&bytes),
        Some(value) => Err(crate::error::MigratorError::BadMagic {
            expected: LEGACY_MAGIC,
            got: value,
        }
        .into()),
        None => eyre::bail!("file too short to detect kind: {}", path.display()),
    }
}

fn magic(bytes: &[u8]) -> Option<u32> {
    bytes.get(0..4).map(LE::read_u32)
}
