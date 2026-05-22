use crate::error::{Result, message};
use crate::fs_paths::{path_to_metadata, validate_relative_path, validate_variant_name};
use crate::manifest::{MANIFEST_NAME, SourceManifest, mod_info, read_manifest, stored_manifest_options};
use crate::metadata::{ModInfo, StoredManifestOption};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SourceMod {
    pub mod_info: ModInfo,
    pub variants: Vec<SourceVariant>,
    pub assets: Vec<SourceFile>,
    pub manifest_options: Vec<StoredManifestOption>,
}

#[derive(Debug, Clone)]
pub struct SourceVariant {
    pub name: String,
    pub path: PathBuf,
    pub files: Vec<SourceFile>,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub relative_path: String,
    pub path: PathBuf,
}

/// Scans a source mod folder using manifest options when available.
pub fn scan_source(root: &Path) -> Result<SourceMod> {
    if !root.is_dir() {
        return Err(message(format!(
            "input is not a directory: {}",
            root.display()
        )));
    }

    let manifest = read_manifest(root)?;
    let variant_names = discover_variant_names(root, manifest.as_ref())?;
    let variants = scan_variants(root, &variant_names)?;
    let assets = scan_assets(root)?;

    Ok(SourceMod {
        mod_info: mod_info(manifest.as_ref()),
        variants,
        assets,
        manifest_options: stored_manifest_options(manifest.as_ref()),
    })
}

fn discover_variant_names(root: &Path, manifest: Option<&SourceManifest>) -> Result<Vec<String>> {
    let names = match manifest.and_then(|value| value.options.as_ref()) {
        Some(options) => options
            .iter()
            .flat_map(|option| {
                let _ = (&option.name, &option.description);
                option.include.iter().cloned()
            })
            .collect(),
        None => root_directory_names(root)?,
    };

    validate_variant_names(names)
}

fn root_directory_names(root: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

fn validate_variant_names(names: Vec<String>) -> Result<Vec<String>> {
    let mut unique = Vec::new();
    for name in names {
        validate_variant_name(&name)?;
        if !unique.contains(&name) {
            unique.push(name);
        }
    }
    Ok(unique)
}

fn scan_variants(root: &Path, names: &[String]) -> Result<Vec<SourceVariant>> {
    let mut variants = Vec::new();
    for name in names {
        let path = root.join(name);
        if !path.is_dir() {
            return Err(message(format!("variant directory is missing: {name}")));
        }
        variants.push(SourceVariant {
            name: name.clone(),
            files: scan_files(&path)?,
            path,
        });
    }
    Ok(variants)
}

fn scan_assets(root: &Path) -> Result<Vec<SourceFile>> {
    let mut assets = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let file_name = entry.file_name();
            if file_name == MANIFEST_NAME {
                continue;
            }
            let relative = PathBuf::from(file_name);
            validate_relative_path(&relative)?;
            assets.push(SourceFile {
                relative_path: path_to_metadata(&relative)?,
                path: entry.path(),
            });
        }
    }
    assets.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(assets)
}

fn scan_files(root: &Path) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<SourceFile>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files(root, &path, files)?;
        } else if entry.file_type()?.is_file() {
            let relative = path.strip_prefix(root)?;
            files.push(SourceFile {
                relative_path: path_to_metadata(relative)?,
                path,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::scan_source;
    use std::fs::{create_dir_all, write};
    use std::path::{Path, PathBuf};

    #[test]
    fn uses_manifest_options() {
        let root = test_root("manifest_options");
        create_dir_all(root.join("B")).unwrap();
        write(root.join("manifest.json"), manifest_text()).unwrap();
        write(root.join("B").join("file.bin"), b"b").unwrap();

        let source = scan_source(&root).unwrap();
        assert_eq!(source.variants[0].name, "B");
        cleanup(&root);
    }

    #[test]
    fn falls_back_to_directories() {
        let root = test_root("directory_fallback");
        create_dir_all(root.join("A")).unwrap();
        write(root.join("A").join("file.bin"), b"a").unwrap();

        let source = scan_source(&root).unwrap();
        assert_eq!(source.variants[0].name, "A");
        cleanup(&root);
    }

    fn manifest_text() -> &'static str {
        r#"{"Version":1,"Name":"Demo","Options":[{"Name":"B","Include":["B"]}]}"#
    }

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("svd_scan_{name}"));
        cleanup(&root);
        root
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }
}
