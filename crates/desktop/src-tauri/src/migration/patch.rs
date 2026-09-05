use hd2_migrator_core::archive::sidecar::patch_sidecar_requirements;
use hd2_migrator_core::operations::PatchBytes;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const GPU_SUFFIX: &str = ".gpu_resources";
const STREAM_SUFFIX: &str = ".stream";

pub struct LoadedPatch {
    bytes: PatchBytes,
}

pub struct PatchInspectionInput {
    descriptor: PatchDescriptor,
    bytes: PatchBytes,
}

struct ResolvedPatchFiles {
    toc: PathBuf,
    gpu: Option<PathBuf>,
    stream: Option<PathBuf>,
}

struct PatchMetadata {
    files: ResolvedPatchFiles,
    name: String,
    original_name: Option<String>,
    toc: Vec<u8>,
    gpu_len: usize,
    stream_len: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchDescriptor {
    path: String,
    name: String,
    original_name: Option<String>,
    byte_length: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPatchFile {
    path: String,
    name: String,
    relative_path: String,
    size: u64,
    root_id: usize,
    source_root: String,
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
}

impl PatchInspectionInput {
    pub fn bytes(&self) -> &PatchBytes {
        &self.bytes
    }

    pub fn descriptor(&self) -> PatchDescriptor {
        self.descriptor.clone()
    }
}

impl PatchMetadata {
    fn read(selected_paths: &[PathBuf]) -> Result<Self, String> {
        let files = ResolvedPatchFiles::resolve(selected_paths)?;
        let name = file_name(&files.toc)?;
        let toc = read_required(&files.toc, "patch TOC")?;
        let gpu_len = optional_file_len(files.gpu.as_deref())?;
        let stream_len = optional_file_len(files.stream.as_deref())?;
        validate_sidecars(&toc, gpu_len, stream_len, &name)?;
        Ok(Self {
            original_name: original_name(&files.toc),
            files,
            name,
            toc,
            gpu_len,
            stream_len,
        })
    }

    fn descriptor(&self) -> PatchDescriptor {
        PatchDescriptor {
            path: self.files.toc.display().to_string(),
            name: self.name.clone(),
            original_name: self.original_name.clone(),
            byte_length: self.toc.len() + self.gpu_len + self.stream_len,
        }
    }

    fn into_inspection_input(self) -> PatchInspectionInput {
        let descriptor = self.descriptor();
        PatchInspectionInput {
            descriptor,
            bytes: PatchBytes {
                name: self.name,
                toc: self.toc,
                gpu: Vec::new(),
                stream: Vec::new(),
            },
        }
    }

    fn load(self) -> Result<LoadedPatch, String> {
        let gpu = read_optional(self.files.gpu.as_deref())?;
        let stream = read_optional(self.files.stream.as_deref())?;
        validate_sidecars(&self.toc, gpu.len(), stream.len(), &self.name)?;
        Ok(LoadedPatch {
            bytes: PatchBytes {
                name: self.name,
                toc: self.toc,
                gpu,
                stream,
            },
        })
    }
}

impl ResolvedPatchFiles {
    fn resolve(selected_paths: &[PathBuf]) -> Result<Self, String> {
        let files = collect_files(selected_paths)?;
        let toc = select_toc_path(&files)?;
        let gpu = sibling_or_selected(&toc, &files, GPU_SUFFIX);
        let stream = sibling_or_selected(&toc, &files, STREAM_SUFFIX);
        Ok(Self { toc, gpu, stream })
    }
}

pub fn load_patch(selected_paths: &[PathBuf]) -> Result<LoadedPatch, String> {
    PatchMetadata::read(selected_paths)?.load()
}

/// Read only the TOC while validating sidecar sizes from filesystem metadata.
pub fn inspect_patch_metadata(selected_paths: &[PathBuf]) -> Result<PatchInspectionInput, String> {
    Ok(PatchMetadata::read(selected_paths)?.into_inspection_input())
}

pub fn describe_patch_metadata(selected_paths: &[PathBuf]) -> Result<PatchDescriptor, String> {
    Ok(PatchMetadata::read(selected_paths)?.descriptor())
}

pub fn discover_patch_files(paths: &[PathBuf]) -> Result<Vec<DiscoveredPatchFile>, String> {
    if paths.is_empty() {
        return Err("Choose a patch file or directory".to_owned());
    }
    let mut discovered = Vec::new();
    for (root_id, path) in paths.iter().enumerate() {
        discovered.extend(discover_root_files(path, root_id)?);
    }
    Ok(discovered)
}

fn discover_root_files(path: &Path, root_id: usize) -> Result<Vec<DiscoveredPatchFile>, String> {
    if path.is_file() {
        let source_root = path.parent().unwrap_or_else(|| Path::new(""));
        return Ok(vec![discovered_file(path, source_root, root_id)?]);
    }
    if !path.is_dir() {
        return Err(format!("Patch path does not exist: {}", path.display()));
    }
    let mut files = WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_type().is_file() => {
                Some(discovered_file(entry.path(), path, root_id))
            }
            Ok(_) => None,
            Err(error) => Some(Err(format!("Read patch directory failed: {error}"))),
        })
        .collect::<Result<Vec<_>, _>>()?;
    files.sort_by(|left, right| {
        left.relative_path
            .to_lowercase()
            .cmp(&right.relative_path.to_lowercase())
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    Ok(files)
}

fn discovered_file(
    path: &Path,
    source_root: &Path,
    root_id: usize,
) -> Result<DiscoveredPatchFile, String> {
    let name = file_name(path)?;
    let relative_path = path
        .strip_prefix(source_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let size = std::fs::metadata(path)
        .map_err(|error| format!("Read patch file metadata {}: {error}", path.display()))?
        .len();
    Ok(DiscoveredPatchFile {
        path: path.display().to_string(),
        name,
        relative_path,
        size,
        root_id,
        source_root: source_root.display().to_string(),
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
    let required =
        patch_sidecar_requirements(toc).map_err(|error| format!("Parse {name}: {error}"))?;
    validate_sidecar_len(name, GPU_SUFFIX, required.gpu, gpu_len)?;
    validate_sidecar_len(name, STREAM_SUFFIX, required.stream, stream_len)
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

fn optional_file_len(path: Option<&Path>) -> Result<usize, String> {
    let Some(path) = path else {
        return Ok(0);
    };
    let len = std::fs::metadata(path)
        .map_err(|error| format!("Read patch sidecar metadata {}: {error}", path.display()))?
        .len();
    usize::try_from(len).map_err(|_| format!("Patch sidecar is too large: {}", path.display()))
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
    use hd2_migrator_core::archive::TocFileType;
    use hd2_migrator_core::archive::toc_only::TocOnlyPackage;

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

    #[test]
    fn accepts_empty_sidecar_intervals_at_aligned_offsets() {
        let directory = tempfile::tempdir().expect("temp directory");
        let toc_path = directory.path().join("example.patch_0");
        std::fs::write(&toc_path, sidecar_toc(198_525_248, 0, 1_398_144, 0)).expect("write TOC");

        let patch = load_patch(&[toc_path]).expect("load patch");

        assert!(patch.bytes().gpu.is_empty());
        assert!(patch.bytes().stream.is_empty());
    }

    #[test]
    fn rejects_non_empty_sidecar_interval_beyond_file() {
        let directory = tempfile::tempdir().expect("temp directory");
        let toc_path = directory.path().join("example.patch_0");
        std::fs::write(&toc_path, sidecar_toc(0, 0, 16, 1)).expect("write TOC");

        let error = load_patch(&[toc_path])
            .err()
            .expect("short stream must fail");

        assert!(error.contains("requires 17 bytes, found 0"));
    }

    #[test]
    fn describes_sidecars_without_loading_their_contents() {
        let directory = tempfile::tempdir().expect("temp directory");
        let toc_path = directory.path().join("example.patch_0");
        std::fs::write(&toc_path, sidecar_toc(0, 3, 0, 6)).expect("write TOC");
        std::fs::write(
            toc_path.with_file_name("example.patch_0.gpu_resources"),
            b"gpu",
        )
        .expect("write GPU");
        std::fs::write(toc_path.with_file_name("example.patch_0.stream"), b"stream")
            .expect("write stream");

        let descriptor = describe_patch_metadata(&[toc_path]).expect("describe patch");

        assert_eq!(descriptor.byte_length, sidecar_toc(0, 3, 0, 6).len() + 9);
    }

    #[test]
    fn discovers_roots_in_input_order_and_directory_files_case_insensitively() {
        let directory = tempfile::tempdir().expect("temp directory");
        let nested = directory.path().join("nested");
        std::fs::create_dir(&nested).expect("create nested directory");
        std::fs::write(nested.join("b.patch"), b"b").expect("write b");
        std::fs::write(nested.join("A.patch"), b"a").expect("write A");
        let direct = directory.path().join("direct.patch");
        std::fs::write(&direct, b"direct").expect("write direct");
        let nested_source_root = nested.display().to_string();

        let files = discover_patch_files(&[direct, nested]).expect("discover files");

        assert_eq!(files[0].root_id, 0);
        assert_eq!(files[0].relative_path, "direct.patch");
        assert_eq!(files[0].source_root, directory.path().display().to_string());
        assert_eq!(files[1].relative_path, "A.patch");
        assert_eq!(files[2].relative_path, "b.patch");
        assert_eq!(files[1].root_id, 1);
        assert_eq!(files[1].source_root, nested_source_root);
    }

    fn sidecar_toc(
        gpu_offset: u64,
        gpu_size: u32,
        stream_offset: u64,
        stream_size: u32,
    ) -> Vec<u8> {
        let type_id = hd2_migrator_core::constants::UNIT_ID;
        TocOnlyPackage {
            types: vec![TocFileType::new(type_id, 1)],
            entries: vec![hd2_migrator_core::archive::toc_only::TocOnlyEntry {
                file_id: 42,
                type_id,
                unknown1: 0,
                unknown2: 0,
                unknown3: 16,
                unknown4: 64,
                toc_data: Vec::new(),
                stream_offset,
                gpu_offset,
                stream_size,
                gpu_size,
            }],
            unknown: 0,
            unk4_data: [0; 56],
        }
        .serialize()
        .expect("serialize TOC")
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
