//! Optional real-Patch regression for deterministic native/WASM-shared output.

use hd2_migrator_io::web::{
    EquipmentCategory, HeadArmorMergeOptions, PatchBytes, inspect_equipment, merge_head_into_armor,
};
use std::path::{Path, PathBuf};

#[test]
fn real_patch_merge_is_deterministic_when_configured() {
    let Some(armor_path) = env_path("HD2_HEAD_MERGE_ARMOR_PATCH") else {
        eprintln!("Set HD2_HEAD_MERGE_ARMOR_PATCH and selection hashes to run this regression");
        return;
    };
    let armor = load_patch(&armor_path);
    let head = env_path("HD2_HEAD_MERGE_HEAD_PATCH").map(|path| load_patch(&path));
    let options = HeadArmorMergeOptions {
        armor_hash: selected_hash(
            &armor,
            "HD2_HEAD_MERGE_ARMOR_HASH",
            EquipmentCategory::Armor,
        ),
        helmet_hash: selected_hash(
            head.as_ref().unwrap_or(&armor),
            "HD2_HEAD_MERGE_HELMET_HASH",
            EquipmentCategory::Helmet,
        ),
    };

    let first = merge_head_into_armor(armor.clone(), head.clone(), options.clone()).unwrap();
    let second = merge_head_into_armor(armor, head, options).unwrap();
    eprintln!("head merge summary: {:#?}", first.summary);
    assert_eq!(first.patch.toc, second.patch.toc);
    assert_eq!(first.patch.gpu, second.patch.gpu);
    assert_eq!(first.patch.stream, second.patch.stream);
    assert!(first.summary.merged_lods > 0);
    if let Some(output_dir) = env_path("HD2_HEAD_MERGE_OUTPUT_DIR") {
        write_patch(&output_dir, &first.patch);
    }
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

fn write_patch(directory: &Path, patch: &PatchBytes) {
    std::fs::create_dir_all(directory).unwrap();
    std::fs::write(directory.join(&patch.name), &patch.toc).unwrap();
    std::fs::write(
        directory.join(format!("{}.gpu_resources", patch.name)),
        &patch.gpu,
    )
    .unwrap();
    std::fs::write(
        directory.join(format!("{}.stream", patch.name)),
        &patch.stream,
    )
    .unwrap();
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn selected_hash(patch: &PatchBytes, env_name: &str, category: EquipmentCategory) -> String {
    if let Ok(value) = std::env::var(env_name) {
        return value;
    }
    let inspection = inspect_equipment(patch).expect("inspect equipment candidates");
    let mut candidates = inspection
        .sources
        .into_iter()
        .flat_map(|source| source.candidates)
        .filter(|option| option.category == category)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.hash.cmp(&right.hash));
    candidates.dedup_by(|left, right| left.hash == right.hash);
    assert_eq!(
        candidates.len(),
        1,
        "Set {env_name}; detected candidates: {candidates:#?}"
    );
    candidates.remove(0).hash
}
