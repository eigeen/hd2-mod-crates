//! Archive index: map FileID hex pairs -> readable armor names per category.
//!
//! Backed by `archivehashes.json`. Tolerant parse: unknown categories warn
//! rather than fail.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

const BUILTIN_INDEX_JSON: &str = hd2_migrator_data::ARCHIVE_INDEX_JSON;
const BUILTIN_OVERRIDES_JSON: &str = hd2_migrator_data::ARCHIVE_INDEX_OVERRIDES_JSON;

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
    preferred_hashes: BTreeMap<String, BTreeMap<String, String>>,
}

impl ArchiveIndex {
    pub fn load(path: &Path) -> crate::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| eyre::eyre!("read {}: {e}", path.display()))?;
        text.parse()
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
        Ok(Self {
            by_category,
            preferred_hashes: BTreeMap::new(),
        })
    }

    pub fn builtin() -> &'static Self {
        static CACHE: OnceLock<ArchiveIndex> = OnceLock::new();
        CACHE.get_or_init(|| {
            let mut index: ArchiveIndex = BUILTIN_INDEX_JSON
                .parse()
                .expect("builtin archivehashes.json must parse");
            index
                .apply_overrides(BUILTIN_OVERRIDES_JSON)
                .expect("builtin archive hash overrides must be valid");
            index
        })
    }

    pub fn category(&self, key: &str) -> Option<&[ArmorEntry]> {
        self.by_category.get(key).map(|v| v.as_slice())
    }

    pub fn categories(&self) -> impl Iterator<Item = &str> {
        self.by_category.keys().map(|s| s.as_str())
    }

    pub fn preferred_hash(&self, category: &str, name: &str) -> Option<&str> {
        self.preferred_hashes
            .get(category)?
            .get(name)
            .map(String::as_str)
    }

    fn apply_overrides(&mut self, text: &str) -> crate::Result<()> {
        let overrides: BTreeMap<String, BTreeMap<String, String>> = serde_json::from_str(text)?;
        for (category, equipment) in &overrides {
            for (name, hash) in equipment {
                self.validate_override(category, name, hash)?;
            }
        }
        self.preferred_hashes = overrides;
        Ok(())
    }

    fn validate_override(&self, category: &str, name: &str, hash: &str) -> crate::Result<()> {
        let matches_entry = self.category(category).is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| entry.name == name && entry.hash.eq_ignore_ascii_case(hash))
        });
        if !matches_entry {
            eyre::bail!("archive override {category}/{name} references unknown hash {hash}");
        }
        Ok(())
    }
}

impl std::str::FromStr for ArchiveIndex {
    type Err = eyre::Report;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        let raw: serde_json::Value = serde_json::from_str(text)?;
        Self::from_value(raw)
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

    #[test]
    fn builtin_override_prefers_player_fs_05_archive() {
        assert_eq!(
            ArchiveIndex::builtin().preferred_hash("Armor", "FS-05 Marksman"),
            Some("8670598c1f4462dc")
        );
    }
}
