use super::{BundleSlicer, DataSource};
use crate::archive::StreamToc;

/// Open the optional Slim-install bundle index used by archive readers.
pub(crate) async fn open_bundle<S: DataSource + ?Sized>(
    source: &S,
) -> crate::Result<Option<BundleSlicer>> {
    if !source.exists("bundles.nxa").await? {
        return Ok(None);
    }
    BundleSlicer::open(source).await.map(Some)
}

/// Load a complete archive from either a legacy or Slim game-data directory.
pub(crate) async fn load_archive<S: DataSource + ?Sized>(
    source: &S,
    bundle: Option<&BundleSlicer>,
    archive_name: &str,
) -> crate::Result<StreamToc> {
    let (toc, gpu, stream) = load_archive_parts(source, bundle, archive_name).await?;
    StreamToc::from_buffers(&toc, &gpu, &stream, archive_name.to_owned())
}

async fn load_archive_parts<S: DataSource + ?Sized>(
    source: &S,
    bundle: Option<&BundleSlicer>,
    archive_name: &str,
) -> crate::Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    if let Some(bundle) = bundle
        && should_load_bundle(source, bundle, archive_name).await?
    {
        return bundle.load_triple(source, archive_name).await;
    }
    load_legacy_parts(source, archive_name).await
}

async fn should_load_bundle<S: DataSource + ?Sized>(
    source: &S,
    bundle: &BundleSlicer,
    archive_name: &str,
) -> crate::Result<bool> {
    let exists_on_disk = source.exists(archive_name).await?;
    Ok(!exists_on_disk && bundle.has_package(archive_name))
}

async fn load_legacy_parts<S: DataSource + ?Sized>(
    source: &S,
    archive_name: &str,
) -> crate::Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let toc = source.read_full(archive_name).await?;
    let gpu = read_optional(source, &format!("{archive_name}.gpu_resources")).await?;
    let stream = read_optional(source, &format!("{archive_name}.stream")).await?;
    Ok((toc, gpu, stream))
}

async fn read_optional<S: DataSource + ?Sized>(source: &S, path: &str) -> crate::Result<Vec<u8>> {
    if !source.exists(path).await? {
        return Ok(Vec::new());
    }
    source.read_full(path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::UNIT_ID;
    use crate::io::NativeDataSource;
    use crate::unit::authority::ArmorMappingTable;
    use std::path::PathBuf;

    #[test]
    fn configured_game_archive_contains_both_body_units() {
        let Some(data_dir) = configured_data_dir() else {
            return;
        };
        let source = NativeDataSource::new(data_dir);
        let archive = pollster::block_on(load_test_archive(&source)).expect("load game archive");
        let mappings = ArmorMappingTable::bundled().unwrap();
        let mapping = mappings.armor("A-9 Helljumper").unwrap();
        for label in ["slim body", "stocky body"] {
            assert!(archive_has_unit(&archive, mapping.get(label).unwrap()));
        }
    }

    async fn load_test_archive(source: &NativeDataSource) -> crate::Result<StreamToc> {
        let bundle = open_bundle(source).await?;
        load_archive(source, bundle.as_ref(), "1d6dc4216e7ce52d").await
    }

    fn configured_data_dir() -> Option<PathBuf> {
        std::env::var_os("HD2_GAME_DATA_DIR").map(PathBuf::from)
    }

    fn archive_has_unit(archive: &StreamToc, unit_id: u64) -> bool {
        archive
            .entries
            .iter()
            .any(|entry| entry.type_id == UNIT_ID && entry.file_id == unit_id)
    }
}
