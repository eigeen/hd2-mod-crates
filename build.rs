use std::path::Path;

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let assets = Path::new(manifest_dir).join("assets");
    let must_exist = [
        ("archivehashes.json", true),
        ("armor_mappings.merged.json", true),
        ("empty_mesh/toc.bin", true),
        ("empty_mesh/gpu.bin", true),
        ("empty_mesh/stream.bin", false),
    ];

    for (rel, must_be_nonempty) in must_exist {
        let path = assets.join(rel);
        println!("cargo:rerun-if-changed=assets/{rel}");
        let meta = std::fs::metadata(&path).unwrap_or_else(|e| {
            panic!(
                "missing required asset {}: {} (extract via README script)",
                path.display(),
                e
            );
        });
        if must_be_nonempty && meta.len() == 0 {
            panic!("asset {} is empty (re-run extraction)", path.display());
        }
    }
}
