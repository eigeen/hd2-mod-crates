use std::error::Error as StdError;

pub type Error = Box<dyn StdError + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

/// Builds a lightweight error for validation failures and missing inputs.
pub fn message(text: impl Into<String>) -> Error {
    text.into().into()
}
