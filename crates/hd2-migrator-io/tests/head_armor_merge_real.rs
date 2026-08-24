//! Optional real-Patch regression for deterministic native/WASM-shared output.

use hd2_migrator_io::web::{HeadArmorMergeOptions, PatchBytes, merge_head_into_armor};
use std::path::{Path, PathBuf};

#[test]
fn real_patch_merge_is_deterministic_when_configured() {
    let Some(armor_path) = env_path("HD2_HEAD_MERGE_ARMOR_PATCH") else {
        eprintln!("Set HD2_HEAD_MERGE_ARMOR_PATCH and selection hashes to run this regression");
        return;
    };
    let options = HeadArmorMergeOptions {
        armor_hash: required_env("HD2_HEAD_MERGE_ARMOR_HASH"),
        helmet_hash: required_env("HD2_HEAD_MERGE_HELMET_HASH"),
    };
    let armor = load_patch(&armor_path);
    let head = env_path("HD2_HEAD_MERGE_HEAD_PATCH").map(|path| load_patch(&path));

    let first = merge_head_into_armor(armor.clone(), head.clone(), options.clone()).unwrap();
    let second = merge_head_into_armor(armor, head, options).unwrap();
    assert_eq!(first.patch.toc, second.patch.toc);
    assert_eq!(first.patch.gpu, second.patch.gpu);
    assert_eq!(first.patch.stream, second.patch.stream);
    assert!(first.summary.merged_lods > 0);
}

fn load_patch(path: &Path) -> PatchBytes {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    PatchBytes {
        name,
        toc: std::fs::read(path).unwrap(),
        gpu: read_optional(PathBuf::from(format!("{}.gpu_resources", path.display()))),
        stream: read_optional(PathBuf::from(format!("{}.stream", path.display()))),
    }
}

fn read_optional(path: PathBuf) -> Vec<u8> {
    std::fs::read(path).unwrap_or_default()
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}
