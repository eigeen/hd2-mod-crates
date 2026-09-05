use super::{Matrix4, Point3};
use crate::archive::TocEntry;

#[derive(Debug, Clone, Copy)]
struct StreamLayout {
    num_vertices: u32,
    vertex_stride: u32,
    vertex_offset: u32,
    position_offset: i32,
    position_format: i32,
}

#[derive(Debug, Clone, Copy)]
struct MeshSection {
    vertex_offset: u32,
    num_vertices: u32,
}

#[derive(Debug, Clone)]
struct MeshLayout {
    transform_index: u32,
    lod_index: i32,
    stream_index: u32,
    sections: Vec<MeshSection>,
}

pub fn parse_unit_points(entry: &TocEntry) -> Vec<Point3> {
    let stream_layouts = read_stream_layouts(&entry.toc_data);
    let mesh_layouts = select_primary_meshes(read_mesh_layouts(&entry.toc_data));
    let transforms = read_transform_matrices(&entry.toc_data);
    let mut points = Vec::new();
    for mesh in &mesh_layouts {
        points.extend(mesh_points(
            &entry.gpu_data,
            &stream_layouts,
            &transforms,
            mesh,
        ));
    }
    points
}

// ---------- vertex parsing ----------------------------------------------

fn read_stream_layouts(toc_data: &[u8]) -> Vec<StreamLayout> {
    let stream_off = read_u32(toc_data, 0x5C) as usize;
    if stream_off == 0 || stream_off + 4 > toc_data.len() {
        return Vec::new();
    }
    let num_streams = read_u32(toc_data, stream_off) as usize;
    let bases = offset_table_bases(toc_data, stream_off, num_streams);
    bases
        .iter()
        .map(|&b| read_stream_layout(toc_data, b))
        .collect()
}

fn read_stream_layout(toc_data: &[u8], base: usize) -> StreamLayout {
    let num_components = read_u64(toc_data, base + 328);
    let vertex_count = read_u32(toc_data, base + 352);
    let vertex_stride = read_u32(toc_data, base + 356);
    let vertex_offset = read_u32(toc_data, base + 416);
    let version = read_u32(toc_data, 0x2C);
    match position_component(toc_data, base + 8, num_components as usize, version) {
        Some((cursor, format)) => StreamLayout {
            num_vertices: vertex_count,
            vertex_stride,
            vertex_offset,
            position_offset: cursor as i32,
            position_format: format as i32,
        },
        None => StreamLayout {
            num_vertices: vertex_count,
            vertex_stride,
            vertex_offset,
            position_offset: -1,
            position_format: -1,
        },
    }
}

fn position_component(
    toc_data: &[u8],
    offset: usize,
    num_components: usize,
    version: u32,
) -> Option<(u32, u32)> {
    let mut cursor = 0u32;
    for index in 0..num_components {
        let at = offset + 20 * index;
        if at + 20 > toc_data.len() {
            return None;
        }
        let component_type = read_u32(toc_data, at);
        let component_format = read_u32(toc_data, at + 4);
        if component_type == 0 {
            return Some((cursor, component_format));
        }
        let component_size = component_size(version, component_format);
        if component_size == 0 {
            return None;
        }
        cursor += component_size as u32;
    }
    None
}

fn read_mesh_layouts(toc_data: &[u8]) -> Vec<MeshLayout> {
    let mesh_off = read_u32(toc_data, 0x64) as usize;
    if mesh_off == 0 || mesh_off + 4 > toc_data.len() {
        return Vec::new();
    }
    let num_meshes = read_u32(toc_data, mesh_off) as usize;
    let bases = offset_table_bases(toc_data, mesh_off, num_meshes);
    bases
        .iter()
        .map(|&b| read_mesh_layout(toc_data, b))
        .collect()
}

fn read_mesh_layout(toc_data: &[u8], base: usize) -> MeshLayout {
    let transform_index = read_u32(toc_data, base + 48);
    let lod_index = read_i32(toc_data, base + 56);
    let stream_index = read_u32(toc_data, base + 60);
    let num_sections = read_u32(toc_data, base + 120) as usize;
    let section_offset = read_u32(toc_data, base + 124) as usize;
    let sections = read_sections(toc_data, base + section_offset, num_sections);
    MeshLayout {
        transform_index,
        lod_index,
        stream_index,
        sections,
    }
}

