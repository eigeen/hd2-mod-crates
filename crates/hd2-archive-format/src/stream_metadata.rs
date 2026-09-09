//! Shared stream-size policy for full archive reads, TOC editing and import checks.

use crate::constants::{BONE_ID, TEX_ID, UNIT_ID};
use crate::error::{MigratorError, Result, message};

pub const POISON_STREAM_SIZE: u32 = 0xDFA6_4C92;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamMetadataRepair {
    pub file_id: u64,
    pub type_id: u64,
    pub original_size: u32,
    pub repaired_size: u32,
    pub poison_value_detected: bool,
}

pub struct StreamDeclaration<'a> {
    pub file_id: u64,
    pub type_id: u64,
    pub offset: u64,
    pub size: u32,
    pub toc_data: &'a [u8],
}

pub struct ResolvedStreamSize {
    pub size: u32,
    pub repair: Option<StreamMetadataRepair>,
}

/// Resolve before slicing stream bytes. `None` means the caller has only a TOC;
/// it must not be treated as an empty stream or used to infer missing payloads.
pub fn resolve_stream_size(
    declaration: StreamDeclaration<'_>,
    stream_len: Option<usize>,
) -> Result<ResolvedStreamSize> {
    if !declaration_mismatches(&declaration, stream_len) {
        return Ok(ResolvedStreamSize {
            size: declaration.size,
            repair: None,
        });
    }
    let size = calculate_actual_size(&declaration, stream_len)?;
    if let Some(length) = stream_len
        && !interval_is_in_bounds(declaration.offset, size, length)
    {
        return Err(resource_error(
            &declaration,
            stream_len,
            &format!("calculated stream size {size} is still out of bounds"),
        ));
    }
    Ok(ResolvedStreamSize {
        size,
        repair: Some(StreamMetadataRepair {
            file_id: declaration.file_id,
            type_id: declaration.type_id,
            original_size: declaration.size,
            repaired_size: size,
            poison_value_detected: declaration.size == POISON_STREAM_SIZE,
        }),
    })
}

fn declaration_mismatches(entry: &StreamDeclaration<'_>, stream_len: Option<usize>) -> bool {
    if matches!(entry.type_id, UNIT_ID | BONE_ID) {
        return entry.size != 0;
    }
    match stream_len {
        Some(length) => !interval_is_in_bounds(entry.offset, entry.size, length),
        None => entry.size == POISON_STREAM_SIZE,
    }
}

/// Unit/Bones have no stream payload; textures store streamed bytes in the first
/// mip's BytesLeft field. Other non-empty payloads need an exact size calculator.
fn calculate_actual_size(entry: &StreamDeclaration<'_>, stream_len: Option<usize>) -> Result<u32> {
    if matches!(entry.type_id, UNIT_ID | BONE_ID) || stream_len == Some(0) {
        return Ok(0);
    }
    if entry.type_id == TEX_ID {
        let bytes = entry
            .toc_data
            .get(16..20)
            .ok_or_else(|| message("texture TOC body is too small to read first mip BytesLeft"))?;
        return Ok(u32::from_le_bytes(
            bytes.try_into().expect("four-byte mip size"),
        ));
    }
    Err(resource_error(
        entry,
        stream_len,
        "the resource type has no exact stream-size calculator",
    ))
}

fn interval_is_in_bounds(offset: u64, size: u32, stream_len: usize) -> bool {
    size == 0
        || offset
            .checked_add(u64::from(size))
            .is_some_and(|end| end <= stream_len as u64)
}

fn resource_error(
    entry: &StreamDeclaration<'_>,
    stream_len: Option<usize>,
    reason: &str,
) -> MigratorError {
    let length = stream_len.map_or_else(|| "unknown".to_owned(), |value| value.to_string());
    message(format!(
        "resource type=0x{:016x}, file=0x{:016x} has mismatched stream metadata: offset={}, size={}, streamLength={length}; {reason}",
        entry.type_id, entry.file_id, entry.offset, entry.size,
    ))
}

#[cfg(test)]
mod tests;
