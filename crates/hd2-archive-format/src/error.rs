use thiserror::Error;

pub type Result<T> = std::result::Result<T, MigratorError>;

#[derive(Debug, Error)]
pub enum MigratorError {
    #[error("bad magic: expected 0x{expected:08x}, got 0x{got:08x}")]
    BadMagic { expected: u32, got: u32 },
    #[error("{0}")]
    Message(String),
}

pub fn message(text: impl Into<String>) -> MigratorError {
    MigratorError::Message(text.into())
}
