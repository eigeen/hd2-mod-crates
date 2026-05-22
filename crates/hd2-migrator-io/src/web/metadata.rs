use crate::archive::{StreamToc, TocEntry};
use byteorder::{ByteOrder, LittleEndian as LE};
use eyre::WrapErr;
use serde::{Deserialize, Serialize};

pub const WEB_METADATA_SCHEMA_VERSION: u32 = 1;
const TOC_FILE_TYPE_SIZE: usize = 32;
const TOC_ENTRY_SIZE: usize = 80;
const HEADER_BASE: usize = 72;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebGameMetadata {
    pub schema_version: u32,
    pub category: String,
    pub targets: Vec<WebArchiveMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebTargetOption {
    pub hash: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebArchiveMetadata {
    pub hash: String,
    pub name: String,
    pub archive: WebArchive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebArchive {
    pub name: String,
    pub unknown: u32,
    pub unk4_data: Vec<u8>,
    pub entries: Vec<WebTocEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebTocEntry {
    pub file_id: u64,
    pub type_id: u64,
    pub unknown1: u64,
    pub unknown2: u64,
    pub unknown3: u32,
    pub unknown4: u32,
    pub entry_index: u32,
    pub toc_size: usize,
    pub gpu_size: usize,
    pub stream_size: usize,
    pub unit_refs: Vec<u64>,
    pub material_refs: Vec<u64>,
}

impl WebGameMetadata {
    pub fn new(category: impl Into<String>, targets: Vec<WebArchiveMetadata>) -> Self {
        Self {
            schema_version: WEB_METADATA_SCHEMA_VERSION,
            category: category.into(),
            targets,
        }
    }

    pub fn target_options(&self) -> Vec<WebTargetOption> {
        self.targets
            .iter()
            .map(|target| WebTargetOption {
                hash: target.hash.clone(),
                name: target.name.clone(),
            })
            .collect()
    }

    pub fn archive(&self, hash: &str) -> Option<&WebArchiveMetadata> {
        self.targets.iter().find(|target| target.hash == hash)
    }
}

impl WebArchiveMetadata {
    pub fn from_stream(hash: String, name: String, archive: &StreamToc) -> Self {
        Self {
            hash,
            name,
            archive: WebArchive::from_stream(archive),
        }
    }

    pub fn from_toc_bytes(hash: String, name: String, toc_data: &[u8]) -> crate::Result<Self> {
        let archive = WebArchive::from_toc_bytes(hash.clone(), toc_data)
            .wrap_err_with(|| format!("parse metadata for {name} ({hash})"))?;
        Ok(Self {
            hash,
            name,
            archive,
        })
    }
}

impl WebArchive {
    pub fn from_stream(archive: &StreamToc) -> Self {
        Self {
            name: archive.name.clone(),
            unknown: archive.unknown,
            unk4_data: archive.unk4_data.to_vec(),
            entries: archive
                .entries
                .iter()
                .map(WebTocEntry::from_entry)
                .collect(),
        }
    }

    pub fn unit_file_ids(&self) -> Vec<u64> {
        self.file_ids_by_type(crate::constants::UNIT_ID)
    }

    pub fn file_ids_by_type(&self, type_id: u64) -> Vec<u64> {
        self.entries
            .iter()
            .filter(|entry| entry.type_id == type_id)
            .map(|entry| entry.file_id)
            .collect()
    }

    fn from_toc_bytes(name: String, toc_data: &[u8]) -> crate::Result<Self> {
        validate_toc_header(toc_data)?;
        let num_types = LE::read_u32(&toc_data[4..8]) as usize;
        let num_files = LE::read_u32(&toc_data[8..12]) as usize;
        let unknown = LE::read_u32(&toc_data[12..16]);
        let mut unk4_data = vec![0u8; 56];
        unk4_data.copy_from_slice(&toc_data[16..72]);
        let entries_start = HEADER_BASE + num_types * TOC_FILE_TYPE_SIZE;
        let bodies_start = entries_start + num_files * TOC_ENTRY_SIZE;
        if toc_data.len() < bodies_start {
            eyre::bail!(
                "toc truncated: header expects {} bytes, got {}",
                bodies_start,
                toc_data.len()
            );
        }
        let entries = (0..num_files)
            .map(|index| web_entry_from_toc(toc_data, entries_start, index))
            .collect::<crate::Result<Vec<_>>>()?;
        Ok(Self {
            name,
            unknown,
            unk4_data,
            entries,
        })
    }
}

impl WebTocEntry {
    fn from_entry(entry: &TocEntry) -> Self {
        Self {
            file_id: entry.file_id,
            type_id: entry.type_id,
            unknown1: entry.unknown1,
            unknown2: entry.unknown2,
            unknown3: entry.unknown3,
            unknown4: entry.unknown4,
            entry_index: entry.entry_index,
            toc_size: entry.toc_data.len(),
            gpu_size: entry.gpu_data.len(),
            stream_size: entry.stream_data.len(),
            unit_refs: crate::refs::list_unit_refs(&entry.toc_data),
            material_refs: crate::refs::list_material_refs(&entry.toc_data),
        }
    }
}

fn validate_toc_header(toc_data: &[u8]) -> crate::Result<()> {
    if toc_data.len() < HEADER_BASE {
        eyre::bail!("toc too small: {} bytes", toc_data.len());
    }
    let magic = LE::read_u32(&toc_data[0..4]);
    if magic != crate::constants::LEGACY_MAGIC {
        return Err(crate::error::MigratorError::BadMagic {
            expected: crate::constants::LEGACY_MAGIC,
            got: magic,
        }
        .into());
    }
    Ok(())
}

fn web_entry_from_toc(
    toc_data: &[u8],
    entries_start: usize,
    index: usize,
) -> crate::Result<WebTocEntry> {
    let off = entries_start + index * TOC_ENTRY_SIZE;
    let hdr = &toc_data[off..off + TOC_ENTRY_SIZE];
    let toc_offset = LE::read_u64(&hdr[16..24]) as usize;
    let toc_size = LE::read_u32(&hdr[56..60]) as usize;
    let toc_end = toc_offset
        .checked_add(toc_size)
        .ok_or_else(|| eyre::eyre!("toc body offset overflow for entry {index}"))?;
    let toc_body = toc_data
        .get(toc_offset..toc_end)
        .ok_or_else(|| eyre::eyre!("toc body OOB for entry {index}"))?;
    Ok(WebTocEntry {
        file_id: LE::read_u64(&hdr[0..8]),
        type_id: LE::read_u64(&hdr[8..16]),
        unknown1: LE::read_u64(&hdr[40..48]),
        unknown2: LE::read_u64(&hdr[48..56]),
        unknown3: LE::read_u32(&hdr[68..72]),
        unknown4: LE::read_u32(&hdr[72..76]),
        entry_index: LE::read_u32(&hdr[76..80]),
        toc_size,
        gpu_size: LE::read_u32(&hdr[64..68]) as usize,
        stream_size: LE::read_u32(&hdr[60..64]) as usize,
        unit_refs: crate::refs::list_unit_refs(toc_body),
        material_refs: crate::refs::list_material_refs(toc_body),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::UNIT_ID;

    #[test]
    fn metadata_round_trips_archive_entries() {
        let archive = StreamToc {
            entries: vec![unit_entry(42)],
            unknown: 7,
            unk4_data: [3; 56],
            name: "source".to_string(),
            ..Default::default()
        };
        let metadata =
            WebArchiveMetadata::from_stream("hash".to_string(), "Armor".to_string(), &archive);

        assert_eq!(metadata.archive.name, "source");
        assert_eq!(metadata.archive.unknown, 7);
        assert_eq!(metadata.archive.unk4_data, vec![3; 56]);
        assert_eq!(metadata.archive.entries.len(), 1);
        assert_eq!(metadata.archive.entries[0].file_id, 42);
        assert_eq!(metadata.archive.entries[0].toc_size, 3);
        assert_eq!(metadata.archive.entries[0].gpu_size, 2);
        assert_eq!(metadata.archive.entries[0].stream_size, 1);
    }

    #[test]
    fn game_metadata_lists_target_options() {
        let metadata = WebGameMetadata::new(
            "Armor",
            vec![WebArchiveMetadata::from_stream(
                "abc".to_string(),
                "Example".to_string(),
                &StreamToc::default(),
            )],
        );

        assert_eq!(metadata.target_options()[0].hash, "abc");
        assert_eq!(metadata.archive("abc").unwrap().name, "Example");
    }

    fn unit_entry(file_id: u64) -> TocEntry {
        let mut entry = TocEntry::new(file_id, UNIT_ID);
        entry.toc_data = vec![1, 2, 3];
        entry.gpu_data = vec![4, 5];
        entry.stream_data = vec![6];
        entry
    }
}
