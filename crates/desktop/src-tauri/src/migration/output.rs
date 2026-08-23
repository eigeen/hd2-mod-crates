use hd2_migrator_io::archive::toc_only::TocOnlyPackage;
use hd2_migrator_io::archive::{SerializedPart, StreamToc};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub struct OutputZip {
    writer: ZipWriter<NamedTempFile>,
    output_path: PathBuf,
}

pub trait OutputProgress: Sync {
    fn ensure_active(&self) -> io::Result<()>;
    fn report_bytes(&self, completed: u64, total: u64) -> io::Result<()>;
}

pub struct PatchZipContext<'a> {
    pub directory: &'a str,
    pub progress: Option<&'a dyn OutputProgress>,
    pub suffix: &'a str,
}

pub fn create_zip(path: &Path) -> Result<OutputZip, String> {
    let parent = path.parent().filter(|value| !value.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Create output directory {}: {error}", parent.display()))?;
    }
    let temporary = NamedTempFile::new_in(parent.unwrap_or_else(|| Path::new(".")))
        .map_err(|error| format!("Create temporary output ZIP: {error}"))?;
    Ok(OutputZip {
        writer: ZipWriter::new(temporary),
        output_path: path.to_path_buf(),
    })
}

pub fn write_zip_entry_with_progress(
    zip: &mut OutputZip,
    path: &str,
    bytes: &[u8],
    progress: Option<&dyn OutputProgress>,
) -> hd2_migrator_io::Result<()> {
    let normalized = path.replace('\\', "/");
    write_zip_content(
        zip,
        ZipEntryContext {
            path: &normalized,
            progress,
            total: bytes.len() as u64,
        },
        |writer| writer.write_all(bytes).map_err(Into::into),
    )
}

#[derive(Clone, Copy)]
pub enum RepatchTocSource<'a> {
    Original(&'a [u8]),
    #[allow(dead_code)]
    Rebuilt(&'a TocOnlyPackage),
}

pub fn write_repatch_toc_to_zip(
    zip: &mut OutputZip,
    path: &str,
    source: RepatchTocSource<'_>,
    progress: Option<&dyn OutputProgress>,
) -> hd2_migrator_io::Result<()> {
    let total = match source {
        RepatchTocSource::Original(bytes) => bytes.len(),
        RepatchTocSource::Rebuilt(package) => package.serialized_len(),
    } as u64;
    write_zip_content(
        zip,
        ZipEntryContext {
            path,
            progress,
            total,
        },
        |writer| match source {
            RepatchTocSource::Original(bytes) => writer.write_all(bytes).map_err(Into::into),
            RepatchTocSource::Rebuilt(package) => package.write_to(writer),
        },
    )
}

pub fn write_patch_to_zip(
    zip: &mut OutputZip,
    patch: &mut StreamToc,
    context: PatchZipContext<'_>,
) -> hd2_migrator_io::Result<()> {
    let serializer = patch.serializer();
    let toc_path = format!("{}/{}", context.directory, context.suffix);
    write_serialized_entry(
        zip,
        &serializer,
        entry_context(&toc_path, SerializedPart::Toc, context.progress),
    )?;
    let gpu_path = format!("{toc_path}.gpu_resources");
    write_serialized_entry(
        zip,
        &serializer,
        entry_context(&gpu_path, SerializedPart::Gpu, context.progress),
    )?;
    let stream_path = format!("{toc_path}.stream");
    write_serialized_entry(
        zip,
        &serializer,
        entry_context(&stream_path, SerializedPart::Stream, context.progress),
    )
}

struct SerializedEntryContext<'a> {
    part: SerializedPart,
    path: &'a str,
    progress: Option<&'a dyn OutputProgress>,
}

fn entry_context<'a>(
    path: &'a str,
    part: SerializedPart,
    progress: Option<&'a dyn OutputProgress>,
) -> SerializedEntryContext<'a> {
    SerializedEntryContext {
        part,
        path,
        progress,
    }
}

