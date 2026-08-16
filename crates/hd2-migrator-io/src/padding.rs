//! Empty-mesh padding to fill target-only Unit slots.
//!
//! Mirrors `mod_armor_migrator/padding.py`. Background: HD2's patch system
//! overrides FileIDs — if a patch contains FileID X, the game uses the
//! patch's entry; otherwise it loads the original. When a target armor has
//! more Unit slots than the source mod covers, those extras still render the
//! target's stock parts. To hide them, we insert empty-Unit entries for
//! every uncovered slot.

pub mod empty_mesh;

use crate::archive::{StreamToc, TocEntry};
use crate::constants::UNIT_ID;
use crate::refs;
use byteorder::{ByteOrder, LittleEndian as LE};
use eyre::WrapErr;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaddingMode {
    Disabled,
    Sanitized,
    Verbatim,
}

#[derive(Debug, Clone)]
pub struct EmptyUnitTemplate {
    pub name: String,
    pub toc_data: Vec<u8>,
    pub gpu_data: Arc<[u8]>,
    pub stream_data: Arc<[u8]>,
    pub source_file_id: u64,
    pub source_slot_ids: Vec<u32>,
}

impl EmptyUnitTemplate {
    /// Produce a new TocEntry from this template, targeting `target_file_id`.
    ///
    /// When `verbatim` is true, the template's bytes are written as-is and
    /// only the entry's top-level FileID changes. Otherwise the TocData is
    /// sanitized (zeroed material refs + index counts) and optionally remapped.
    pub fn clone_for(
        &self,
        target_file_id: u64,
        slot_id_remap: Option<&HashMap<u32, u32>>,
        file_id_remap: Option<&HashMap<u64, u64>>,
        verbatim: bool,
    ) -> TocEntry {
        let toc = if verbatim {
            self.toc_data.clone()
        } else {
            let sanitized = sanitize_empty_unit(&self.toc_data);
            let empty_remap = HashMap::new();
            refs::rewrite_unit(
                &sanitized,
                file_id_remap.unwrap_or(&empty_remap),
                slot_id_remap,
            )
        };
        let mut entry = TocEntry::new(target_file_id, UNIT_ID);
        entry.toc_data = toc;
        entry.gpu_data = self.gpu_data.clone();
        entry.stream_data = self.stream_data.clone();
        entry
    }
}

/// Heuristic: smallest GPU+TOC = most likely empty mesh.
pub fn find_empty_unit_candidates(patch: &StreamToc) -> Vec<&TocEntry> {
    let mut units: Vec<&TocEntry> = patch
        .entries
        .iter()
        .filter(|e| e.type_id == UNIT_ID)
        .collect();
    units.sort_by_key(|e| (e.gpu_data.len(), e.toc_data.len()));
    units
}

pub fn builtin_template() -> EmptyUnitTemplate {
    static CACHE: OnceLock<EmptyUnitTemplate> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let toc = StreamToc::from_buffers(
                empty_mesh::TOC,
                empty_mesh::GPU,
                empty_mesh::STREAM,
                "<builtin empty mesh>".to_string(),
            )
            .expect("builtin empty mesh patch must parse");
            let units: Vec<&TocEntry> = toc
                .entries
                .iter()
                .filter(|e| e.type_id == UNIT_ID)
                .collect();
            let chosen = units
                .first()
                .copied()
                .expect("builtin empty mesh patch has no Unit entries");
            EmptyUnitTemplate {
                name: "<builtin empty mesh>".to_string(),
                toc_data: chosen.toc_data.clone(),
                gpu_data: chosen.gpu_data.clone(),
                stream_data: chosen.stream_data.clone(),
                source_file_id: chosen.file_id,
                source_slot_ids: scan_u32_words(&chosen.toc_data),
            }
        })
        .clone()
}

pub fn extract_template(patch_path: &Path) -> crate::Result<EmptyUnitTemplate> {
    let patch = StreamToc::from_files(patch_path)
        .wrap_err_with(|| format!("read empty-mesh source {}", patch_path.display()))?;
    let candidates = find_empty_unit_candidates(&patch);
    let chosen = candidates
        .first()
        .copied()
        .ok_or_else(|| eyre::eyre!("no Unit entries in {}", patch_path.display()))?;
    Ok(EmptyUnitTemplate {
        name: patch_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<custom>")
            .to_string(),
        toc_data: chosen.toc_data.clone(),
        gpu_data: chosen.gpu_data.clone(),
        stream_data: chosen.stream_data.clone(),
        source_file_id: chosen.file_id,
        source_slot_ids: scan_u32_words(&chosen.toc_data),
    })
}

