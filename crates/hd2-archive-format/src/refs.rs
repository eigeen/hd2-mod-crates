//! Minimal in-memory reference rewrite helpers.

use std::collections::HashMap;

pub fn rewrite_u64_refs(data: &mut [u8], remap: &HashMap<u64, u64>) -> usize {
    let mut changed = 0;
    for chunk in data.chunks_exact_mut(8) {
        let current = u64::from_le_bytes(chunk.try_into().expect("exact chunk"));
        let Some(next) = remap.get(&current) else {
            continue;
        };
        chunk.copy_from_slice(&next.to_le_bytes());
        changed += 1;
    }
    changed
}
