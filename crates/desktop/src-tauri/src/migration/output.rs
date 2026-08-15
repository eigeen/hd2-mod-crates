use hd2_migrator_io::archive::{SerializedPart, StreamToc};
use std::fs::File;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub struct OutputZip {
    writer: ZipWriter<File>,
    temporary: NamedTempFile,
    output_path: PathBuf,
}

pub fn create_zip(path: &Path) -> Result<OutputZip, String> {
    let parent = path.parent().filter(|value| !value.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Create output directory {}: {error}", parent.display()))?;
    }
    let temporary = NamedTempFile::new_in(parent.unwrap_or_else(|| Path::new(".")))
        .map_err(|error| format!("Create temporary output ZIP: {error}"))?;
    let file = temporary
        .reopen()
        .map_err(|error| format!("Open temporary output ZIP: {error}"))?;
    Ok(OutputZip {
        writer: ZipWriter::new(file),
        temporary,
        output_path: path.to_path_buf(),
    })
}

pub fn write_zip_entry(
    zip: &mut OutputZip,
    path: &str,
    bytes: &[u8],
) -> hd2_migrator_io::Result<()> {
    validate_entry_path(path)?;
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.writer.start_file(path.replace('\\', "/"), options)?;
    zip.writer.write_all(bytes)?;
    Ok(())
}

pub fn write_patch_to_zip(
    zip: &mut OutputZip,
    patch: &mut StreamToc,
    directory: &str,
    suffix: &str,
) -> hd2_migrator_io::Result<()> {
    let serializer = patch.serializer();
    let toc_path = format!("{directory}/{suffix}");
    write_serialized_entry(zip, &serializer, &toc_path, SerializedPart::Toc)?;
    let gpu_path = format!("{toc_path}.gpu_resources");
    write_serialized_entry(zip, &serializer, &gpu_path, SerializedPart::Gpu)?;
    let stream_path = format!("{toc_path}.stream");
    write_serialized_entry(zip, &serializer, &stream_path, SerializedPart::Stream)
}

fn write_serialized_entry(
    zip: &mut OutputZip,
    serializer: &hd2_migrator_io::archive::StreamTocSerializer<'_>,
    path: &str,
    part: SerializedPart,
) -> hd2_migrator_io::Result<()> {
    validate_entry_path(path)?;
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.writer.start_file(path, options)?;
    serializer.write_part(part, &mut zip.writer)
}

pub fn finish_zip(zip: OutputZip) -> Result<(), String> {
    let OutputZip {
        writer,
        temporary,
        output_path,
    } = zip;
    let completed_file = writer
        .finish()
        .map_err(|error| format!("Finish output ZIP: {error}"))?;
    completed_file
        .sync_all()
        .map_err(|error| format!("Flush output ZIP: {error}"))?;
    drop(completed_file);
    temporary
        .persist(&output_path)
        .map(|_| ())
        .map_err(|error| format!("Move completed ZIP to {}: {error}", output_path.display()))
}

fn validate_entry_path(value: &str) -> hd2_migrator_io::Result<()> {
    let path = Path::new(value);
    if value.is_empty() || path.is_absolute() {
        eyre::bail!("invalid output ZIP entry path {value:?}");
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        eyre::bail!("unsafe output ZIP entry path {value:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hd2_migrator_io::archive::TocEntry;
    use std::io::Read;

    #[test]
    fn writes_stored_zip_entry() {
        let directory = tempfile::tempdir().expect("temp directory");
        let output = directory.path().join("output.zip");
        let mut zip = create_zip(&output).expect("create ZIP");
        write_zip_entry(&mut zip, "variant/example.patch_0", b"toc").expect("write entry");
        finish_zip(zip).expect("finish ZIP");

        let file = File::open(output).expect("open ZIP");
        let mut archive = zip::ZipArchive::new(file).expect("read ZIP");
        assert_eq!(archive.len(), 1);
        assert_eq!(
            archive.by_index(0).expect("entry").name(),
            "variant/example.patch_0"
        );
    }

    #[test]
    fn rejects_parent_path_components() {
        assert!(validate_entry_path("../outside").is_err());
    }

    #[test]
    fn atomically_replaces_an_existing_output() {
        let directory = tempfile::tempdir().expect("temp directory");
        let output = directory.path().join("output.zip");
        std::fs::write(&output, b"previous valid output").expect("write previous output");
        let mut zip = create_zip(&output).expect("create ZIP");
        write_zip_entry(&mut zip, "replacement.patch_0", b"new").expect("write entry");

        finish_zip(zip).expect("replace ZIP");

        let file = File::open(output).expect("open replacement ZIP");
        let mut archive = zip::ZipArchive::new(file).expect("read replacement ZIP");
        assert_eq!(
            archive.by_index(0).expect("entry").name(),
            "replacement.patch_0"
        );
    }

    #[test]
    fn dropping_an_incomplete_zip_removes_its_temporary_file() {
        let directory = tempfile::tempdir().expect("temp directory");
        let output = directory.path().join("output.zip");
        let mut zip = create_zip(&output).expect("create ZIP");
        let temporary_path = zip.temporary.path().to_path_buf();
        write_zip_entry(&mut zip, "partial.patch_0", b"partial").expect("write entry");

        drop(zip);

        assert!(!temporary_path.exists());
        assert!(!output.exists());
    }

    #[test]
    fn streams_archive_parts_directly_into_zip_entries() {
        let directory = tempfile::tempdir().expect("temp directory");
        let output = directory.path().join("output.zip");
        let mut patch = sample_patch();
        let (expected_toc, expected_gpu, expected_stream) = patch.clone().serialize();
        let mut zip = create_zip(&output).expect("create ZIP");

        write_patch_to_zip(&mut zip, &mut patch, "variant", "example.patch_0")
            .expect("stream patch");
        finish_zip(zip).expect("finish ZIP");

        let mut archive =
            zip::ZipArchive::new(File::open(output).expect("open ZIP")).expect("read ZIP");
        assert_eq!(
            read_zip_entry(&mut archive, "variant/example.patch_0"),
            expected_toc
        );
        assert_eq!(
            read_zip_entry(&mut archive, "variant/example.patch_0.gpu_resources"),
            expected_gpu
        );
        assert_eq!(
            read_zip_entry(&mut archive, "variant/example.patch_0.stream"),
            expected_stream
        );
    }

    fn sample_patch() -> StreamToc {
        let mut entry = TocEntry::new(1, 2);
        entry.toc_data = b"toc body".to_vec();
        entry.gpu_data = b"gpu body".to_vec();
        entry.stream_data = b"stream body".to_vec();
        StreamToc {
            entries: vec![entry],
            ..StreamToc::default()
        }
    }

    fn read_zip_entry(archive: &mut zip::ZipArchive<File>, name: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        archive
            .by_name(name)
            .expect("ZIP entry")
            .read_to_end(&mut bytes)
            .expect("read entry");
        bytes
    }
}
