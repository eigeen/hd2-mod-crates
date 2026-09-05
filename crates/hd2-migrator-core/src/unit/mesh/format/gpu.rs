use crate::archive::ResourceBytes;
use std::ops::Deref;

#[derive(Debug)]
pub(super) enum UnitGpuData {
    Shared(ResourceBytes),
    Owned(Vec<u8>),
}

pub(super) enum UnitGpuCheckpoint {
    Shared(ResourceBytes),
    OwnedLen(usize),
}

impl UnitGpuData {
    pub(super) fn checkpoint(&self) -> UnitGpuCheckpoint {
        match self {
            Self::Shared(bytes) => UnitGpuCheckpoint::Shared(bytes.clone()),
            Self::Owned(bytes) => UnitGpuCheckpoint::OwnedLen(bytes.len()),
        }
    }

    pub(super) fn make_owned(&mut self) -> crate::Result<&mut Vec<u8>> {
        let owned = match self {
            Self::Shared(bytes) => Some(copy_with_capacity(bytes, 0)?),
            Self::Owned(_) => None,
        };
        if let Some(bytes) = owned {
            *self = Self::Owned(bytes);
        }
        match self {
            Self::Owned(bytes) => Ok(bytes),
            Self::Shared(_) => unreachable!("shared GPU data was not promoted"),
        }
    }

    pub(super) fn reserve_growth(&mut self, additional: usize) -> crate::Result<()> {
        if additional == 0 {
            return Ok(());
        }
        match self {
            Self::Shared(bytes) => *self = Self::Owned(copy_with_capacity(bytes, additional)?),
            Self::Owned(bytes) => reserve_owned_growth(bytes, additional)?,
        }
        Ok(())
    }

    pub(super) fn restore(&mut self, checkpoint: UnitGpuCheckpoint) {
        match checkpoint {
            UnitGpuCheckpoint::Shared(bytes) => *self = Self::Shared(bytes),
            UnitGpuCheckpoint::OwnedLen(len) => match self {
                Self::Owned(bytes) => bytes.truncate(len),
                Self::Shared(_) => unreachable!("owned GPU checkpoint restored into shared data"),
            },
        }
    }

    pub(super) fn into_shared(self) -> ResourceBytes {
        match self {
            Self::Shared(bytes) => bytes,
            Self::Owned(bytes) => ResourceBytes::from(bytes),
        }
    }
}

impl Deref for UnitGpuData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Shared(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

fn copy_with_capacity(bytes: &[u8], additional: usize) -> crate::Result<Vec<u8>> {
    let capacity = bytes
        .len()
        .checked_add(additional)
        .ok_or_else(|| eyre::eyre!("merged GPU buffer size overflow"))?;
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(capacity)
        .map_err(|_| eyre::eyre!("not enough memory for merged GPU buffer"))?;
    owned.extend_from_slice(bytes);
    Ok(owned)
}

fn reserve_owned_growth(bytes: &mut Vec<u8>, additional: usize) -> crate::Result<()> {
    bytes
        .try_reserve_exact(additional)
        .map_err(|_| eyre::eyre!("not enough memory for merged GPU buffer"))
}
