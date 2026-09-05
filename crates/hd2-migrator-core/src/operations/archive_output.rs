//! Shared archive output manifest and serialization boundary.

use crate::archive::{SerializedPart, StreamToc, StreamTocSerializer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveOutputFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveOutputEntry {
    pub path: String,
    pub part: SerializedPart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveOutputManifest {
    entries: [ArchiveOutputEntry; 3],
}

impl ArchiveOutputManifest {
    pub fn new(toc_path: impl Into<String>) -> Self {
        let toc_path = toc_path.into();
        Self {
            entries: [
                ArchiveOutputEntry {
                    path: toc_path.clone(),
                    part: SerializedPart::Toc,
                },
                ArchiveOutputEntry {
                    path: format!("{toc_path}.gpu_resources"),
                    part: SerializedPart::Gpu,
                },
                ArchiveOutputEntry {
                    path: format!("{toc_path}.stream"),
                    part: SerializedPart::Stream,
                },
            ],
        }
    }

    pub fn entries(&self) -> &[ArchiveOutputEntry; 3] {
        &self.entries
    }
}

pub struct PreparedArchiveOutput {
    archive: StreamToc,
    manifest: ArchiveOutputManifest,
}

impl PreparedArchiveOutput {
    pub fn new(archive: StreamToc, toc_path: impl Into<String>) -> Self {
        Self {
            archive,
            manifest: ArchiveOutputManifest::new(toc_path),
        }
    }

    pub fn manifest(&self) -> &ArchiveOutputManifest {
        &self.manifest
    }

    pub fn serializer(&mut self) -> StreamTocSerializer<'_> {
        self.archive.serializer()
    }

    pub fn into_files(mut self) -> crate::Result<Vec<ArchiveOutputFile>> {
        let mut files = Vec::with_capacity(self.manifest.entries().len());
        self.write_files(|file| {
            files.push(file);
            Ok(())
        })?;
        Ok(files)
    }

    /// Serialize one entry at a time and release it after the sink accepts it.
    pub fn write_files<F>(&mut self, mut write_file: F) -> crate::Result<()>
    where
        F: FnMut(ArchiveOutputFile) -> crate::Result<()>,
    {
        let serializer = self.archive.serializer();
        for entry in self.manifest.entries() {
            write_file(serialize_entry(&serializer, entry)?)?;
        }
        Ok(())
    }
}

fn serialize_entry(
    serializer: &StreamTocSerializer<'_>,
    entry: &ArchiveOutputEntry,
) -> crate::Result<ArchiveOutputFile> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(serializer.part_len(entry.part))?;
    serializer.write_part(entry.part, &mut bytes)?;
    Ok(ArchiveOutputFile {
        path: entry.path.clone(),
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::TocEntry;

    #[test]
    fn manifest_and_buffered_serialization_keep_canonical_order() {
        let mut archive = StreamToc::default();
        let mut entry = TocEntry::new(7, 11);
        entry.toc_data = b"toc".to_vec();
        entry.gpu_data = b"gpu".to_vec().into();
        entry.stream_data = b"stream".to_vec().into();
        archive.entries.push(entry);

        let output = PreparedArchiveOutput::new(archive, "folder/example.patch");
        let paths = output
            .manifest()
            .entries()
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                "folder/example.patch".to_owned(),
                "folder/example.patch.gpu_resources".to_owned(),
                "folder/example.patch.stream".to_owned(),
            ]
        );

        let files = output.into_files().expect("serialize archive output");
        assert_eq!(
            files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
            paths
        );
        assert!(!files[0].bytes.is_empty());
        assert_eq!(files[1].bytes, b"gpu");
        assert_eq!(files[2].bytes, b"stream");
    }
}