fn scan_u32_words(buf: &[u8]) -> Vec<u32> {
    let mut out = Vec::with_capacity(buf.len() / 4);
    let mut pos = 0;
    while pos + 4 <= buf.len() {
        out.push(LE::read_u32(&buf[pos..pos + 4]));
        pos += 4;
    }
    out
}

// ---------- Audit / sanitization ------------------------------------------

#[derive(Debug, Clone)]
pub struct EmptyUnitAudit {
    pub header_refs: [u64; 5],
    pub material_ids: Vec<u64>,
    pub num_indices: u32,
    pub section_indices: Vec<u32>,
}

impl EmptyUnitAudit {
    pub fn external_refs(&self) -> Vec<u64> {
        let mut out: Vec<u64> = self
            .header_refs
            .iter()
            .copied()
            .filter(|&v| v != 0)
            .collect();
        out.extend(self.material_ids.iter().copied().filter(|&v| v != 0));
        out
    }
    pub fn is_dependency_free(&self) -> bool {
        self.external_refs().is_empty()
    }
    pub fn is_non_drawing(&self) -> bool {
        self.num_indices == 0 && self.section_indices.iter().all(|&v| v == 0)
    }
}

pub fn audit_empty_unit(toc_data: &[u8]) -> EmptyUnitAudit {
    let mut header_refs = [0u64; 5];
    if toc_data.len() >= 0x28 {
        for (i, off) in [0x00usize, 0x08, 0x10, 0x18, 0x20].iter().enumerate() {
            header_refs[i] = LE::read_u64(&toc_data[*off..*off + 8]);
        }
    }
    let material_ids = read_material_ids(toc_data);
    let section_indices = read_section_index_counts(toc_data);
    let num_indices = section_indices.iter().sum();
    EmptyUnitAudit {
        header_refs,
        material_ids,
        num_indices,
        section_indices,
    }
}

pub fn sanitize_empty_unit(toc_data: &[u8]) -> Vec<u8> {
    let mut buf = toc_data.to_vec();
    zero_global_material_refs(&mut buf);
    zero_stream_indices(&mut buf);
    zero_section_indices(&mut buf);
    buf
}

fn header_offsets(toc_data: &[u8]) -> (usize, usize, usize) {
    if toc_data.len() < 0x74 {
        return (0, 0, 0);
    }
    let stream_off = LE::read_u32(&toc_data[0x5C..0x60]) as usize;
    // 0x60..0x64 is ending_off; unused here.
    let mesh_off = LE::read_u32(&toc_data[0x64..0x68]) as usize;
    let materials_off = LE::read_u32(&toc_data[0x70..0x74]) as usize;
    (stream_off, mesh_off, materials_off)
}

fn read_material_ids(toc_data: &[u8]) -> Vec<u64> {
    let (_, _, materials_off) = header_offsets(toc_data);
    if materials_off == 0 || materials_off + 4 > toc_data.len() {
        return Vec::new();
    }
    let num_mats = LE::read_u32(&toc_data[materials_off..materials_off + 4]) as usize;
    let ids_off = materials_off + 4 + 4 * num_mats;
    let end = ids_off.saturating_add(8 * num_mats);
    if end > toc_data.len() {
        return Vec::new();
    }
    (0..num_mats)
        .map(|i| LE::read_u64(&toc_data[ids_off + 8 * i..ids_off + 8 * i + 8]))
        .collect()
}

fn stream_info_bases(toc_data: &[u8]) -> Vec<usize> {
    let (stream_off, _, _) = header_offsets(toc_data);
    if stream_off == 0 || stream_off + 4 > toc_data.len() {
        return Vec::new();
    }
    let num_streams = LE::read_u32(&toc_data[stream_off..stream_off + 4]) as usize;
    let offsets_at = stream_off + 4;
    (0..num_streams)
        .filter_map(|i| {
            let p = offsets_at + 4 * i;
            if p + 4 > toc_data.len() {
                return None;
            }
            let rel = LE::read_u32(&toc_data[p..p + 4]) as usize;
            let base = stream_off + rel;
            if base + 416 <= toc_data.len() {
                Some(base)
            } else {
                None
            }
        })
        .collect()
}

fn mesh_info_bases(toc_data: &[u8]) -> Vec<usize> {
    let (_, mesh_off, _) = header_offsets(toc_data);
    if mesh_off == 0 || mesh_off + 4 > toc_data.len() {
        return Vec::new();
    }
    let num_meshes = LE::read_u32(&toc_data[mesh_off..mesh_off + 4]) as usize;
    let offsets_at = mesh_off + 4;
    (0..num_meshes)
        .filter_map(|i| {
            let p = offsets_at + 4 * i;
            if p + 4 > toc_data.len() {
                return None;
            }
            let rel = LE::read_u32(&toc_data[p..p + 4]) as usize;
            let base = mesh_off + rel;
            if base + 128 <= toc_data.len() {
                Some(base)
            } else {
                None
            }
        })
        .collect()
}

