//! Default target exclusions for broad armor migrations.

const DEFAULT_EXCLUDED_TARGET_HASHES: &[&str] = &[
    "d895f447d332c331",
    "fda9b1496320364e",
    "681a512c5a5c0de7",
    "62c6fbc5cd90448d",
    "1866f762c49358ea",
    "33cecc4e485ea3c4",
    "788df19150915809",
    "31d06451a09679f1",
    "8670598c1f4462dc",
    "ccac0a57aca3ae92",
    "c72998936b1d88a2",
    "d5f85cb5efb44cd2",
    "dd3c12b413651958",
    "91033cfaaad4ad18",
    "0fecb336be1d287b",
    "58e4bd4b2278d15c",
    "562a45e9bc984eb9",
    "6cd3d55c05d4eac1",
    "519cc1ec2eb56e1d",
    "7ae5a2f32d6f24f1",
    "75a21fc49f80fb9c",
    "21a24072a79aeba8",
    "292b09dac80ce1de",
    "88b7d737b6e62cc0",
    "fd109bfeeed36726",
    "3b143cc283b7f707",
    "f4880623d32bafa9",
    "8313c9a556b8ee85",
];

const DEFAULT_EXCLUDED_TARGET_NAMES: &[&str] = &[
    "SA-7 Headfirst",
    "AF-91 Field Chemist",
    "B-24 Enforcer",
    "FS-05 Marksman",
    "SC-37 Legionnaire",
    "UF-84 Doubt Killer",
    "B-01 Tactical (Variation 1)",
    "B-01 Tactical (Variation 2)",
    "B-01 Tactical (Variation 3)",
    "B-01 Tactical (Variation 4)",
];

/// Return true for targets that should not be selected in broad default runs.
pub fn is_default_excluded_target(hash: &str, name: &str) -> bool {
    is_default_excluded_hash(hash) || is_default_excluded_name(name)
}

pub(crate) fn is_default_excluded_name(name: &str) -> bool {
    DEFAULT_EXCLUDED_TARGET_NAMES
        .iter()
        .any(|excluded| name.eq_ignore_ascii_case(excluded))
}

fn is_default_excluded_hash(hash: &str) -> bool {
    DEFAULT_EXCLUDED_TARGET_HASHES
        .iter()
        .any(|excluded| hash.eq_ignore_ascii_case(excluded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_sa_7_by_hash_or_name() {
        assert!(is_default_excluded_target("d895f447d332c331", "Other"));
        assert!(is_default_excluded_target("other", "SA-7 Headfirst"));
    }

    #[test]
    fn excludes_multi_archive_equipment_by_hash_or_name() {
        assert!(is_default_excluded_target("31d06451a09679f1", "Other"));
        assert!(is_default_excluded_target("other", "FS-05 Marksman"));
        assert!(is_default_excluded_target("7ae5a2f32d6f24f1", "Other"));
        assert!(is_default_excluded_target("other", "AF-91 Field Chemist"));
    }

    #[test]
    fn excludes_variant_equipment_by_hash_or_name() {
        assert!(is_default_excluded_target("58e4bd4b2278d15c", "Other"));
        assert!(is_default_excluded_target(
            "other",
            "B-01 Tactical (Variation 3)"
        ));
    }

    #[test]
    fn keeps_regular_targets() {
        assert!(!is_default_excluded_target("other", "AF-52 Lockdown"));
    }
}
