//! Embedded built-in empty mesh asset bytes.
//!
//! Extracted from `mod_armor_migrator/_builtin_empty_mesh.py` to
//! `assets/empty_mesh/{toc,gpu,stream}.bin` once; see README for the script.

pub static TOC: &[u8] = include_bytes!("../../assets/empty_mesh/toc.bin");
pub static GPU: &[u8] = include_bytes!("../../assets/empty_mesh/gpu.bin");
pub static STREAM: &[u8] = include_bytes!("../../assets/empty_mesh/stream.bin");