fn section_offsets(toc_data: &[u8]) -> Vec<usize> {
    let mut offsets = Vec::new();
    for base in mesh_info_bases(toc_data) {
        if base + 128 > toc_data.len() {
            continue;
        }
        let num_sections = LE::read_u32(&toc_data[base + 120..base + 124]) as usize;
        let section_rel = LE::read_u32(&toc_data[base + 124..base + 128]) as usize;
        for i in 0..num_sections {
            let off = base + section_rel + 24 * i;
            if off + 24 <= toc_data.len() {
                offsets.push(off);
            }
        }
    }
    offsets
}

fn read_section_index_counts(toc_data: &[u8]) -> Vec<u32> {
    section_offsets(toc_data)
        .iter()
        .filter_map(|&off| {
            if off + 20 <= toc_data.len() {
                Some(LE::read_u32(&toc_data[off + 16..off + 20]))
            } else {
                None
            }
        })
        .collect()
}

fn zero_global_material_refs(buf: &mut [u8]) {
    let ids = read_material_ids(buf);
    let (_, _, materials_off) = header_offsets(buf);
    if materials_off == 0 {
        return;
    }
    let ids_off = materials_off + 4 + 4 * ids.len();
    for i in 0..ids.len() {
        let p = ids_off + 8 * i;
        if p + 8 <= buf.len() {
            LE::write_u64(&mut buf[p..p + 8], 0);
        }
    }
}

fn zero_stream_indices(buf: &mut [u8]) {
    for base in stream_info_bases(buf) {
        if base + 416 <= buf.len() {
            LE::write_u32(&mut buf[base + 384..base + 388], 0);
            LE::write_u32(&mut buf[base + 412..base + 416], 0);
        }
    }
}

fn zero_section_indices(buf: &mut [u8]) {
    for off in section_offsets(buf) {
        if off + 20 <= buf.len() {
            LE::write_u32(&mut buf[off + 16..off + 20], 0);
        }
    }
}

// ---------- Slot-list inference from reference variants -----------------

pub fn collect_target_slot_lists(
    reference_dir: &Path,
    patch_filename: &str,
) -> crate::Result<HashMap<String, Vec<u64>>> {
    let mut out: HashMap<String, Vec<u64>> = HashMap::new();
    for entry in std::fs::read_dir(reference_dir)
        .wrap_err_with(|| format!("read_dir {}", reference_dir.display()))?
    {
        let entry = entry?;
        let sub = entry.path();
        if !sub.is_dir() {
            continue;
        }
        let path = sub.join(patch_filename);
        if !path.exists() {
            continue;
        }
        let p = StreamToc::from_files(&path)?;
        let fids: Vec<u64> = p
            .entries
            .iter()
            .filter(|e| e.type_id == UNIT_ID)
            .map(|e| e.file_id)
            .collect();
        let name = sub
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        out.insert(name, fids);
    }
    Ok(out)
}

// ---------- Padding -----------------------------------------------------

pub fn pad_patch(
    patch: &mut StreamToc,
    target_full_unit_slots: &[u64],
    template: &EmptyUnitTemplate,
    mode: PaddingMode,
    slot_id_remap: Option<&HashMap<u32, u32>>,
) -> Vec<u64> {
    if mode == PaddingMode::Disabled {
        return Vec::new();
    }
    let covered: HashSet<u64> = patch
        .entries
        .iter()
        .filter(|e| e.type_id == UNIT_ID)
        .map(|e| e.file_id)
        .collect();
    let extras: Vec<u64> = target_full_unit_slots
        .iter()
        .copied()
        .filter(|fid| !covered.contains(fid))
        .collect();
    let verbatim = matches!(mode, PaddingMode::Verbatim);
    for &fid in &extras {
        let entry = template.clone_for(fid, slot_id_remap, None, verbatim);
        patch.entries.push(entry);
    }
    extras
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_template_loads() {
        let t = builtin_template();
        assert!(!t.toc_data.is_empty());
        assert!(t.source_file_id != 0);
    }

    #[test]
    fn builtin_audit_is_non_drawing_after_sanitize() {
        let t = builtin_template();
        let sanitized = sanitize_empty_unit(&t.toc_data);
        let audit = audit_empty_unit(&sanitized);
        assert!(
            audit.is_non_drawing(),
            "expected non-drawing audit: {:?}",
            audit
        );
    }
}
