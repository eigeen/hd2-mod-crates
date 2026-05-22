use std::fs::{create_dir_all, read, write};
use std::io::Read;
use std::path::{Path, PathBuf};
use svd_core::export::{ExportMode, ExportOptions, export, list_available_variants};
use svd_core::metadata::{FileOperation, METADATA_FILE, PackageMetadata};
use svd_core::pack::{PackOptions, pack};
use tempfile::TempDir;

#[test]
fn classifies_core_file_operations() {
    let root = test_root("ops_source");
    let package = test_root("ops_package");
    let out = test_root("ops_out");
    create_dir_all(root.join("Base")).unwrap();
    create_dir_all(root.join("Target")).unwrap();

    write(root.join("Base").join("same.bin"), b"same").unwrap();
    write(root.join("Target").join("same.bin"), b"same").unwrap();
    write(root.join("Base").join("patch.bin"), b"base").unwrap();
    write(root.join("Target").join("patch.bin"), b"target").unwrap();
    write(root.join("Target").join("new.bin"), b"new").unwrap();
    write(root.join("Base").join("delete.bin"), b"delete").unwrap();
    write(root.join("Target").join("empty.bin"), b"").unwrap();

    pack(&PackOptions {
        input: root.clone(),
        base: "Base".to_owned(),
        output: package.clone(),
        zip_output: None,
        level: 3,
        jobs: None,
    })
    .unwrap();

    let out_zip = out.with_extension("zip");
    let _ = std::fs::remove_file(&out_zip);
    export(&ExportOptions {
        package: package.clone(),
        output: Some(out_zip.clone()),
        mode: ExportMode::Variants(vec!["Target".to_owned()]),
        jobs: None,
    })
    .unwrap();

    let extracted = extract_to_temp(&out_zip);
    let target = extracted.path().join("Target");
    assert_eq!(read(target.join("same.bin")).unwrap(), b"same");
    assert_eq!(read(target.join("patch.bin")).unwrap(), b"target");
    assert_eq!(read(target.join("new.bin")).unwrap(), b"new");
    assert!(!target.join("delete.bin").exists());
    assert_eq!(read(target.join("empty.bin")).unwrap(), b"");
    assert!(extracted.path().join("manifest.json").is_file());
    cleanup(&root);
    cleanup(&package);
    let _ = std::fs::remove_file(out_zip);
}

#[test]
fn pairs_patch_resource_files_by_logical_suffix() {
    let root = test_root("logical_patch_source");
    let package = test_root("logical_patch_package");
    let out = test_root("logical_patch_out");
    create_dir_all(root.join("Base").join("data")).unwrap();
    create_dir_all(root.join("Target").join("data")).unwrap();

    write(
        root.join("Base")
            .join("data")
            .join("1d9e8acfc3ee3ace.patch_0"),
        b"base core",
    )
    .unwrap();
    write(
        root.join("Base")
            .join("data")
            .join("1d9e8acfc3ee3ace.patch_0.gpu_resources"),
        b"shared gpu",
    )
    .unwrap();
    write(
        root.join("Target")
            .join("data")
            .join("2d25ef67b7d87ea3.patch_0"),
        b"target core",
    )
    .unwrap();
    write(
        root.join("Target")
            .join("data")
            .join("2d25ef67b7d87ea3.patch_0.gpu_resources"),
        b"shared gpu",
    )
    .unwrap();

    pack(&PackOptions {
        input: root.clone(),
        base: "Base".to_owned(),
        output: package.clone(),
        zip_output: None,
        level: 3,
        jobs: Some(4),
    })
    .unwrap();

    let metadata = read_metadata(&package);
    let target = metadata
        .variants
        .iter()
        .find(|variant| variant.name == "Target")
        .unwrap();
    assert_eq!(target.files.len(), 2);
    assert!(target.files.iter().any(|record| {
        record.relative_path == "data/2d25ef67b7d87ea3.patch_0"
            && record.base_relative_path.as_deref() == Some("data/1d9e8acfc3ee3ace.patch_0")
            && record.operation == FileOperation::Patch
    }));
    assert!(target.files.iter().any(|record| {
        record.relative_path == "data/2d25ef67b7d87ea3.patch_0.gpu_resources"
            && record.base_relative_path.as_deref()
                == Some("data/1d9e8acfc3ee3ace.patch_0.gpu_resources")
            && record.operation == FileOperation::SameAsBase
    }));

    let out_zip = out.with_extension("zip");
    let _ = std::fs::remove_file(&out_zip);
    export(&ExportOptions {
        package: package.clone(),
        output: Some(out_zip.clone()),
        mode: ExportMode::Variants(vec!["Target".to_owned()]),
        jobs: None,
    })
    .unwrap();

    let extracted = extract_to_temp(&out_zip);
    let target_data = extracted.path().join("Target").join("data");
    assert_eq!(
        read(target_data.join("2d25ef67b7d87ea3.patch_0")).unwrap(),
        b"target core"
    );
    assert_eq!(
        read(target_data.join("2d25ef67b7d87ea3.patch_0.gpu_resources")).unwrap(),
        b"shared gpu"
    );
    assert!(!target_data.join("1d9e8acfc3ee3ace.patch_0").exists());
    cleanup(&root);
    cleanup(&package);
    let _ = std::fs::remove_file(out_zip);
}

