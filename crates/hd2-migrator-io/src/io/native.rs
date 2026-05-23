//! Filesystem-backed [`DataSource`] used by the CLI driver and parity tests.

use super::{DataSource, IoFuture};
use eyre::WrapErr;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub struct NativeDataSource {
    base: PathBuf,
}

impl NativeDataSource {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    pub fn base(&self) -> &Path {
        &self.base
    }
}

impl DataSource for NativeDataSource {
    fn read_full<'a>(&'a self, path: &'a str) -> IoFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let full = self.base.join(path);
            std::fs::read(&full).wrap_err_with(|| format!("read {}", full.display()))
        })
    }

    fn read_range<'a>(&'a self, path: &'a str, offset: u64, len: u64) -> IoFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let full = self.base.join(path);
            let mut file = File::open(&full)
                .wrap_err_with(|| format!("open {}", full.display()))?;
            file.seek(SeekFrom::Start(offset))
                .wrap_err_with(|| format!("seek {} @ {offset}", full.display()))?;
            let mut buf = vec![0u8; usize::try_from(len).map_err(|_| eyre::eyre!("range len overflow"))?];
            file.read_exact(&mut buf)
                .wrap_err_with(|| format!("read range {} +{len}", full.display()))?;
            Ok(buf)
        })
    }

    fn exists<'a>(&'a self, path: &'a str) -> IoFuture<'a, bool> {
        Box::pin(async move { Ok(self.base.join(path).is_file()) })
    }

    fn list_bundle_chunks<'a>(&'a self) -> IoFuture<'a, Vec<String>> {
        Box::pin(async move {
            let entries = std::fs::read_dir(&self.base)
                .wrap_err_with(|| format!("read_dir {}", self.base.display()))?;
            let mut out = Vec::new();
            for entry in entries {
                let entry = entry?;
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if is_bundle_chunk(&name) {
                    out.push(name);
                }
            }
            out.sort();
            Ok(out)
        })
    }
}

// Matches `bundles.NN.nxa` where NN is exactly two ASCII digits.
fn is_bundle_chunk(name: &str) -> bool {
    let Some(stripped) = name
        .strip_prefix("bundles.")
        .and_then(|s| s.strip_suffix(".nxa"))
    else {
        return false;
    };
    stripped.len() == 2 && stripped.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_chunk_name_matcher() {
        assert!(is_bundle_chunk("bundles.00.nxa"));
        assert!(is_bundle_chunk("bundles.42.nxa"));
        assert!(!is_bundle_chunk("bundles.nxa"));
        assert!(!is_bundle_chunk("bundles.0.nxa"));
        assert!(!is_bundle_chunk("bundles.001.nxa"));
        assert!(!is_bundle_chunk("bundles.AB.nxa"));
        assert!(!is_bundle_chunk("notbundles.00.nxa"));
    }
}
