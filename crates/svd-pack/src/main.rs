use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use svd_core::error::{Result, message};
use svd_core::manifest::read_manifest;
use svd_core::pack::{PackOptions, pack};
use svd_core::sfx::write_self_extracting;

const EMBEDDED_EXPORTER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/embedded-exporter.bin"));

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            pause_on_exit();
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let input = parse_input()?;
    let (output_dir, package_path, sfx_path) = derive_output_paths(&input)?;
    let base = pick_default_base(&input)?;

    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir)?;
    }
    if package_path.exists() {
        std::fs::remove_file(&package_path)?;
    }
    if let Some(sfx) = &sfx_path
        && sfx.exists()
    {
        std::fs::remove_file(sfx)?;
    }

    println!("input   : {}", input.display());
    println!("base    : {}", base);
    match &sfx_path {
        Some(sfx) => println!("output  : {}", sfx.display()),
        None => println!("output  : {}", package_path.display()),
    }

    pack(&PackOptions {
        input,
        base,
        output: output_dir.clone(),
        zip_output: Some(package_path.clone()),
        level: 3,
        jobs: None,
    })?;

    if let Some(sfx) = sfx_path {
        write_self_extracting(EMBEDDED_EXPORTER, &package_path, &sfx)?;
        std::fs::remove_file(&package_path)?;
    }
    std::fs::remove_dir_all(&output_dir)?;
    Ok(())
}

fn parse_input() -> Result<PathBuf> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or_else(|| message("usage: svd-pack <input-directory>"))?;
    if args.next().is_some() {
        return Err(message(
            "unexpected extra arguments; pass a single directory",
        ));
    }
    let path = PathBuf::from(input);
    if !path.is_dir() {
        return Err(message(format!(
            "input is not a directory: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn derive_output_paths(input: &Path) -> Result<(PathBuf, PathBuf, Option<PathBuf>)> {
    let absolute = std::fs::canonicalize(input)?;
    let name = absolute
        .file_name()
        .ok_or_else(|| message("input path has no final component"))?
        .to_string_lossy()
        .into_owned();
    let parent = absolute
        .parent()
        .ok_or_else(|| message("input path has no parent directory"))?
        .to_path_buf();
    let stem = format!("{name}-pack");
    let dir = parent.join(&stem);
    let zip = parent.join(format!("{stem}.svd"));
    let sfx = if EMBEDDED_EXPORTER.is_empty() {
        None
    } else {
        Some(parent.join(format!("{stem}.exe")))
    };
    Ok((dir, zip, sfx))
}

fn pick_default_base(input: &Path) -> Result<String> {
    if let Some(manifest) = read_manifest(input)?
        && let Some(options) = manifest.options
    {
        for option in options {
            if let Some(name) = option.include.into_iter().next() {
                return Ok(name);
            }
        }
    }

    let mut names: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(input)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    names
        .into_iter()
        .next()
        .ok_or_else(|| message("no variant directories found in input"))
}

fn pause_on_exit() {
    eprint!("press Enter to exit...");
    let _ = io::stderr().flush();
    let mut buf = String::new();
    let _ = io::stdin().read_line(&mut buf);
}