#[test]
fn writes_zip_package_and_exports_all_from_zip() {
    let root = test_root("zip_source");
    let package = test_root("zip_package");
    let package_zip = test_root("zip_package_file").with_extension("zip");
    let out = test_root("zip_out");
    let _ = std::fs::remove_file(&package_zip);
    create_dir_all(root.join("Base")).unwrap();
    create_dir_all(root.join("Target")).unwrap();
    write(root.join("note.txt"), b"asset").unwrap();
    write(root.join("Base").join("file.bin"), b"base").unwrap();
    write(root.join("Target").join("file.bin"), b"target").unwrap();

    pack(&PackOptions {
        input: root.clone(),
        base: "Base".to_owned(),
        output: package.clone(),
        zip_output: Some(package_zip.clone()),
        level: 3,
        jobs: Some(2),
    })
    .unwrap();

    let names = list_available_variants(&package_zip).unwrap();
    assert_eq!(names, vec!["Base".to_owned(), "Target".to_owned()]);

    let out_zip = out.with_extension("zip");
    let _ = std::fs::remove_file(&out_zip);
    export(&ExportOptions {
        package: package_zip.clone(),
        output: Some(out_zip.clone()),
        mode: ExportMode::All,
        jobs: Some(2),
    })
    .unwrap();

    let extracted = extract_to_temp(&out_zip);
    let root_out = extracted.path();
    assert_eq!(read(root_out.join("note.txt")).unwrap(), b"asset");
    assert_eq!(read(root_out.join("Base").join("file.bin")).unwrap(), b"base");
    assert_eq!(
        read(root_out.join("Target").join("file.bin")).unwrap(),
        b"target"
    );

    let manifest_text = std::fs::read_to_string(root_out.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    assert_eq!(manifest["Version"], 1);
    let options = manifest["Options"].as_array().unwrap();
    let option_names: Vec<&str> = options
        .iter()
        .map(|opt| opt["Name"].as_str().unwrap())
        .collect();
    assert!(option_names.contains(&"Base"));
    assert!(option_names.contains(&"Target"));
    for opt in options {
        let name = opt["Name"].as_str().unwrap();
        let include = opt["Include"].as_array().unwrap();
        assert_eq!(include.len(), 1);
        assert_eq!(include[0].as_str().unwrap(), name);
    }
    cleanup(&root);
    cleanup(&package);
    let _ = std::fs::remove_file(out_zip);
    let _ = std::fs::remove_file(package_zip);
}

#[test]
fn carries_source_manifest_options_through_pack_and_filters_on_export() {
    let root = test_root("manifest_carry_source");
    let package = test_root("manifest_carry_package");
    let out = test_root("manifest_carry_out");
    create_dir_all(root.join("Base")).unwrap();
    create_dir_all(root.join("VariantA")).unwrap();
    create_dir_all(root.join("VariantB")).unwrap();
    write(root.join("Base").join("file.bin"), b"base").unwrap();
    write(root.join("VariantA").join("file.bin"), b"a").unwrap();
    write(root.join("VariantB").join("file.bin"), b"b").unwrap();
    write(root.join("cover.png"), b"cover-bytes").unwrap();
    write(
        root.join("manifest.json"),
        r#"{
            "Version": 1,
            "Guid": "abc-123",
            "Name": "Demo Pack",
            "Description": "Demo description",
            "IconPath": "cover.png",
            "Options": [
                { "Name": "Base Option", "Include": ["Base"], "Description": "base" },
                { "Name": "Alpha Option", "Include": ["VariantA"], "Description": "first one" },
                { "Name": "Beta Option", "Include": ["VariantB"], "Description": "second one" }
            ]
        }"#,
    )
    .unwrap();

    pack(&PackOptions {
        input: root.clone(),
        base: "Base".to_owned(),
        output: package.clone(),
        zip_output: None,
        level: 3,
        jobs: None,
    })
    .unwrap();

    let stored = read_metadata(&package);
    assert_eq!(stored.manifest_options.len(), 3);
    assert_eq!(stored.manifest_options[1].name, "Alpha Option");
    assert_eq!(
        stored.manifest_options[1].description.as_deref(),
        Some("first one")
    );
    assert_eq!(stored.mod_info.icon_path.as_deref(), Some("cover.png"));
    assert_eq!(stored.mod_info.guid.as_deref(), Some("abc-123"));

    let out_zip = out.with_extension("zip");
    let _ = std::fs::remove_file(&out_zip);
    export(&ExportOptions {
        package: package.clone(),
        output: Some(out_zip.clone()),
        mode: ExportMode::Variants(vec!["VariantA".to_owned()]),
        jobs: None,
    })
    .unwrap();

    let extracted = extract_to_temp(&out_zip);
    let root_out = extracted.path();
    assert_eq!(read(root_out.join("cover.png")).unwrap(), b"cover-bytes");
    assert_eq!(read(root_out.join("VariantA").join("file.bin")).unwrap(), b"a");
    assert!(!root_out.join("VariantB").exists());

    let manifest_text = std::fs::read_to_string(root_out.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    assert_eq!(manifest["Version"], 1);
    assert_eq!(manifest["Guid"], "abc-123");
    assert_eq!(manifest["Name"], "Demo Pack");
    assert_eq!(manifest["Description"], "Demo description");
    assert_eq!(manifest["IconPath"], "cover.png");
    let options = manifest["Options"].as_array().unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0]["Name"], "Alpha Option");
    assert_eq!(options[0]["Description"], "first one");
    let include = options[0]["Include"].as_array().unwrap();
    assert_eq!(include.len(), 1);
    assert_eq!(include[0], "VariantA");

    cleanup(&root);
    cleanup(&package);
    let _ = std::fs::remove_file(out_zip);
}

fn extract_to_temp(zip_path: &Path) -> TempDir {
    let dir = TempDir::new().unwrap();
    let file = std::fs::File::open(zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let rel = entry.enclosed_name().unwrap();
        let out_path = dir.path().join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).unwrap();
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut out_file = std::fs::File::create(&out_path).unwrap();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).unwrap();
        std::io::Write::write_all(&mut out_file, &buf).unwrap();
    }
    dir
}

fn read_metadata(package: &Path) -> PackageMetadata {
    let text = std::fs::read_to_string(package.join(METADATA_FILE)).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn test_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("svd_pack_{name}"));
    cleanup(&root);
    root
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}
