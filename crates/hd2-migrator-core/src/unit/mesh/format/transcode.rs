use super::super::vertex::{Value, default_value, encode, transform_direction, transform_position};
use super::*;

pub(super) struct VertexTranscodeRequest<'a> {
    pub(super) source_gpu: &'a [u8],
    pub(super) source: &'a StreamRecord,
    pub(super) source_version: u32,
    pub(super) section: MeshSection,
    pub(super) target: &'a StreamRecord,
    pub(super) target_version: u32,
    pub(super) transform: &'a [f32; 16],
}

pub(super) fn validate_layout_compatibility(
    target: &StreamRecord,
    source: &StreamRecord,
    target_version: u32,
    source_version: u32,
) -> crate::Result<()> {
    for component in &target.components {
        validate_component(target_version, *component)?;
    }
    for component in &source.components {
        validate_source_component(target, *component, source_version)?;
    }
    Ok(())
}

fn validate_source_component(
    target: &StreamRecord,
    component: Component,
    source_version: u32,
) -> crate::Result<()> {
    validate_component(source_version, component)?;
    if semantic_is_required_in_target(component.semantic)
        && !target
            .components
            .iter()
            .any(|target| target.semantic == component.semantic)
    {
        eyre::bail!(
            "body stream is missing required helmet vertex semantic {}/{}",
            component.semantic.kind,
            component.semantic.index
        );
    }
    Ok(())
}

fn semantic_is_required_in_target(semantic: Semantic) -> bool {
    matches!(semantic.kind, 0 | 6 | 7)
}

/// Return only declared vertices, excluding alignment padding at the buffer tail.
pub(super) fn meaningful_vertex_data<'a>(
    gpu: &'a [u8],
    stream: &StreamRecord,
) -> crate::Result<&'a [u8]> {
    let size = stream
        .vertex_count
        .checked_mul(stream.vertex_stride)
        .ok_or_else(|| eyre::eyre!("body vertex buffer size overflow"))?;
    if size > stream.vertex_size {
        eyre::bail!("body vertex count exceeds its buffer size");
    }
    checked_slice(gpu, stream.vertex_offset, size, "body vertex buffer")
}

pub(super) fn transcode_vertices(request: VertexTranscodeRequest<'_>) -> crate::Result<Vec<u8>> {
    let capacity = (request.section.vertex_count as usize)
        .checked_mul(request.target.vertex_stride as usize)
        .ok_or_else(|| eyre::eyre!("transcoded vertex buffer size overflow"))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| eyre::eyre!("not enough memory for transcoded vertex buffer"))?;
    for local_index in 0..request.section.vertex_count {
        transcode_vertex(&request, local_index, &mut output)?;
    }
    Ok(output)
}

fn transcode_vertex(
    request: &VertexTranscodeRequest<'_>,
    local_index: u32,
    output: &mut Vec<u8>,
) -> crate::Result<()> {
    let source_index = request.section.vertex_offset + local_index;
    let source_start = request.source.vertex_offset as usize
        + source_index as usize * request.source.vertex_stride as usize;
    let before = output.len();
    for target_component in &request.target.components {
        transcode_component(request, source_start, *target_component, output)?;
    }
    output.resize(before + request.target.vertex_stride as usize, 0);
    Ok(())
}

fn transcode_component(
    request: &VertexTranscodeRequest<'_>,
    source_start: usize,
    target: Component,
    output: &mut Vec<u8>,
) -> crate::Result<()> {
    let value = source_component_value(request, source_start, target)?;
    let transformed = transform_component(value, target.semantic, request.transform);
    encode(request.target_version, target.format, transformed, output)
}

fn source_component_value(
    request: &VertexTranscodeRequest<'_>,
    source_start: usize,
    target: Component,
) -> crate::Result<Value> {
    let Some(source) = request
        .source
        .components
        .iter()
        .find(|component| component.semantic == target.semantic)
    else {
        return Ok(default_value(target.semantic));
    };
    let size = component_size(request.source_version, source.format).unwrap_or(0);
    let bytes = checked_slice_usize(
        request.source_gpu,
        source_start + source.offset,
        size,
        "helmet vertex component",
    )?;
    decode(request.source_version, source.format, bytes)
}

fn transform_component(value: Value, semantic: Semantic, transform: &[f32; 16]) -> Value {
    match semantic.kind {
        0 => transform_position(value, transform),
        1 => transform_direction(value, transform, true),
        2 | 3 => transform_direction(value, transform, false),
        _ => value,
    }
}
