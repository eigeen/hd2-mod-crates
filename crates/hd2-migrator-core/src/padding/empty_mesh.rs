//! Embedded built-in empty mesh asset bytes.
//!
//! Extracted from `mod_armor_migrator/_builtin_empty_mesh.py` to
//! `assets/empty_mesh/{toc,gpu,stream}.bin` once; see README for the script.

pub static TOC: &[u8] = hd2_migrator_data::EMPTY_MESH_TOC;
pub static GPU: &[u8] = hd2_migrator_data::EMPTY_MESH_GPU;
pub static STREAM: &[u8] = hd2_migrator_data::EMPTY_MESH_STREAM;