fn write_serialized_entry(
    zip: &mut OutputZip,
    serializer: &hd2_migrator_io::archive::StreamTocSerializer<'_>,
    context: SerializedEntryContext<'_>,
) -> hd2_migrator_io::Result<()> {
    let total = serializer.part_len(context.part) as u64;
    write_zip_content(
        zip,
        ZipEntryContext {
            path: context.path,
            progress: context.progress,
            total,
        },
        |writer| serializer.write_part(context.part, writer),
    )
}

struct ZipEntryContext<'a> {
    path: &'a str,
    progress: Option<&'a dyn OutputProgress>,
    total: u64,
}

fn write_zip_content<F>(
    zip: &mut OutputZip,
    context: ZipEntryContext<'_>,
    write: F,
) -> hd2_migrator_io::Result<()>
where
    F: FnOnce(&mut ProgressWriter<'_, ZipWriter<NamedTempFile>>) -> hd2_migrator_io::Result<()>,
{
    validate_entry_path(context.path)?;
    ensure_active(context.progress)?;
    report_bytes(context.progress, 0, context.total)?;
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.writer.start_file(context.path, options)?;
    let mut writer = ProgressWriter::new(&mut zip.writer, context.progress, context.total);
    write(&mut writer)?;
    report_bytes(context.progress, writer.completed, context.total)?;
    Ok(())
}

const WRITE_CHUNK_SIZE: usize = 1024 * 1024;
const PROGRESS_INTERVAL: u64 = 4 * 1024 * 1024;

struct ProgressWriter<'a, W> {
    completed: u64,
    inner: &'a mut W,
    next_report: u64,
    progress: Option<&'a dyn OutputProgress>,
    total: u64,
}

impl<'a, W> ProgressWriter<'a, W> {
    fn new(inner: &'a mut W, progress: Option<&'a dyn OutputProgress>, total: u64) -> Self {
        Self {
            completed: 0,
            inner,
            next_report: PROGRESS_INTERVAL,
            progress,
            total,
        }
    }
}

impl<W: Write> Write for ProgressWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        ensure_active(self.progress)?;
        let count = self
            .inner
            .write(&bytes[..bytes.len().min(WRITE_CHUNK_SIZE)])?;
        self.completed += count as u64;
        if self.completed >= self.next_report {
            report_bytes(self.progress, self.completed, self.total)?;
            self.next_report = self.completed + PROGRESS_INTERVAL;
        }
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn ensure_active(progress: Option<&dyn OutputProgress>) -> io::Result<()> {
    match progress {
        Some(progress) => progress.ensure_active(),
        None => Ok(()),
    }
}

fn report_bytes(
    progress: Option<&dyn OutputProgress>,
    completed: u64,
    total: u64,
) -> io::Result<()> {
    match progress {
        Some(progress) => progress.report_bytes(completed, total),
        None => Ok(()),
    }
}

