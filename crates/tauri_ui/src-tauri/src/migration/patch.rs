use hd2_migrator_io::archive::toc_only::TocOnlyPackage;
use hd2_migrator_io::web::PatchBytes;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const GPU_SUFFIX: &str = ".gpu_resources";
const STREAM_SUFFIX: &str = ".stream";

pub struct LoadedPatch {
    path: PathBuf,
    original_name: Option<String>,
    bytes: PatchBytes,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchDescriptor {
    path: String,
    name: String,
    original_name: Option<String>,
    byte_length: usize,
}

impl LoadedPatch {
    pub fn name(&self) -> &str {
        &self.bytes.name
    }

    pub fn bytes(&self) -> &PatchBytes {
        &self.bytes
    }

    pub fn into_bytes(self) -> PatchBytes {
        self.bytes
    }

    pub fn descriptor(&self) -> PatchDescriptor {
        PatchDescriptor {
            path: self.path.display().to_string(),
            name: self.bytes.name.clone(),
            original_name: self.original_name.clone(),
            byte_length: self.bytes.toc.len() + self.bytes.gpu.len() + self.bytes.stream.len(),
        }
    }
}

pub fn load_patch(selected_paths: &[PathBuf]) -> Result<LoadedPatch, String> {
    let files = collect_files(selected_paths)?;
    let toc_path = select_toc_path(&files)?;
    let name = file_name(&toc_path)?;
    let gpu_path = sibling_or_selected(&toc_path, &files, GPU_SUFFIX);
    let stream_path = sibling_or_selected(&toc_path, &files, STREAM_SUFFIX);
    let toc = read_required(&toc_path, "patch TOC")?;
    let gpu = read_optional(gpu_path.as_deref())?;
    let stream = read_optional(stream_path.as_deref())?;
    validate_sidecars(&toc, gpu.len(), stream.len(), &name)?;
    Ok(LoadedPatch {
        original_name: original_name(&toc_path),
        path: toc_path,
        bytes: PatchBytes {
            name,
            toc,
            gpu,
            stream,
        },
    })
}

fn collect_files(selected_paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    if selected_paths.is_empty() {
        return Err("Choose a patch file or directory".to_owned());
    }
    let mut files = BTreeSet::new();
    for path in selected_paths {
        collect_path_files(path, &mut files)?;
    }
    Ok(files.into_iter().collect())
}

fn collect_path_files(path: &Path, files: &mut BTreeSet<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        files.insert(path.to_path_buf());
        return Ok(());
    }
    if !path.is_dir() {
        return Err(format!("Patch path does not exist: {}", path.display()));
    }
    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|error| format!("Read patch directory failed: {error}"))?;
        if entry.file_type().is_file() {
            files.insert(entry.into_path());
        }
    }
    Ok(())
}

fn select_toc_path(files: &[PathBuf]) -> Result<PathBuf, String> {
    let candidates: Vec<&PathBuf> = files.iter().filter(is_toc_candidate).collect();
    match candidates.as_slice() {
        [path] => Ok((*path).to_path_buf()),
        [] => Err("No patch TOC file was found".to_owned()),
        _ => Err("Multiple patch TOC files were found; choose one patch at a time".to_owned()),
    }
}

fn is_toc_candidate(path: &&PathBuf) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    !name.ends_with(GPU_SUFFIX) && !name.ends_with(STREAM_SUFFIX)
}

fn sibling_or_selected(toc: &Path, files: &[PathBuf], suffix: &str) -> Option<PathBuf> {
    let expected = format!("{}{}", toc.file_name()?.to_str()?, suffix);
    files
        .iter()
        .find(|path| path.file_name().and_then(|value| value.to_str()) == Some(expected.as_str()))
        .cloned()
        .or_else(|| sibling_if_file(toc, expected))
}

fn sibling_if_file(toc: &Path, expected: String) -> Option<PathBuf> {
    let sibling = toc.with_file_name(expected);
    sibling.is_file().then_some(sibling)
}

fn validate_sidecars(
    toc: &[u8],
    gpu_len: usize,
    stream_len: usize,
    name: &str,
) -> Result<(), String> {
    let package = TocOnlyPackage::parse(toc).map_err(|error| format!("Parse {name}: {error}"))?;
    let required_gpu = package
        .entries
        .iter()
        .map(|entry| entry.gpu_offset + u64::from(entry.gpu_size))
        .max()
        .unwrap_or(0);
    let required_stream = package
        .entries
        .iter()
        .map(|entry| entry.stream_offset + u64::from(entry.stream_size))
        .max()
        .unwrap_or(0);
    validate_sidecar_len(name, GPU_SUFFIX, required_gpu, gpu_len)?;
    validate_sidecar_len(name, STREAM_SUFFIX, required_stream, stream_len)
}

fn validate_sidecar_len(
    name: &str,
    suffix: &str,
    required: u64,
    actual: usize,
) -> Result<(), String> {
    if required <= actual as u64 {
        return Ok(());
    }
    Err(format!(
        "{name}{suffix} is missing or too small: requires {required} bytes, found {actual}"
    ))
}

fn read_required(path: &Path, label: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("Read {label} {}: {error}", path.display()))
}

fn read_optional(path: Option<&Path>) -> Result<Vec<u8>, String> {
    match path {
        Some(value) => read_required(value, "patch sidecar"),
        None => Ok(Vec::new()),
    }
}

fn file_name(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("Patch filename is not valid UTF-8: {}", path.display()))
}

fn original_name(path: &Path) -> Option<String> {
    path.parent()?.file_name()?.to_str().map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hd2_migrator_io::archive::toc_only::TocOnlyPackage;

    #[test]
    fn loads_toc_and_automatically_discovers_sidecars() {
        let directory = tempfile::tempdir().expect("temp directory");
        let toc_path = directory.path().join("example.patch_0");
        let toc = empty_toc();
        std::fs::write(&toc_path, toc).expect("write TOC");

        let patch = load_patch(std::slice::from_ref(&toc_path)).expect("load patch");

        assert_eq!(patch.name(), "example.patch_0");
        assert!(patch.bytes().gpu.is_empty());
        assert!(patch.bytes().stream.is_empty());
    }

    #[test]
    fn rejects_multiple_toc_candidates() {
        let directory = tempfile::tempdir().expect("temp directory");
        let first = directory.path().join("first.patch_0");
        let second = directory.path().join("second.patch_0");
        std::fs::write(&first, empty_toc()).expect("write first TOC");
        std::fs::write(&second, empty_toc()).expect("write second TOC");

        let error = load_patch(&[first, second])
            .err()
            .expect("multiple TOCs must fail");

        assert!(error.contains("Multiple patch TOC files"));
    }

    fn empty_toc() -> Vec<u8> {
        TocOnlyPackage {
            types: Vec::new(),
            entries: Vec::new(),
            unknown: 0,
            unk4_data: [0; 56],
        }
        .serialize()
        .expect("serialize empty TOC")
    }
}
