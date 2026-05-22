mod tui;

use std::path::{Path, PathBuf};
use svd_core::args::parse_export_args;
use svd_core::error::{Result, message};
use svd_core::export::{ExportOptions, export};
use svd_core::sfx::load_embedded_package;
use tempfile::NamedTempFile;
use tui::{run_error_tui, run_export_tui};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 1 {
        return run_interactive_export();
    }
    let options = parse_export_args(args)?;
    export(&options)
}

fn run_interactive_export() -> Result<()> {
    let executable = std::env::current_exe()?;
    let embedded: Option<NamedTempFile> = load_embedded_package()?;
    let (package, output, display_name, _hold) = match embedded {
        Some(temp) => {
            let pkg = temp.path().to_path_buf();
            let out = default_output_path(&executable)?;
            let display = executable
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned());
            (pkg, out, display, Some(temp))
        }
        None => {
            let same_named = default_package_path(&executable)?;
            let pkg = match resolve_package(&executable, &same_named)? {
                ResolvedPackage::Found(path) => path,
                ResolvedPackage::Missing => return run_missing_package_tui(&same_named),
                ResolvedPackage::Ambiguous(candidates) => {
                    return run_ambiguous_package_tui(&candidates);
                }
            };
            let out = default_output_path(&pkg)?;
            (pkg, out, None, None)
        }
    };
    let mode = match run_export_tui(package.clone(), output.clone(), display_name) {
        Ok(mode) => mode,
        Err(error) => {
            run_error_tui("无法打开超级模组分包", error.to_string())?;
            return Ok(());
        }
    };
    let Some(mode) = mode else {
        return Ok(());
    };

    let options = ExportOptions {
        package,
        output: Some(output),
        mode,
        jobs: None,
    };
    export(&options)
}

fn run_missing_package_tui(package: &Path) -> Result<()> {
    run_error_tui(
        "找不到超级模组分包",
        format!(
            "请把 .svd 分包放在导出器旁边（同名优先：{}），\n或保证目录中仅有一个 .svd 文件。",
            package.display()
        ),
    )
}

fn run_ambiguous_package_tui(candidates: &[PathBuf]) -> Result<()> {
    let names = candidates
        .iter()
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>()
        .join("\n  ");
    run_error_tui(
        "目录中存在多个 .svd 分包",
        format!(
            "请只保留一个 .svd 文件，或将目标分包重命名为与导出器同名。\n当前候选：\n  {names}"
        ),
    )
}

enum ResolvedPackage {
    Found(PathBuf),
    Missing,
    Ambiguous(Vec<PathBuf>),
}

fn resolve_package(executable: &Path, same_named: &Path) -> Result<ResolvedPackage> {
    if same_named.is_file() {
        return Ok(ResolvedPackage::Found(same_named.to_path_buf()));
    }
    let dir = executable
        .parent()
        .ok_or_else(|| message("executable has no parent directory"))?;
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file() {
            continue;
        }
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("svd"))
        {
            candidates.push(path);
        }
    }
    candidates.sort();
    match candidates.len() {
        0 => Ok(ResolvedPackage::Missing),
        1 => Ok(ResolvedPackage::Found(
            candidates.into_iter().next().unwrap(),
        )),
        _ => Ok(ResolvedPackage::Ambiguous(candidates)),
    }
}

fn default_package_path(executable: &Path) -> Result<PathBuf> {
    let stem = executable
        .file_stem()
        .ok_or_else(|| message("cannot infer package name from executable path"))?;
    Ok(executable.with_file_name(stem).with_extension("svd"))
}

fn default_output_path(package: &Path) -> Result<PathBuf> {
    let stem = package
        .file_stem()
        .ok_or_else(|| message("cannot infer output name from package path"))?;
    Ok(package.with_file_name(stem).with_extension("zip"))
}
