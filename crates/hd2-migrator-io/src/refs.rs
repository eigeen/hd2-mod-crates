//! FileID (u64) and SlotID (u32) rewriting inside Unit / Material blobs.
//!
//! Mirrors `mod_armor_migrator/refs.py`. We only touch fields where Stingray
//! stores cross-resource references; geometry / texture pixel data is copied
//! byte-for-byte.
//!
//! Unit blob layout (offsets in TocData):
//!
//! ```text
//! 0x00 u64 UnkRef1
//! 0x08 u64 BonesRef
//! 0x10 u64 CompositeRef
//! 0x18 u64 UnkRef2
//! 0x20 u64 StateMachineRef
//! 0x70 u32 MaterialsOffset
//!   @MaterialsOffset:
//!     u32 NumMaterials
//!     u32[NumMaterials] SectionsIDs   (slot IDs — also remapped)
//!     u64[NumMaterials] MaterialIDs   (FileIDs — remapped)
//! ```
//!
//! Material blob (TexIDs after section header):
//!
//! ```text
//! 0x40 u32 NumTextures
//!     ... TexUnks[NumTextures] (u32) ...
//!     u64[NumTextures] TexIDs   (FileIDs — remapped)
//! ```

use crate::constants::{MATERIAL_ID, UNIT_ID};
use byteorder::{ByteOrder, LittleEndian as LE};
use std::collections::HashMap;

const UNIT_HEADER_REFS: [usize; 5] = [0x00, 0x08, 0x10, 0x18, 0x20];
const UNIT_MATERIALS_OFFSET_FIELD: usize = 0x70;
const UNIT_MIN_LEN: usize = 0x80;
const MATERIAL_MIN_LEN: usize = 0x88;
const MATERIAL_NUM_TEXTURES_FIELD: usize = 0x40;
const MATERIAL_TEX_BASE: usize = 136;

#[inline]
fn remap_u64(value: u64, table: &HashMap<u64, u64>) -> u64 {
    table.get(&value).copied().unwrap_or(value)
}

pub fn rewrite_unit(
    toc_data: &[u8],
    remap: &HashMap<u64, u64>,
    slot_remap: Option<&HashMap<u32, u32>>,
) -> Vec<u8> {
    if toc_data.len() < UNIT_MIN_LEN {
        return toc_data.to_vec();
    }
    let mut buf = toc_data.to_vec();

    // 5 header u64 refs.
    for &off in &UNIT_HEADER_REFS {
        let old = LE::read_u64(&buf[off..off + 8]);
        let new = remap_u64(old, remap);
        if new != old {
            LE::write_u64(&mut buf[off..off + 8], new);
        }
    }

    // MaterialIDs[] u64 array under MaterialsOffset.
    if buf.len() >= UNIT_MATERIALS_OFFSET_FIELD + 4 {
        let materials_offset =
            LE::read_u32(&buf[UNIT_MATERIALS_OFFSET_FIELD..UNIT_MATERIALS_OFFSET_FIELD + 4])
                as usize;
        if materials_offset != 0 && materials_offset + 4 <= buf.len() {
            let num_materials = LE::read_u32(&buf[materials_offset..materials_offset + 4]) as usize;
            let mat_id_start = materials_offset + 4 + 4 * num_materials;
            let end = mat_id_start.saturating_add(8 * num_materials);
            if end <= buf.len() {
                for i in 0..num_materials {
                    let pos = mat_id_start + 8 * i;
                    let old = LE::read_u64(&buf[pos..pos + 8]);
                    let new = remap_u64(old, remap);
                    if new != old {
                        LE::write_u64(&mut buf[pos..pos + 8], new);
                    }
                }
            }
        }
    }

    // Whole-blob u32 SlotID scan — safe because slot IDs are murmur32 hashes.
    if let Some(slot_remap) = slot_remap {
        if !slot_remap.is_empty() {
            let mut pos = 0;
            while pos + 4 <= buf.len() {
                let val = LE::read_u32(&buf[pos..pos + 4]);
                if let Some(&new) = slot_remap.get(&val) {
                    if new != val {
                        LE::write_u32(&mut buf[pos..pos + 4], new);
                    }
                }
                pos += 4;
            }
        }
    }

    buf
}

