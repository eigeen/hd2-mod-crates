use crate::error::{Result, message};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

/// Sentinel placed between the exporter executable and the embedded package payload.
/// The first occurrence (scanned from the end) whose tail begins with the ZIP local-file
/// signature `PK\x03\x04` marks the start of the embedded svd package.
pub const SFX_GUARD: &[u8] = b"\n--SVD-SFX-V1-cR1zX9HpQ2DvN6mTfB8sLkW7uYjE5oA3-END--\n";

const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// Writes `exporter || SFX_GUARD || contents_of(package_zip)` to `output_exe`.
pub fn write_self_extracting(
    exporter: &[u8],
    package_zip: &Path,
    output_exe: &Path,
) -> Result<()> {
    if exporter.is_empty() {
        return Err(message("cannot build self-extracting exe: exporter bytes are empty"));
    }
    if let Some(parent) = output_exe.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = File::create(output_exe)?;
    out.write_all(exporter)?;
    out.write_all(SFX_GUARD)?;
    let mut zip = File::open(package_zip)?;
    std::io::copy(&mut zip, &mut out)?;
    out.flush()?;
    Ok(())
}

/// Inspects the current executable for an embedded package and, when found, writes
/// the payload to a temporary file whose lifetime the caller must keep.
pub fn load_embedded_package() -> Result<Option<NamedTempFile>> {
    let exe = std::env::current_exe()?;
    let bytes = std::fs::read(&exe)?;
    let Some(start) = find_validated_guard(&bytes, SFX_GUARD) else {
        return Ok(None);
    };
    let payload = &bytes[start + SFX_GUARD.len()..];
    let file = NamedTempFile::new()?;
    std::fs::write(file.path(), payload)?;
    Ok(Some(file))
}

fn find_validated_guard(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    let first = needle[0];
    let last_idx = haystack.len() - needle.len();
    for i in (0..=last_idx).rev() {
        if haystack[i] != first {
            continue;
        }
        if haystack[i..i + needle.len()] != *needle {
            continue;
        }
        let after = i + needle.len();
        if after + ZIP_MAGIC.len() <= haystack.len()
            && haystack[after..after + ZIP_MAGIC.len()] == ZIP_MAGIC
        {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_alone_without_zip_magic_is_ignored() {
        let mut buf = b"prefix".to_vec();
        buf.extend_from_slice(SFX_GUARD);
        buf.extend_from_slice(b"not a zip");
        assert!(find_validated_guard(&buf, SFX_GUARD).is_none());
    }

    #[test]
    fn guard_followed_by_zip_magic_is_found() {
        let mut buf = b"exporter-bytes".to_vec();
        buf.extend_from_slice(SFX_GUARD);
        buf.extend_from_slice(&ZIP_MAGIC);
        buf.extend_from_slice(b"rest");
        let pos = find_validated_guard(&buf, SFX_GUARD).expect("guard found");
        assert_eq!(pos, b"exporter-bytes".len());
    }

    #[test]
    fn last_validated_guard_wins() {
        let mut buf = b"intro".to_vec();
        buf.extend_from_slice(SFX_GUARD);
        buf.extend_from_slice(b"decoy-no-zip");
        let split_offset = buf.len();
        buf.extend_from_slice(SFX_GUARD);
        buf.extend_from_slice(&ZIP_MAGIC);
        let pos = find_validated_guard(&buf, SFX_GUARD).expect("guard found");
        assert_eq!(pos, split_offset);
    }
}
