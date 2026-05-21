//! Archive index: map FileID hex pairs -> readable armor names per category.
//!
//! Backed by `archivehashes.json`. Tolerant parse: unknown categories warn
//! rather than fail.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

const BUILTIN_INDEX_JSON: &str = include_str!("../assets/archivehashes.json");

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ArmorEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
pub struct ArchiveIndex {
    by_category: BTreeMap<String, Vec<ArmorEntry>>,
}

impl ArchiveIndex {
    pub fn load(path: &Path) -> crate::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| eyre::eyre!("read {}: {e}", path.display()))?;
        Self::from_str(&text)
    }

    pub fn from_str(text: &str) -> crate::Result<Self> {
        let raw: serde_json::Value = serde_json::from_str(text)?;
        Self::from_value(raw)
    }

    fn from_value(raw: serde_json::Value) -> crate::Result<Self> {
        let mut by_category = BTreeMap::new();
        let obj = raw
            .as_object()
            .ok_or_else(|| eyre::eyre!("archive index is not a JSON object"))?;
        for (cat, val) in obj {
            let entries = match val.as_array() {
                Some(a) => parse_entry_list(a),
                None => match val.as_object() {
                    Some(o) => parse_entry_map(o),
                    None => {
                        tracing::warn!(category = %cat, "unexpected archive index value, skipping");
                        continue;
                    }
                },
            };
            by_category.insert(cat.clone(), entries);
        }
        Ok(Self { by_category })
    }

    pub fn builtin() -> &'static Self {
        static CACHE: OnceLock<ArchiveIndex> = OnceLock::new();
        CACHE.get_or_init(|| {
            ArchiveIndex::from_str(BUILTIN_INDEX_JSON)
                .expect("builtin archivehashes.json must parse")
        })
    }

    pub fn category(&self, key: &str) -> Option<&[ArmorEntry]> {
        self.by_category.get(key).map(|v| v.as_slice())
    }

    pub fn categories(&self) -> impl Iterator<Item = &str> {
        self.by_category.keys().map(|s| s.as_str())
    }
}

fn parse_entry_list(arr: &[serde_json::Value]) -> Vec<ArmorEntry> {
    arr.iter()
        .filter_map(|v| serde_json::from_value::<ArmorEntry>(v.clone()).ok())
        .collect()
}

fn parse_entry_map(obj: &serde_json::Map<String, serde_json::Value>) -> Vec<ArmorEntry> {
    obj.iter()
        .map(|(hash, val)| {
            let name = val.as_str().map(str::to_owned).unwrap_or_default();
            ArmorEntry {
                name,
                hash: hash.clone(),
                extra: BTreeMap::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_index_parses() {
        let idx = ArchiveIndex::builtin();
        assert!(idx.categories().next().is_some(), "no categories");
    }
}
