use super::*;

struct SlotSets<'a> {
    target: &'a HashSet<u32>,
    source: &'a HashSet<u32>,
}

pub(super) fn estimate_gpu_growth(
    target: &ParsedUnit,
    source: &ParsedUnit,
    lods: &[i32],
) -> crate::Result<usize> {
    let target_slots = target.normal_slots();
    let source_slots = source.normal_slots();
    let slots = SlotSets {
        target: &target_slots,
        source: &source_slots,
    };
    lods.iter().try_fold(0usize, |total, lod| {
        let growth = estimate_lod_growth(target, source, *lod, &slots)?;
        total
            .checked_add(growth.unwrap_or(0))
            .ok_or_else(|| eyre::eyre!("merged GPU buffer size overflow"))
    })
}

fn estimate_lod_growth(
    target_unit: &ParsedUnit,
    source: &ParsedUnit,
    target_lod: i32,
    slots: &SlotSets<'_>,
) -> crate::Result<Option<usize>> {
    let host_index = select_host_mesh(&target_unit.meshes, target_lod, slots.target)?;
    let target = &target_unit.streams[target_unit.meshes[host_index].stream_index];
    for source_lod in source_lods_by_distance(source, target_lod) {
        let sections = visible_source_sections(source, source_lod, slots.source);
        if sections_are_layout_compatible(target_unit.version, source, target, &sections) {
            return estimate_stream_growth(target, source, &sections).map(Some);
        }
    }
    Ok(None)
}

fn sections_are_layout_compatible(
    target_version: u32,
    source: &ParsedUnit,
    target: &StreamRecord,
    sections: &[SourceSection],
) -> bool {
    !sections.is_empty()
        && sections.iter().all(|section| {
            let mesh = &source.meshes[section.mesh_index];
            let stream = &source.streams[mesh.stream_index];
            validate_layout_compatibility(target, stream, target_version, source.version).is_ok()
        })
}

fn estimate_stream_growth(
    target: &StreamRecord,
    source: &ParsedUnit,
    sections: &[SourceSection],
) -> crate::Result<usize> {
    let (added_vertices, added_indices, source_uses_u32) =
        sum_source_section_counts(source, sections)?;
    let vertex_count = checked_count_sum(target.vertex_count, added_vertices, "vertex")?;
    let index_count = checked_count_sum(target.index_count, added_indices, "index")?;
    let vertex_size = checked_buffer_size(vertex_count, target.vertex_stride, "vertex")?;
    let index_stride = if target.index_type == 1 || source_uses_u32 {
        4
    } else {
        2
    };
    let index_size = checked_buffer_size(index_count, index_stride, "index")?;
    vertex_size
        .checked_add(index_size)
        .and_then(|size| size.checked_add(30))
        .ok_or_else(|| eyre::eyre!("merged GPU buffer size overflow"))
}

fn sum_source_section_counts(
    source: &ParsedUnit,
    sections: &[SourceSection],
) -> crate::Result<(u32, u32, bool)> {
    sections.iter().try_fold(
        (0u32, 0u32, false),
        |(vertices, indices, uses_u32), section| {
            let mesh = &source.meshes[section.mesh_index];
            let stream = &source.streams[mesh.stream_index];
            let section = mesh.sections[section.section_index];
            Ok((
                checked_count_sum(vertices, section.vertex_count, "vertex")?,
                checked_count_sum(indices, section.index_count, "index")?,
                uses_u32 || stream.index_type == 1,
            ))
        },
    )
}

fn checked_count_sum(current: u32, added: u32, name: &str) -> crate::Result<u32> {
    current
        .checked_add(added)
        .ok_or_else(|| eyre::eyre!("{name} count overflow"))
}

fn checked_buffer_size(count: u32, stride: u32, name: &str) -> crate::Result<usize> {
    (count as usize)
        .checked_mul(stride as usize)
        .ok_or_else(|| eyre::eyre!("{name} buffer size overflow"))
}