pub fn finish_zip(zip: OutputZip) -> Result<(), String> {
    let OutputZip {
        writer,
        output_path,
    } = zip;
    let temporary = writer
        .finish()
        .map_err(|error| format!("Finish output ZIP: {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Flush output ZIP: {error}"))?;
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
    use std::fs::File;
    use std::io::Read;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestProgress {
        cancel_after_report: bool,
        cancelled: AtomicBool,
    }

    impl TestProgress {
        fn active() -> Self {
            Self {
                cancel_after_report: false,
                cancelled: AtomicBool::new(false),
            }
        }
    }

    impl OutputProgress for TestProgress {
        fn ensure_active(&self) -> io::Result<()> {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(io::Error::other("task cancelled"));
            }
            Ok(())
        }

        fn report_bytes(&self, completed: u64, _total: u64) -> io::Result<()> {
            if self.cancel_after_report && completed >= PROGRESS_INTERVAL {
                self.cancelled.store(true, Ordering::Release);
            }
            Ok(())
        }
    }

    #[test]
    fn writes_stored_zip_entry() {
        let directory = tempfile::tempdir().expect("temp directory");
        let output = directory.path().join("output.zip");
        let mut zip = create_zip(&output).expect("create ZIP");
        write_zip_entry_with_progress(&mut zip, "variant/example.patch_0", b"toc", None)
            .expect("write entry");
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
    fn writes_empty_zip_entry() {
        let directory = tempfile::tempdir().expect("temp directory");
        let output = directory.path().join("output.zip");
        let mut zip = create_zip(&output).expect("create ZIP");
        write_zip_entry_with_progress(&mut zip, "example.patch_0.stream", b"", None)
            .expect("write empty entry");
        finish_zip(zip).expect("finish ZIP");

        let file = File::open(output).expect("open ZIP");
        let mut archive = zip::ZipArchive::new(file).expect("read ZIP");
        let entry = archive.by_index(0).expect("empty entry");
        assert_eq!(entry.name(), "example.patch_0.stream");
        assert_eq!(entry.size(), 0);
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
        write_zip_entry_with_progress(&mut zip, "replacement.patch_0", b"new", None)
            .expect("write entry");

        finish_zip(zip).expect("replace ZIP");

        let file = File::open(output).expect("open replacement ZIP");
        let mut archive = zip::ZipArchive::new(file).expect("read replacement ZIP");
        assert_eq!(
            archive.by_index(0).expect("entry").name(),
            "replacement.patch_0"
        );
    }

    #[test]
    fn replacing_a_larger_output_leaves_no_trailing_bytes() {
        let directory = tempfile::tempdir().expect("temp directory");
        let output = directory.path().join("output.zip");
        std::fs::write(&output, vec![0x5a; 1024 * 1024]).expect("write larger old output");
        let mut zip = create_zip(&output).expect("create ZIP");
        write_zip_entry_with_progress(&mut zip, "replacement.patch_0", b"new", None)
            .expect("write entry");

        finish_zip(zip).expect("replace ZIP");

        let bytes = std::fs::read(&output).expect("read replacement ZIP");
        assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06");
        let mut archive = zip::ZipArchive::new(File::open(output).expect("open replacement ZIP"))
            .expect("read replacement ZIP");
        assert_eq!(archive.len(), 1);
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
        let temporary_path = zip
            .writer
            .get_ref()
            .expect("open temporary ZIP")
            .path()
            .to_path_buf();
        write_zip_entry_with_progress(&mut zip, "partial.patch_0", b"partial", None)
            .expect("write entry");

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
        let progress = TestProgress::active();

        write_patch_to_zip(
            &mut zip,
            &mut patch,
            PatchZipContext {
                directory: "variant",
                progress: Some(&progress),
                suffix: "example.patch_0",
            },
        )
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

    #[test]
    fn checks_cancellation_between_output_chunks() {
        let progress = TestProgress {
            cancel_after_report: true,
            cancelled: AtomicBool::new(false),
        };
        let mut output = Vec::new();
        let mut writer = ProgressWriter::new(&mut output, Some(&progress), 8 * 1024 * 1024);

        let error = writer
            .write_all(&vec![1; 8 * 1024 * 1024])
            .expect_err("cancel write");

        assert_eq!(error.to_string(), "task cancelled");
        assert_eq!(output.len() as u64, PROGRESS_INTERVAL);
    }

    #[test]
    fn streams_a_rebuilt_repatch_toc_into_the_zip() {
        let directory = tempfile::tempdir().expect("temp directory");
        let output = directory.path().join("repatch.zip");
        let mut source = sample_patch();
        let (toc, _, _) = source.serialize();
        let package = TocOnlyPackage::parse(&toc).expect("parse TOC-only");
        let expected = package.serialize().expect("serialize expected TOC");
        let progress = TestProgress::active();
        let mut zip = create_zip(&output).expect("create ZIP");

        write_repatch_toc_to_zip(
            &mut zip,
            "example.patch_0",
            RepatchTocSource::Rebuilt(&package),
            Some(&progress),
        )
        .expect("stream rebuilt TOC");
        finish_zip(zip).expect("finish ZIP");

        let mut archive =
            zip::ZipArchive::new(File::open(output).expect("open ZIP")).expect("read ZIP");
        assert_eq!(read_zip_entry(&mut archive, "example.patch_0"), expected);
    }

    fn sample_patch() -> StreamToc {
        let mut entry = TocEntry::new(1, 2);
        entry.toc_data = b"toc body".to_vec();
        entry.gpu_data = b"gpu body".to_vec().into();
        entry.stream_data = b"stream body".to_vec().into();
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