fn read_sections(toc_data: &[u8], start: usize, count: usize) -> Vec<MeshSection> {
    let mut out = Vec::new();
    for index in 0..count {
        let at = start + 24 * index;
        if at + 24 > toc_data.len() {
            continue;
        }
        out.push(MeshSection {
            vertex_offset: read_u32(toc_data, at + 4),
            num_vertices: read_u32(toc_data, at + 8),
        });
    }
    out
}

fn select_primary_meshes(meshes: Vec<MeshLayout>) -> Vec<MeshLayout> {
    let lod_zero: Vec<MeshLayout> = meshes
        .iter()
        .filter(|m| m.lod_index == 0)
        .cloned()
        .collect();
    if !lod_zero.is_empty() {
        return lod_zero;
    }
    let non_negative: Vec<MeshLayout> = meshes
        .iter()
        .filter(|m| m.lod_index >= 0)
        .cloned()
        .collect();
    if !non_negative.is_empty() {
        let best_lod = non_negative.iter().map(|m| m.lod_index).min().unwrap_or(0);
        return non_negative
            .into_iter()
            .filter(|m| m.lod_index == best_lod)
            .collect();
    }
    meshes
}

fn read_transform_matrices(toc_data: &[u8]) -> Vec<Matrix4> {
    let transform_off = read_u32(toc_data, 0x34) as usize;
    if transform_off == 0 || transform_off + 16 > toc_data.len() {
        return Vec::new();
    }
    let count = read_u32(toc_data, transform_off) as usize;
    let matrices_at = transform_off + 16 + 48 * count;
    (0..count)
        .map(|index| read_matrix(toc_data, matrices_at + 64 * index))
        .collect()
}

fn read_matrix(toc_data: &[u8], offset: usize) -> Matrix4 {
    if offset + 64 > toc_data.len() {
        return identity_matrix();
    }
    let mut m = [0.0f64; 16];
    for (i, value) in m.iter_mut().enumerate() {
        let raw = read_u32(toc_data, offset + 4 * i);
        *value = f32::from_bits(raw) as f64;
    }
    m
}

fn mesh_points(
    gpu_data: &[u8],
    stream_layouts: &[StreamLayout],
    transforms: &[Matrix4],
    mesh: &MeshLayout,
) -> Vec<Point3> {
    if mesh.stream_index as usize >= stream_layouts.len() {
        return Vec::new();
    }
    let stream = stream_layouts[mesh.stream_index as usize];
    if stream.position_offset < 0 || stream.vertex_stride == 0 {
        return Vec::new();
    }
    let matrix = matrix_for_mesh(transforms, mesh.transform_index as usize);
    let mut points = Vec::new();
    for section in &mesh.sections {
        points.extend(section_points(gpu_data, &stream, &matrix, section));
    }
    points
}

fn section_points(
    gpu_data: &[u8],
    stream: &StreamLayout,
    matrix: &Matrix4,
    section: &MeshSection,
) -> Vec<Point3> {
    let mut points = Vec::new();
    let end = section
        .vertex_offset
        .saturating_add(section.num_vertices)
        .min(stream.num_vertices);
    for vertex_index in section.vertex_offset..end {
        if let Some(point) = read_vertex_position(gpu_data, stream, vertex_index) {
            points.push(transform_point(point, matrix));
        }
    }
    points
}

fn read_vertex_position(
    gpu_data: &[u8],
    stream: &StreamLayout,
    vertex_index: u32,
) -> Option<Point3> {
    let offset = stream.vertex_offset as usize
        + vertex_index as usize * stream.vertex_stride as usize
        + stream.position_offset as usize;
    let needed = component_size(0, stream.position_format as u32);
    if offset.checked_add(needed)? > gpu_data.len() {
        return None;
    }
    decode_position(gpu_data, offset, stream.position_format)
}

