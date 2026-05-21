use thiserror::Error;

pub type Result<T> = eyre::Result<T>;

#[derive(Debug, Error)]
pub enum MigratorError {
    #[error("bad magic: expected 0x{expected:08X}, got 0x{got:08X}")]
    BadMagic { expected: u32, got: u32 },

    #[error("unknown TypeID: 0x{0:016X}")]
    UnknownTypeId(u64),

    #[error("remap incomplete: {} unmatched FileIDs", unmatched.len())]
    RemapIncomplete { unmatched: Vec<u64> },

    #[error("empty mesh audit failed: {reason}")]
    EmptyMeshAuditFailed { reason: String },

    #[error("LZ4 decompression failed: {0}")]
    Lz4(String),

    #[error("malformed archive: {0}")]
    Malformed(String),
}
