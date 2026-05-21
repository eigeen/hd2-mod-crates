//! Extract Unit customization labels from TocData.
//!
//! Mirrors `mod_armor_migrator/unit_names.py`. Strategy:
//! 1. Scan the blob for ASCII strings matching
//!    `HelldiverCustomization(BodyType|Slot|Weight|PieceType)_<ident>`.
//! 2. For each of the four prefixes, the LAST occurrence wins (this matches
//!    the Python `matches[-1]` behavior).
//! 3. If any of the four are missing, fall back to a bonehash-based inference
//!    from the shared `hashlists/bonehash.txt`.

const BONEHASH_TEXT: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/bonehash.txt"));

use std::sync::OnceLock;

const BODY: &str = "HelldiverCustomizationBodyType_";
const SLOT: &str = "HelldiverCustomizationSlot_";
const WEIGHT: &str = "HelldiverCustomizationWeight_";
const PIECE: &str = "HelldiverCustomizationPieceType_";

/// Customization metadata pulled from a Unit's TocData.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnitCustomizationName {
    pub body_type: String,
    pub slot: String,
    pub weight: String,
    pub piece_type: String,
}

impl UnitCustomizationName {
    /// `BodyType/Slot/Weight/PieceType` — NameFromMesh-style customization path.
    pub fn label(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.body_type, self.slot, self.weight, self.piece_type
        )
    }

    /// Normalize `body_type` to one of `Stocky`, `Slim`, `Any`, else `Unknown`.
    pub fn body_variant(&self) -> &'static str {
        match self.body_type.as_str() {
            "Any" => "Any",
            "Stocky" => "Stocky",
            "Slim" => "Slim",
            _ => "Unknown",
        }
    }
}

pub fn extract_customization_name(toc_data: &[u8]) -> Option<UnitCustomizationName> {
    let values = customization_values(toc_data);
    if values.iter().all(Option::is_some) {
        let [body, slot, weight, piece] = values;
        return Some(UnitCustomizationName {
            body_type: body.unwrap_or_default(),
            slot: slot.unwrap_or_default(),
            weight: weight.unwrap_or_default(),
            piece_type: piece.unwrap_or_default(),
        });
    }
    extract_bonehash_customization_name(toc_data)
}

pub fn body_variant(toc_data: &[u8]) -> &'static str {
    extract_customization_name(toc_data)
        .as_ref()
        .map(UnitCustomizationName::body_variant)
        .unwrap_or("Unknown")
}

/// Returns `[body, slot, weight, piece]`, each `Some(suffix)` if found.
pub(crate) fn customization_values(toc_data: &[u8]) -> [Option<String>; 4] {
    let matches = scan_customization_strings(toc_data);
    let prefixes = [BODY, SLOT, WEIGHT, PIECE];
    let mut out: [Option<String>; 4] = Default::default();
    for (i, prefix) in prefixes.iter().enumerate() {
        let last = matches
            .iter()
            .rfind(|s| s.starts_with(prefix))
            .map(|s| s[prefix.len()..].to_string());
        out[i] = last;
    }
    out
}

/// Find every `HelldiverCustomization(BodyType|Slot|Weight|PieceType)_<ident>`
/// occurrence in the byte stream. Pure scanning — no regex crate dep.
fn scan_customization_strings(data: &[u8]) -> Vec<String> {
    const HEAD: &[u8] = b"HelldiverCustomization";
    let mut out = Vec::new();
    let mut i = 0;
    while i + HEAD.len() < data.len() {
        if &data[i..i + HEAD.len()] != HEAD {
            i += 1;
            continue;
        }
        // Match one of the four kinds after HEAD.
        let after = &data[i + HEAD.len()..];
        let kind_len = match after {
            x if x.starts_with(b"BodyType_") => "BodyType_".len(),
            x if x.starts_with(b"Slot_") => "Slot_".len(),
            x if x.starts_with(b"Weight_") => "Weight_".len(),
            x if x.starts_with(b"PieceType_") => "PieceType_".len(),
            _ => {
                i += 1;
                continue;
            }
        };
        let start = i;
        let body_start = start + HEAD.len() + kind_len;
        let mut end = body_start;
        while end < data.len() && is_ident_byte(data[end]) {
            end += 1;
        }
        if end > body_start {
            if let Ok(s) = std::str::from_utf8(&data[start..end]) {
                out.push(s.to_string());
            }
        }
        i = end.max(i + 1);
    }
    out
}

#[inline]
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Infer customization semantics from known mesh group bone hashes.
fn extract_bonehash_customization_name(toc_data: &[u8]) -> Option<UnitCustomizationName> {
    let mut matches = bonehash_semantics_in_blob(toc_data);
    matches.sort();
    matches.dedup();
    let (body_type, slot, piece_type) = one_match(matches)?;
    Some(UnitCustomizationName {
        body_type,
        slot,
        weight: "Medium".to_string(),
        piece_type,
    })
}

fn one_match(mut matches: Vec<(String, String, String)>) -> Option<(String, String, String)> {
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}

fn bonehash_semantics_in_blob(toc_data: &[u8]) -> Vec<(String, String, String)> {
    let names = relevant_bonehash_names();
    (0..toc_data.len().saturating_sub(3))
        .filter_map(|offset| bonehash_name_at(toc_data, offset, &names))
        .filter_map(semantic_from_bone_name)
        .collect()
}

