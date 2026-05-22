use crate::error::{Result, message};
use std::path::{Component, Path, PathBuf};

/// Converts a safe relative path to the forward-slash form used in metadata.
pub fn path_to_metadata(path: &Path) -> Result<String> {
    validate_relative_path(path)?;
    let text = path
        .components()
        .map(component_text)
        .collect::<Result<Vec<_>>>()?
        .join("/");
    Ok(text)
}

/// Converts a metadata path back into a safe platform path.
pub fn metadata_to_path(text: &str) -> Result<PathBuf> {
    let path = PathBuf::from(text.replace('/', std::path::MAIN_SEPARATOR_STR));
    validate_relative_path(&path)?;
    Ok(path)
}

/// Rejects absolute paths, prefixes, and parent traversal.
pub fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(message("relative path is empty"));
    }

    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err(message(format!("unsafe relative path: {}", path.display()))),
        }
    }

    Ok(())
}

/// Rejects variant names that cannot map to one directory.
pub fn validate_variant_name(name: &str) -> Result<()> {
    let path = Path::new(name);
    if name.is_empty() || path.components().count() != 1 {
        return Err(message(format!("unsafe variant name: {name}")));
    }
    validate_relative_path(path)
}

/// Joins a checked metadata path under a trusted root.
pub fn join_metadata_path(root: &Path, relative: &str) -> Result<PathBuf> {
    Ok(root.join(metadata_to_path(relative)?))
}

fn component_text(component: Component<'_>) -> Result<String> {
    match component {
        Component::Normal(value) => Ok(value.to_string_lossy().into_owned()),
        _ => Err(message("invalid component")),
    }
}

#[cfg(test)]
mod tests {
    use super::{metadata_to_path, validate_relative_path};
    use std::path::Path;

    #[test]
    fn rejects_parent_traversal() {
        assert!(metadata_to_path("../escape").is_err());
        assert!(validate_relative_path(Path::new("a/../b")).is_err());
    }

    #[test]
    fn accepts_nested_relative_path() {
        assert!(metadata_to_path("a/b/c.patch_0").is_ok());
    }
}
