use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set"));
    let dest = out_dir.join("embedded-exporter.bin");

    let profile = env::var("PROFILE").unwrap_or_else(|_| "release".to_string());
    let exe_name = if cfg!(target_os = "windows") {
        "svd-export.exe"
    } else {
        "svd-export"
    };
    let candidate = workspace_target_dir().join(&profile).join(exe_name);

    println!("cargo:rerun-if-changed={}", candidate.display());
    println!("cargo:rerun-if-changed=build.rs");

    let source_meta = std::fs::metadata(&candidate);
    let source_size = source_meta
        .as_ref()
        .map(|meta| if meta.is_file() { meta.len() } else { 0 })
        .unwrap_or(0);

    if source_size > 0 {
        std::fs::copy(&candidate, &dest).expect("copy embedded exporter");
        eprintln!(
            "svd-pack/build.rs: embedded svd-export ({} bytes) from {}",
            source_size,
            candidate.display()
        );
    } else {
        std::fs::write(&dest, b"").expect("write empty placeholder");
        let reason = match source_meta {
            Err(_) => "file not found",
            Ok(meta) if !meta.is_file() => "path exists but is not a file",
            Ok(_) => "file is empty (possibly a stale placeholder)",
        };
        let cargo_cmd = if profile == "release" {
            "cargo build --release"
        } else {
            "cargo build"
        };
        // Emit each line as its own cargo:warning so it survives output filters/summarizers.
        println!("cargo:warning=╔════════════════════════════════════════════════════════════════════════╗");
        println!("cargo:warning=║  svd-pack: NO EMBEDDED EXPORTER — output will be a plain .svd file     ║");
        println!("cargo:warning=╚════════════════════════════════════════════════════════════════════════╝");
        println!("cargo:warning=  reason   : {reason}");
        println!("cargo:warning=  expected : {}", candidate.display());
        println!("cargo:warning=  fix      : run `{cargo_cmd} -p svd-export` first, then rebuild svd-pack");
        println!("cargo:warning=             (the second build picks up the new exporter automatically)");
    }
}

fn workspace_target_dir() -> PathBuf {
    if let Some(dir) = env::var_os("CARGO_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("target"))
        .expect("workspace target dir")
}