fn decode_position(gpu_data: &[u8], offset: usize, position_format: i32) -> Option<Point3> {
    match position_format {
        0 => Some((f32::from_bits(read_u32(gpu_data, offset)) as f64, 0.0, 0.0)),
        1 => {
            let x = f32::from_bits(read_u32(gpu_data, offset)) as f64;
            let y = f32::from_bits(read_u32(gpu_data, offset + 4)) as f64;
            Some((x, y, 0.0))
        }
        2 => {
            let x = f32::from_bits(read_u32(gpu_data, offset)) as f64;
            let y = f32::from_bits(read_u32(gpu_data, offset + 4)) as f64;
            let z = f32::from_bits(read_u32(gpu_data, offset + 8)) as f64;
            Some((x, y, z))
        }
        3 => {
            let x = f32::from_bits(read_u32(gpu_data, offset)) as f64;
            let y = f32::from_bits(read_u32(gpu_data, offset + 4)) as f64;
            let z = f32::from_bits(read_u32(gpu_data, offset + 8)) as f64;
            Some((x, y, z))
        }
        33 => {
            let x = half_f16(read_u16(gpu_data, offset)) as f64;
            let y = half_f16(read_u16(gpu_data, offset + 2)) as f64;
            Some((x, y, 0.0))
        }
        35 => {
            let x = half_f16(read_u16(gpu_data, offset)) as f64;
            let y = half_f16(read_u16(gpu_data, offset + 2)) as f64;
            let z = half_f16(read_u16(gpu_data, offset + 4)) as f64;
            Some((x, y, z))
        }
        _ => None,
    }
}

fn component_size(version: u32, component_format: u32) -> usize {
    if version == 10_800_437 {
        match component_format {
            0 => 4,
            1 => 8,
            2 => 12,
            3 => 16,
            4 => 4,
            20 => 16,
            24 | 25 | 26 | 29 => 4,
            31 => 8,
            _ => 0,
        }
    } else {
        match component_format {
            0 => 4,
            1 => 8,
            2 => 12,
            3 => 16,
            4 => 4,
            24 => 16,
            28..=30 => 4,
            33 => 4,
            35 => 8,
            _ => 0,
        }
    }
}

fn matrix_for_mesh(transforms: &[Matrix4], transform_index: usize) -> Matrix4 {
    if transform_index < transforms.len() {
        transforms[transform_index]
    } else {
        identity_matrix()
    }
}

fn transform_point(point: Point3, m: &Matrix4) -> Point3 {
    let (x, y, z) = point;
    (
        m[0] * x + m[4] * y + m[8] * z + m[12],
        m[1] * x + m[5] * y + m[9] * z + m[13],
        m[2] * x + m[6] * y + m[10] * z + m[14],
    )
}

fn identity_matrix() -> Matrix4 {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn offset_table_bases(toc_data: &[u8], table_off: usize, count: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let offsets_at = table_off + 4;
    for index in 0..count {
        let off_at = offsets_at + 4 * index;
        if off_at + 4 > toc_data.len() {
            continue;
        }
        let relative = read_u32(toc_data, off_at) as usize;
        let base = table_off + relative;
        if base < toc_data.len() {
            out.push(base);
        }
    }
    out
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    if offset + 4 > data.len() {
        return 0;
    }
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn read_i32(data: &[u8], offset: usize) -> i32 {
    if offset + 4 > data.len() {
        return 0;
    }
    i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn read_u64(data: &[u8], offset: usize) -> u64 {
    if offset + 8 > data.len() {
        return 0;
    }
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

fn read_u16(data: &[u8], offset: usize) -> u16 {
    if offset + 2 > data.len() {
        return 0;
    }
    u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
}

/// IEEE 754 half-precision (binary16) → f32. No `half` crate dep.
fn half_f16(bits: u16) -> f32 {
    let sign = (bits >> 15) & 0x1;
    let exp = (bits >> 10) & 0x1F;
    let frac = (bits & 0x3FF) as u32;
    let f32_bits = if exp == 0 {
        if frac == 0 {
            (sign as u32) << 31
        } else {
            // subnormal
            let mut e: i32 = -14;
            let mut m = frac;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x3FF;
            ((sign as u32) << 31) | (((e + 127) as u32) << 23) | (m << 13)
        }
    } else if exp == 0x1F {
        ((sign as u32) << 31) | (0xFF << 23) | (frac << 13)
    } else {
        ((sign as u32) << 31) | (((exp as i32 - 15 + 127) as u32) << 23) | (frac << 13)
    };
    f32::from_bits(f32_bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_f16_basic_values() {
        assert!((half_f16(0x3C00) - 1.0).abs() < 1e-6);
        assert!((half_f16(0xC000) + 2.0).abs() < 1e-6);
        assert_eq!(half_f16(0x0000), 0.0);
    }
}
