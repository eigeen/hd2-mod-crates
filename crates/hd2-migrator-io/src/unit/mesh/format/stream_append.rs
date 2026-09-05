use super::transcode::meaningful_vertex_data;
use super::*;

#[derive(Debug, Clone, Copy)]
struct AppendedSection {
    vertex_offset: u32,
    vertex_count: u32,
    index_offset: u32,
    index_count: u32,
}

pub(super) struct PendingSection {
    pub(super) material_index: u32,
    pub(super) group_index: u32,
    pub(super) vertex_count: u32,
    pub(super) index_count: u32,
    pub(super) vertices: Vec<u8>,
    pub(super) indices: Vec<u32>,
}

pub(super) struct CompletedSection {
    material_index: u32,
    group_index: u32,
    appended: AppendedSection,
}

struct StreamAppendState {
    vertices: Vec<u8>,
    indices: Vec<u32>,
    vertex_count: u32,
    index_count: u32,
    completed: Vec<CompletedSection>,
}

struct AppendedBuffers {
    vertex_offset: usize,
    vertex_size: usize,
    index_offset: usize,
    index_size: usize,
}

impl StreamAppendState {
    fn from_target(gpu: &[u8], target: &StreamRecord) -> crate::Result<Self> {
        Ok(Self {
            vertices: copy_vertex_bytes(meaningful_vertex_data(gpu, target)?)?,
            indices: decode_indices(
                gpu,
                target.index_offset,
                target.index_count,
                target.index_type,
            )?,
            vertex_count: target.vertex_count,
            index_count: target.index_count,
            completed: Vec::new(),
        })
    }

    fn push_section(&mut self, section: PendingSection) -> crate::Result<()> {
        let appended = AppendedSection {
            vertex_offset: self.vertex_count,
            vertex_count: section.vertex_count,
            index_offset: self.index_count,
            index_count: section.index_count,
        };
        self.vertex_count = checked_count_sum(self.vertex_count, section.vertex_count, "vertex")?;
        self.index_count = checked_count_sum(self.index_count, section.index_count, "index")?;
        self.vertices.extend(section.vertices);
        self.indices.extend(section.indices);
        self.completed.push(CompletedSection {
            material_index: section.material_index,
            group_index: section.group_index,
            appended,
        });
        Ok(())
    }

    fn reserve_sections(&mut self, sections: &[PendingSection]) -> crate::Result<()> {
        let vertex_bytes = checked_payload_len(sections, |section| section.vertices.len())?;
        let index_values = checked_payload_len(sections, |section| section.indices.len())?;
        try_reserve_payload(&mut self.vertices, vertex_bytes, "merged vertex buffer")?;
        try_reserve_payload(&mut self.indices, index_values, "merged index buffer")?;
        try_reserve_payload(&mut self.completed, sections.len(), "merged mesh sections")
    }
}

pub(super) fn append_stream_sections(
    gpu: &mut Vec<u8>,
    target: &mut StreamRecord,
    sections: Vec<PendingSection>,
) -> crate::Result<Vec<CompletedSection>> {
    let mut state = StreamAppendState::from_target(gpu, target)?;
    state.reserve_sections(&sections)?;
    for section in sections {
        state.push_section(section)?;
    }
    let index_type = select_index_type(target.index_type, &state.indices);
    let buffers = append_stream_buffers(gpu, &state.vertices, &state.indices, index_type)?;
    update_target_stream(target, &state, buffers, index_type)?;
    Ok(state.completed)
}

pub(super) fn validate_source_indices(indices: &[u32], vertex_count: u32) -> crate::Result<()> {
    if indices.iter().any(|index| *index >= vertex_count) {
        eyre::bail!("helmet section index exceeds its vertex range");
    }
    Ok(())
}

pub(super) fn completed_mesh_section(section: CompletedSection) -> MeshSection {
    MeshSection {
        material_index: section.material_index,
        vertex_offset: section.appended.vertex_offset,
        vertex_count: section.appended.vertex_count,
        index_offset: section.appended.index_offset,
        index_count: section.appended.index_count,
        group_index: section.group_index,
    }
}