pub fn rewrite_material(toc_data: &[u8], remap: &HashMap<u64, u64>) -> Vec<u8> {
    if toc_data.len() < MATERIAL_MIN_LEN {
        return toc_data.to_vec();
    }
    let mut buf = toc_data.to_vec();
    let num_textures =
        LE::read_u32(&buf[MATERIAL_NUM_TEXTURES_FIELD..MATERIAL_NUM_TEXTURES_FIELD + 4]) as usize;
    let tex_ids_start = MATERIAL_TEX_BASE + 4 * num_textures;
    let end = tex_ids_start.saturating_add(8 * num_textures);
    if num_textures == 0 || end > buf.len() {
        return buf;
    }
    for i in 0..num_textures {
        let pos = tex_ids_start + 8 * i;
        let old = LE::read_u64(&buf[pos..pos + 8]);
        let new = remap_u64(old, remap);
        if new != old {
            LE::write_u64(&mut buf[pos..pos + 8], new);
        }
    }
    buf
}

pub fn rewrite(
    type_id: u64,
    toc_data: &[u8],
    remap: &HashMap<u64, u64>,
    slot_remap: Option<&HashMap<u32, u32>>,
) -> Vec<u8> {
    match type_id {
        UNIT_ID => rewrite_unit(toc_data, remap, slot_remap),
        MATERIAL_ID => rewrite_material(toc_data, remap),
        _ => toc_data.to_vec(),
    }
}

pub fn list_unit_refs(toc_data: &[u8]) -> Vec<u64> {
    if toc_data.len() < UNIT_MIN_LEN {
        return Vec::new();
    }
    let mut out = Vec::new();
    for &off in &UNIT_HEADER_REFS {
        out.push(LE::read_u64(&toc_data[off..off + 8]));
    }
    let materials_offset =
        LE::read_u32(&toc_data[UNIT_MATERIALS_OFFSET_FIELD..UNIT_MATERIALS_OFFSET_FIELD + 4])
            as usize;
    if materials_offset != 0 && materials_offset + 4 <= toc_data.len() {
        let num_materials =
            LE::read_u32(&toc_data[materials_offset..materials_offset + 4]) as usize;
        let mat_id_start = materials_offset + 4 + 4 * num_materials;
        let end = mat_id_start.saturating_add(8 * num_materials);
        if end <= toc_data.len() {
            for i in 0..num_materials {
                let pos = mat_id_start + 8 * i;
                out.push(LE::read_u64(&toc_data[pos..pos + 8]));
            }
        }
    }
    out
}

