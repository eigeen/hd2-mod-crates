use crate::error::{Result, message};
use crate::fs_paths::path_to_metadata;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

/// Keeps an extracted zip package alive while callers read from its root.
pub struct PackageRoot {
    root: PathBuf,
    _temp: Option<TempDir>,
}

impl PackageRoot {
    pub fn path(&self) -> &Path {
        &self.root
    }
}

/// Opens either a directory package or a zip package.
pub fn open_package_root(package: &Path) -> Result<PackageRoot> {
    if package.is_dir() {
        return Ok(PackageRoot {
            root: package.to_path_buf(),
            _temp: None,
        });
    }
    if !package.is_file() {
        return Err(message(format!("package not found: {}", package.display())));
    }

    let temp = TempDir::new()?;
    extract_zip(package, temp.path())?;
    Ok(PackageRoot {
        root: temp.path().to_path_buf(),
        _temp: Some(temp),
    })
}

/// Writes a zip archive of `package_dir` using a portable Deflate container.
/// Use this for user-facing artifacts that third-party tools must read.
pub fn create_package_zip(package_dir: &Path, zip_path: &Path) -> Result<()> {
    write_zip(
        package_dir,
        zip_path,
        SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644),
    )
}

/// Writes a zip archive of `package_dir` using zstd per-file compression.
/// Yields ~2x smaller archives on the svd payload but requires zstd-aware readers,
/// so it's reserved for the internal `.svd` container that only svd-export opens.
pub fn create_internal_package_zip(package_dir: &Path, zip_path: &Path) -> Result<()> {
    write_zip(
        package_dir,
        zip_path,
        SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Zstd)
            .compression_level(Some(11))
            .unix_permissions(0o644),
    )
}

fn write_zip(package_dir: &Path, zip_path: &Path, options: SimpleFileOptions) -> Result<()> {
    ensure_zip_is_outside_package(package_dir, zip_path)?;
    if let Some(parent) = zip_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = File::create(zip_path)?;
    let mut zip = zip::ZipWriter::new(file);

    for path in package_files(package_dir)? {
        let name = archive_name(package_dir, &path)?;
        zip.start_file(name, options)?;
        let mut input = File::open(path)?;
        std::io::copy(&mut input, &mut zip)?;
    }
    zip.finish()?;
    Ok(())
}

fn ensure_zip_is_outside_package(package_dir: &Path, zip_path: &Path) -> Result<()> {
    if zip_path.starts_with(package_dir) {
        return Err(message("zip output must be outside the package directory"));
    }
    Ok(())
}

fn package_files(package_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_package_files(package_dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_package_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_package_files(&path, files)?;
            continue;
        }
        if entry.file_type()?.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn archive_name(package_dir: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(package_dir)?;
    path_to_metadata(relative)
}

fn extract_zip(zip_path: &Path, output: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for index in 0..archive.len() {
        extract_zip_entry(&mut archive, index, output)?;
    }
    Ok(())
}

fn extract_zip_entry<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
    output: &Path,
) -> Result<()> {
    let mut entry = archive.by_index(index)?;
    let relative = entry
        .enclosed_name()
        .ok_or_else(|| message("unsafe path inside zip package"))?;
    let path = output.join(relative);
    if entry.is_dir() {
        std::fs::create_dir_all(path)?;
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    std::io::copy(&mut entry, &mut file)?;
    file.flush()?;
    Ok(())
}
