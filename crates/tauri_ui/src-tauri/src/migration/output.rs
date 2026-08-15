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

pub fn finish_zip(zip: OutputZip) -> Result<(), String> {
    let OutputZip {
        writer,
        temporary,
        output_path,
    } = zip;
    writer
        .finish()
        .map_err(|error| format!("Finish output ZIP: {error}"))?;
    remove_existing_output(&output_path)?;
    temporary
        .persist(&output_path)
        .map(|_| ())
        .map_err(|error| format!("Move completed ZIP to {}: {error}", output_path.display()))
}

fn remove_existing_output(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(path)
        .map_err(|error| format!("Replace existing output ZIP {}: {error}", path.display()))
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
}
