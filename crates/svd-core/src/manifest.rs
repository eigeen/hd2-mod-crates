use crate::error::Result;
use crate::metadata::{ModInfo, StoredManifestOption};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

pub const MANIFEST_NAME: &str = "manifest.json";
pub const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SourceManifest {
    pub guid: Option<String>,
    pub name: Option<String>,
    pub options: Option<Vec<ManifestOption>>,
    pub description: Option<String>,
    pub icon_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ManifestOption {
    pub name: String,
    pub include: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct OutputManifest {
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    pub name: String,
    pub options: Vec<OutputOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct OutputOption {
    pub name: String,
    pub include: Vec<String>,
    pub description: String,
}

/// Reads the source manifest if it exists; the file is UTF-8 JSON.
pub fn read_manifest(root: &Path) -> Result<Option<SourceManifest>> {
    let path = root.join(MANIFEST_NAME);
    if !path.exists() {
        return Ok(None);
    }

    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

/// Extracts package-level metadata from a source manifest.
pub fn mod_info(manifest: Option<&SourceManifest>) -> ModInfo {
    manifest.map_or_else(ModInfo::default, |source| ModInfo {
        name: source.name.clone(),
        description: source.description.clone(),
        guid: source.guid.clone(),
        icon_path: source.icon_path.clone(),
    })
}

/// Converts source manifest Options into the canonical form stored in the patch package.
pub fn stored_manifest_options(manifest: Option<&SourceManifest>) -> Vec<StoredManifestOption> {
    manifest
        .and_then(|source| source.options.as_ref())
        .map(|options| {
            options
                .iter()
                .map(|opt| StoredManifestOption {
                    name: opt.name.clone(),
                    include: opt.include.clone(),
                    description: opt.description.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Writes a mod-manager manifest that registers each enabled variant as a sub-mod option,
/// reusing the canonical Option groupings from the source manifest when available.
pub fn write_export_manifest(
    root: &Path,
    mod_info: &ModInfo,
    options_template: &[StoredManifestOption],
    selected_variants: &[String],
) -> Result<()> {
    let manifest = build_export_manifest(mod_info, options_template, selected_variants);
    let path = root.join(MANIFEST_NAME);
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

fn build_export_manifest(
    mod_info: &ModInfo,
    options_template: &[StoredManifestOption],
    selected_variants: &[String],
) -> OutputManifest {
    OutputManifest {
        version: MANIFEST_VERSION,
        guid: mod_info.guid.clone(),
        name: mod_info.name.clone().unwrap_or_default(),
        options: build_output_options(options_template, selected_variants),
        description: mod_info.description.clone(),
        icon_path: mod_info.icon_path.clone(),
    }
}

fn build_output_options(
    options_template: &[StoredManifestOption],
    selected_variants: &[String],
) -> Vec<OutputOption> {
    let selected: BTreeSet<&str> = selected_variants.iter().map(String::as_str).collect();

    if options_template.is_empty() {
        return selected_variants
            .iter()
            .map(|name| OutputOption {
                name: name.clone(),
                include: vec![name.clone()],
                description: name.clone(),
            })
            .collect();
    }

    options_template
        .iter()
        .filter_map(|option| {
            let include: Vec<String> = option
                .include
                .iter()
                .filter(|name| selected.contains(name.as_str()))
                .cloned()
                .collect();
            if include.is_empty() {
                return None;
            }
            Some(OutputOption {
                name: option.name.clone(),
                include,
                description: option
                    .description
                    .clone()
                    .unwrap_or_else(|| option.name.clone()),
            })
        })
        .collect()
}