fn update_target_stream(
    target: &mut StreamRecord,
    state: &StreamAppendState,
    buffers: AppendedBuffers,
    index_type: u32,
) -> crate::Result<()> {
    target.vertex_count = state.vertex_count;
    target.index_count = state.index_count;
    target.vertex_offset = u32::try_from(buffers.vertex_offset)?;
    target.vertex_size = u32::try_from(buffers.vertex_size)?;
    target.index_offset = u32::try_from(buffers.index_offset)?;
    target.index_size = u32::try_from(buffers.index_size)?;
    target.index_type = index_type;
    Ok(())
}

fn append_stream_buffers(
    output: &mut Vec<u8>,
    vertices: &[u8],
    indices: &[u32],
    index_type: u32,
) -> crate::Result<AppendedBuffers> {
    let index_size = checked_index_size(indices, index_type)?;
    let vertex_offset = checked_align_up(output.len(), 16)?;
    let vertex_end = vertex_offset
        .checked_add(vertices.len())
        .ok_or_else(|| eyre::eyre!("merged GPU buffer size overflow"))?;
    let index_offset = checked_align_up(vertex_end, 16)?;
    reserve_final_buffer(output, index_offset, index_size)?;
    output.resize(vertex_offset, 0);
    output.extend_from_slice(vertices);
    output.resize(index_offset, 0);
    append_index_values(output, indices, index_type);
    Ok(AppendedBuffers {
        vertex_offset,
        vertex_size: vertices.len(),
        index_offset,
        index_size,
    })
}

fn copy_vertex_bytes(bytes: &[u8]) -> crate::Result<Vec<u8>> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| eyre::eyre!("not enough memory for merged vertex buffer"))?;
    output.extend_from_slice(bytes);
    Ok(output)
}

fn checked_align_up(value: usize, alignment: usize) -> crate::Result<usize> {
    value
        .checked_add(alignment - 1)
        .map(|aligned| aligned & !(alignment - 1))
        .ok_or_else(|| eyre::eyre!("merged GPU buffer size overflow"))
}

fn checked_index_size(indices: &[u32], index_type: u32) -> crate::Result<usize> {
    indices
        .len()
        .checked_mul(index_stride(index_type)?)
        .ok_or_else(|| eyre::eyre!("index buffer size overflow"))
}

fn reserve_final_buffer(
    output: &mut Vec<u8>,
    index_offset: usize,
    index_size: usize,
) -> crate::Result<()> {
    let final_len = index_offset
        .checked_add(index_size)
        .ok_or_else(|| eyre::eyre!("merged GPU buffer size overflow"))?;
    output
        .try_reserve_exact(final_len - output.len())
        .map_err(|_| eyre::eyre!("not enough memory for merged GPU buffer"))
}

fn append_index_values(output: &mut Vec<u8>, indices: &[u32], index_type: u32) {
    if index_type == 1 {
        output.extend(indices.iter().flat_map(|value| value.to_le_bytes()));
    } else {
        output.extend(
            indices
                .iter()
                .flat_map(|value| (*value as u16).to_le_bytes()),
        );
    }
}

fn checked_payload_len<T>(items: &[T], len: impl Fn(&T) -> usize) -> crate::Result<usize> {
    items.iter().try_fold(0usize, |total, item| {
        total
            .checked_add(len(item))
            .ok_or_else(|| eyre::eyre!("merged buffer size overflow"))
    })
}

fn try_reserve_payload<T>(values: &mut Vec<T>, added: usize, label: &str) -> crate::Result<()> {
    values
        .try_reserve_exact(added)
        .map_err(|_| eyre::eyre!("not enough memory for {label}"))
}

fn checked_count_sum(current: u32, added: u32, name: &str) -> crate::Result<u32> {
    current
        .checked_add(added)
        .ok_or_else(|| eyre::eyre!("{name} count overflow"))
}

fn select_index_type(current: u32, indices: &[u32]) -> u32 {
    if current == 1 || indices.iter().any(|index| *index > u16::MAX as u32) {
        1
    } else {
        0
    }
}