pub fn list_material_refs(toc_data: &[u8]) -> Vec<u64> {
    if toc_data.len() < MATERIAL_MIN_LEN {
        return Vec::new();
    }
    let num_textures =
        LE::read_u32(&toc_data[MATERIAL_NUM_TEXTURES_FIELD..MATERIAL_NUM_TEXTURES_FIELD + 4])
            as usize;
    let tex_ids_start = MATERIAL_TEX_BASE + 4 * num_textures;
    let end = tex_ids_start.saturating_add(8 * num_textures);
    if num_textures == 0 || end > toc_data.len() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(num_textures);
    for i in 0..num_textures {
        let pos = tex_ids_start + 8 * i;
        out.push(LE::read_u64(&toc_data[pos..pos + 8]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_unit_blob(
        bones: u64,
        state_machine: u64,
        materials_offset: usize,
        material_ids: &[u64],
    ) -> Vec<u8> {
        let n = material_ids.len();
        let total = materials_offset + 4 + 4 * n + 8 * n;
        let mut buf = vec![0u8; total];
        LE::write_u64(&mut buf[0x08..0x10], bones);
        LE::write_u64(&mut buf[0x20..0x28], state_machine);
        LE::write_u32(&mut buf[0x70..0x74], materials_offset as u32);
        LE::write_u32(&mut buf[materials_offset..materials_offset + 4], n as u32);
        for (i, _) in material_ids.iter().enumerate() {
            LE::write_u32(
                &mut buf[materials_offset + 4 + 4 * i..materials_offset + 8 + 4 * i],
                0xDEAD_0000 + i as u32,
            );
        }
        let base = materials_offset + 4 + 4 * n;
        for (i, mid) in material_ids.iter().enumerate() {
            LE::write_u64(&mut buf[base + 8 * i..base + 8 * i + 8], *mid);
        }
        buf
    }

    fn make_material_blob(tex_ids: &[u64]) -> Vec<u8> {
        let n = tex_ids.len();
        let total = MATERIAL_TEX_BASE + 4 * n + 8 * n;
        let mut buf = vec![0u8; total.max(MATERIAL_MIN_LEN)];
        LE::write_u32(&mut buf[0x40..0x44], n as u32);
        for i in 0..n {
            LE::write_u32(
                &mut buf[MATERIAL_TEX_BASE + 4 * i..MATERIAL_TEX_BASE + 4 + 4 * i],
                0xCAFE_0000 + i as u32,
            );
        }
        let base = MATERIAL_TEX_BASE + 4 * n;
        for (i, t) in tex_ids.iter().enumerate() {
            LE::write_u64(&mut buf[base + 8 * i..base + 8 + 8 * i], *t);
        }
        buf
    }

    #[test]
    fn rewrite_unit_header_refs() {
        let blob = make_unit_blob(111, 222, 0x80, &[]);
        let mut remap = HashMap::new();
        remap.insert(111u64, 1111);
        remap.insert(222u64, 2222);
        let out = rewrite_unit(&blob, &remap, None);
        assert_eq!(LE::read_u64(&out[0x08..0x10]), 1111);
        assert_eq!(LE::read_u64(&out[0x20..0x28]), 2222);
    }

    #[test]
    fn rewrite_unit_material_ids() {
        let blob = make_unit_blob(0, 0, 0x80, &[10, 20, 30]);
        let mut remap = HashMap::new();
        remap.insert(10u64, 100);
        remap.insert(20u64, 200);
        remap.insert(30u64, 300);
        let out = rewrite_unit(&blob, &remap, None);
        let base = 0x80 + 4 + 4 * 3;
        assert_eq!(LE::read_u64(&out[base..base + 8]), 100);
        assert_eq!(LE::read_u64(&out[base + 8..base + 16]), 200);
        assert_eq!(LE::read_u64(&out[base + 16..base + 24]), 300);
    }

    #[test]
    fn rewrite_unit_slot_ids_whole_blob_scan() {
        let blob = make_unit_blob(0, 0, 0x80, &[0, 0]);
        let mut slot_remap = HashMap::new();
        slot_remap.insert(0xDEAD_0000u32, 0xBEEF_0000);
        slot_remap.insert(0xDEAD_0001u32, 0xBEEF_0001);
        let out = rewrite_unit(&blob, &HashMap::new(), Some(&slot_remap));
        let s0 = LE::read_u32(&out[0x80 + 4..0x80 + 8]);
        let s1 = LE::read_u32(&out[0x80 + 8..0x80 + 12]);
        assert_eq!(s0, 0xBEEF_0000);
        assert_eq!(s1, 0xBEEF_0001);
    }

    #[test]
    fn rewrite_material_tex_ids() {
        let blob = make_material_blob(&[0xA, 0xB, 0xC]);
        let mut remap = HashMap::new();
        remap.insert(0xAu64, 0xA1);
        remap.insert(0xBu64, 0xB1);
        remap.insert(0xCu64, 0xC1);
        let out = rewrite_material(&blob, &remap);
        let base = MATERIAL_TEX_BASE + 4 * 3;
        assert_eq!(LE::read_u64(&out[base..base + 8]), 0xA1);
        assert_eq!(LE::read_u64(&out[base + 8..base + 16]), 0xB1);
        assert_eq!(LE::read_u64(&out[base + 16..base + 24]), 0xC1);
    }

    #[test]
    fn list_unit_refs_covers_header_and_materials() {
        let blob = make_unit_blob(0xAA, 0xBB, 0x80, &[1, 2]);
        let refs = list_unit_refs(&blob);
        // Order: UnkRef1, BonesRef, CompositeRef, UnkRef2, StateMachineRef, MaterialIDs..
        assert_eq!(refs.len(), 5 + 2);
        assert_eq!(refs[1], 0xAA);
        assert_eq!(refs[4], 0xBB);
        assert_eq!(refs[5], 1);
        assert_eq!(refs[6], 2);
    }
}
