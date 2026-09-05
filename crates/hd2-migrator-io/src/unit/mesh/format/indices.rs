use super::*;

pub(super) fn decode_index_range(
    gpu: &[u8],
    stream: &StreamRecord,
    section: MeshSection,
) -> crate::Result<Vec<u32>> {
    let stride = index_stride(stream.index_type)?;
    let relative = checked_index_byte_len(section.index_offset, stride)?;
    let start = (stream.index_offset as usize)
        .checked_add(relative)
        .ok_or_else(|| eyre::eyre!("index buffer offset overflow"))?;
    let bytes = checked_slice_usize(
        gpu,
        start,
        checked_index_byte_len(section.index_count, stride)?,
        "helmet section indices",
    )?;
    decode_index_bytes(bytes, stream.index_type)
}

pub(super) fn decode_indices(
    gpu: &[u8],
    offset: u32,
    count: u32,
    index_type: u32,
) -> crate::Result<Vec<u32>> {
    let stride = index_stride(index_type)?;
    let bytes = checked_slice_usize(
        gpu,
        offset as usize,
        checked_index_byte_len(count, stride)?,
        "body index buffer",
    )?;
    decode_index_bytes(bytes, index_type)
}

fn checked_index_byte_len(count: u32, stride: usize) -> crate::Result<usize> {
    (count as usize)
        .checked_mul(stride)
        .ok_or_else(|| eyre::eyre!("index buffer size overflow"))
}

pub(super) fn decode_index_bytes(bytes: &[u8], index_type: u32) -> crate::Result<Vec<u32>> {
    let stride = index_stride(index_type)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len() / stride)
        .map_err(|_| eyre::eyre!("not enough memory to decode index buffer"))?;
    match index_type {
        0 => output.extend(
            bytes
                .chunks_exact(2)
                .map(|chunk| LE::read_u16(chunk) as u32),
        ),
        1 => output.extend(bytes.chunks_exact(4).map(LE::read_u32)),
        _ => unreachable!("index type was validated above"),
    }
    Ok(output)
}

#[cfg(test)]
pub(super) fn encode_indices(indices: &[u32], index_type: u32) -> Vec<u8> {
    if index_type == 1 {
        return indices
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
    }
    indices
        .iter()
        .flat_map(|value| (*value as u16).to_le_bytes())
        .collect()
}

pub(super) fn index_stride(index_type: u32) -> crate::Result<usize> {
    match index_type {
        0 => Ok(2),
        1 => Ok(4),
        _ => eyre::bail!("unsupported index type {index_type}"),
    }
}