fn bonehash_name_at<'a>(data: &[u8], offset: usize, names: &[(u32, &'a str)]) -> Option<&'a str> {
    let value = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?);
    names
        .iter()
        .find(|(hash, _)| *hash == value)
        .map(|(_, name)| *name)
}

fn relevant_bonehash_names() -> Vec<(u32, &'static str)> {
    static NAMES: OnceLock<Vec<(u32, &'static str)>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            BONEHASH_TEXT
                .lines()
                .filter_map(parse_bonehash_line)
                .filter(|(_, name)| semantic_from_bone_name(name).is_some())
                .collect()
        })
        .clone()
}

fn parse_bonehash_line(line: &'static str) -> Option<(u32, &'static str)> {
    let (value, name) = line.trim().split_once(char::is_whitespace)?;
    Some((value.parse().ok()?, name.trim_start()))
}

fn semantic_from_bone_name(name: &str) -> Option<(String, String, String)> {
    let normalized = normalize_bone_mesh_name(name);
    let parts: Vec<&str> = normalized.split('_').collect();
    let body_type = body_type_from_bone_parts(&parts)?;
    let (slot, piece_type) = slot_piece_from_bone_part(&parts[1..parts.len() - 1].join("_"))?;
    Some((
        body_type.to_string(),
        slot.to_string(),
        piece_type.to_string(),
    ))
}

fn body_type_from_bone_parts(parts: &[&str]) -> Option<&'static str> {
    if parts.len() < 3 || !matches!(parts[0], "g" | "grp") {
        return None;
    }
    match *parts.last()? {
        "male" => Some("Stocky"),
        "female" => Some("Slim"),
        _ => None,
    }
}

fn normalize_bone_mesh_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let without_lod = strip_lod_suffix(&lower);
    strip_named_suffix(without_lod, &["shadow", "cloth"]).to_string()
}

fn strip_lod_suffix(name: &str) -> &str {
    let Some((prefix, suffix)) = name.rsplit_once("_lod") else {
        return name;
    };
    if suffix.chars().all(|ch| ch.is_ascii_digit()) {
        prefix
    } else {
        name
    }
}

fn strip_named_suffix<'a>(name: &'a str, suffixes: &[&str]) -> &'a str {
    suffixes
        .iter()
        .find_map(|suffix| name.strip_suffix(&format!("_{suffix}")))
        .unwrap_or(name)
}

fn slot_piece_from_bone_part(part: &str) -> Option<(&'static str, &'static str)> {
    match part {
        "torso_undergarment" => Some(("Torso", "Undergarment")),
        "torso" => Some(("Torso", "Armor")),
        "torso_arm_l" => Some(("LeftArm", "Undergarment")),
        "torso_arm_r" => Some(("RightArm", "Undergarment")),
        "shoulder_l" | "l_shoulder" => Some(("LeftShoulder", "Armor")),
        "shoulder_r" | "r_shoulder" => Some(("RightShoulder", "Armor")),
        "legs_hips_undergarment" => Some(("Hip", "Undergarment")),
        "legs_hips" => Some(("Hip", "Armor")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_all_four_fields() {
        let mut blob = Vec::new();
        blob.extend_from_slice(b"\x00\x00garbage");
        blob.extend_from_slice(b"HelldiverCustomizationBodyType_Stocky\x00");
        blob.extend_from_slice(b"HelldiverCustomizationSlot_Torso\x00");
        blob.extend_from_slice(b"HelldiverCustomizationWeight_Medium\x00");
        blob.extend_from_slice(b"HelldiverCustomizationPieceType_Armor\x00");
        let got = extract_customization_name(&blob).expect("present");
        assert_eq!(got.body_type, "Stocky");
        assert_eq!(got.slot, "Torso");
        assert_eq!(got.weight, "Medium");
        assert_eq!(got.piece_type, "Armor");
        assert_eq!(got.body_variant(), "Stocky");
        assert_eq!(got.label(), "Stocky/Torso/Medium/Armor");
    }

    #[test]
    fn last_occurrence_wins() {
        let mut blob = Vec::new();
        blob.extend_from_slice(b"HelldiverCustomizationBodyType_First\x00");
        blob.extend_from_slice(b"HelldiverCustomizationSlot_Slot\x00");
        blob.extend_from_slice(b"HelldiverCustomizationWeight_W\x00");
        blob.extend_from_slice(b"HelldiverCustomizationPieceType_P\x00");
        blob.extend_from_slice(b"HelldiverCustomizationBodyType_Second\x00");
        let got = extract_customization_name(&blob).expect("present");
        assert_eq!(got.body_type, "Second");
    }

    #[test]
    fn missing_field_returns_none() {
        let blob = b"HelldiverCustomizationBodyType_Stocky\x00".to_vec();
        assert!(extract_customization_name(&blob).is_none());
    }

    #[test]
    fn infers_body_variant_from_bonehash() {
        let stocky = extract_customization_name(&531958952_u32.to_le_bytes()).expect("stocky");
        let slim = extract_customization_name(&1146309845_u32.to_le_bytes()).expect("slim");
        assert_eq!(stocky.label(), "Stocky/Torso/Medium/Undergarment");
        assert_eq!(slim.label(), "Slim/Torso/Medium/Undergarment");
    }

    #[test]
    fn ambiguous_bonehash_semantics_return_none() {
        let mut blob = Vec::new();
        blob.extend_from_slice(&531958952_u32.to_le_bytes());
        blob.extend_from_slice(&1146309845_u32.to_le_bytes());
        assert!(extract_customization_name(&blob).is_none());
    }
}
