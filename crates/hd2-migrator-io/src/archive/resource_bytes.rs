use std::ops::{Deref, Range};
use std::sync::Arc;

/// Immutable byte range backed by one shared owned buffer.
#[derive(Debug, Clone)]
pub struct ResourceBytes {
    storage: Arc<Vec<u8>>,
    range: Range<usize>,
}

impl ResourceBytes {
    pub(super) fn slice(&self, offset: usize, len: usize) -> Option<Self> {
        let start = self.range.start.checked_add(offset)?;
        let end = start.checked_add(len)?;
        if end > self.range.end {
            return None;
        }
        Some(Self {
            storage: self.storage.clone(),
            range: start..end,
        })
    }

    pub fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.storage, &other.storage)
    }
}

impl Default for ResourceBytes {
    fn default() -> Self {
        Self::from(Vec::new())
    }
}

impl Deref for ResourceBytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.storage[self.range.clone()]
    }
}

impl AsRef<[u8]> for ResourceBytes {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl PartialEq for ResourceBytes {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Eq for ResourceBytes {}

impl From<Vec<u8>> for ResourceBytes {
    fn from(bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        Self {
            storage: Arc::new(bytes),
            range: 0..len,
        }
    }
}

impl<const N: usize> From<[u8; N]> for ResourceBytes {
    fn from(bytes: [u8; N]) -> Self {
        Self::from(Vec::from(bytes))
    }
}
