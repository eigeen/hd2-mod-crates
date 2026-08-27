use byteorder::{ByteOrder, LittleEndian as LE};

pub(super) const LEGACY_VERSION: u32 = 10_800_437;
pub(super) const CURRENT_VERSION: u32 = 10_800_438;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct Semantic {
    pub kind: u32,
    pub index: u32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Component {
    pub semantic: Semantic,
    pub format: u32,
    pub offset: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Value {
    Float([f32; 4]),
    Uint([u32; 4]),
}

pub(super) fn uint_values(value: Value) -> [u32; 4] {
    value_as_uints(value)
}

pub(super) fn float_values(value: Value) -> [f32; 4] {
    value_as_floats(value)
}

pub(super) fn component_size(version: u32, format: u32) -> Option<usize> {
    let normalized = normalize_format(version, format)?;
    match normalized {
        0 => Some(4),
        1 => Some(8),
        2 => Some(12),
        3 => Some(16),
        4 | 28 | 29 | 30 | 33 => Some(4),
        24 => Some(16),
        35 => Some(8),
        _ => None,
    }
}

pub(super) fn validate_component(version: u32, component: Component) -> crate::Result<()> {
    if component.semantic.kind > 7 {
        eyre::bail!("unsupported vertex semantic {}", component.semantic.kind);
    }
    component_size(version, component.format)
        .ok_or_else(|| eyre::eyre!("unsupported vertex format {}", component.format))?;
    Ok(())
}

pub(super) fn decode(version: u32, format: u32, bytes: &[u8]) -> crate::Result<Value> {
    let format = normalize_format(version, format)
        .ok_or_else(|| eyre::eyre!("unsupported Unit version {version}"))?;
    let value = match format {
        0 => Value::Float([read_f32(bytes, 0)?, 0.0, 0.0, 0.0]),
        1 => Value::Float([read_f32(bytes, 0)?, read_f32(bytes, 4)?, 0.0, 0.0]),
        2 => Value::Float([
            read_f32(bytes, 0)?,
            read_f32(bytes, 4)?,
            read_f32(bytes, 8)?,
            0.0,
        ]),
        3 => Value::Float([
            read_f32(bytes, 0)?,
            read_f32(bytes, 4)?,
            read_f32(bytes, 8)?,
            read_f32(bytes, 12)?,
        ]),
        4 => Value::Float([
            read_u8(bytes, 0)? as f32 / 255.0,
            read_u8(bytes, 1)? as f32 / 255.0,
            read_u8(bytes, 2)? as f32 / 255.0,
            read_u8(bytes, 3)? as f32 / 255.0,
        ]),
        24 => Value::Uint([
            read_u32(bytes, 0)?,
            read_u32(bytes, 4)?,
            read_u32(bytes, 8)?,
            read_u32(bytes, 12)?,
        ]),
        28 => Value::Uint([
            read_u8(bytes, 0)? as u32,
            read_u8(bytes, 1)? as u32,
            read_u8(bytes, 2)? as u32,
            read_u8(bytes, 3)? as u32,
        ]),
        29 => {
            let packed = read_u32(bytes, 0)?;
            Value::Float([
                (packed & 0x3ff) as f32 / 1023.0,
                ((packed >> 10) & 0x3ff) as f32 / 1023.0,
                ((packed >> 20) & 0x3ff) as f32 / 1023.0,
                ((packed >> 30) & 0x3) as f32 / 3.0,
            ])
        }
        30 => Value::Float(decode_octahedral(read_u32(bytes, 0)?)),
        33 => Value::Float([
            half_to_f32(read_u16(bytes, 0)?),
            half_to_f32(read_u16(bytes, 2)?),
            0.0,
            0.0,
        ]),
        35 => Value::Float([
            half_to_f32(read_u16(bytes, 0)?),
            half_to_f32(read_u16(bytes, 2)?),
            half_to_f32(read_u16(bytes, 4)?),
            half_to_f32(read_u16(bytes, 6)?),
        ]),
        _ => eyre::bail!("unsupported vertex format {format}"),
    };
    Ok(value)
}

pub(super) fn encode(
    version: u32,
    format: u32,
    value: Value,
    output: &mut Vec<u8>,
) -> crate::Result<()> {
    let format = normalize_format(version, format)
        .ok_or_else(|| eyre::eyre!("unsupported Unit version {version}"))?;
    let floats = value_as_floats(value);
    let uints = value_as_uints(value);
    match format {
        0..=3 => {
            let count = (format + 1) as usize;
            for value in &floats[..count] {
                output.extend_from_slice(&value.to_le_bytes());
            }
        }
        4 => output.extend(floats.map(float_to_unorm8)),
        24 => output.extend(uints.into_iter().flat_map(u32::to_le_bytes)),
        28 => output.extend(uints.map(|value| value.min(255) as u8)),
        29 => {
            let packed = (float_to_unorm(floats[0], 1023))
                | (float_to_unorm(floats[1], 1023) << 10)
                | (float_to_unorm(floats[2], 1023) << 20)
                | (float_to_unorm(floats[3], 3) << 30);
            output.extend_from_slice(&packed.to_le_bytes());
        }
        30 => output.extend_from_slice(&encode_octahedral(floats).to_le_bytes()),
        33 => {
            output.extend_from_slice(&f32_to_half(floats[0]).to_le_bytes());
            output.extend_from_slice(&f32_to_half(floats[1]).to_le_bytes());
        }
        35 => {
            for value in floats {
                output.extend_from_slice(&f32_to_half(value).to_le_bytes());
            }
        }
        _ => eyre::bail!("unsupported vertex format {format}"),
    }
    Ok(())
}

pub(super) fn default_value(semantic: Semantic) -> Value {
    match semantic.kind {
        1 => Value::Float([0.0, 0.0, 1.0, 0.0]),
        2 => Value::Float([1.0, 0.0, 0.0, 1.0]),
        3 => Value::Float([0.0, 1.0, 0.0, 1.0]),
        5 => Value::Float([1.0, 1.0, 1.0, 1.0]),
        6 => Value::Uint([0; 4]),
        _ => Value::Float([0.0; 4]),
    }
}

pub(super) fn transform_direction(value: Value, matrix: &[f32; 16], normal: bool) -> Value {
    let Value::Float(mut values) = value else {
        return value;
    };
    let vector = if normal {
        normal_matrix_mul(matrix, values)
    } else {
        matrix_vector_mul(matrix, values)
    };
    values[..3].copy_from_slice(&normalized(vector));
    Value::Float(values)
}

pub(super) fn transform_position(value: Value, matrix: &[f32; 16]) -> Value {
    let Value::Float(mut values) = value else {
        return value;
    };
    let x = values[0];
    let y = values[1];
    let z = values[2];
    values[0] = matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12];
    values[1] = matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13];
    values[2] = matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14];
    Value::Float(values)
}

pub(super) fn relative_transform(
    source: &[f32; 16],
    target: &[f32; 16],
) -> crate::Result<[f32; 16]> {
    Ok(multiply(&invert_affine(target)?, source))
}

fn normalize_format(version: u32, format: u32) -> Option<u32> {
    match version {
        CURRENT_VERSION => Some(format),
        LEGACY_VERSION => Some(match format {
            20 => 24,
            24 => 28,
            25 => 29,
            26 => 30,
            29 => 33,
            31 => 35,
            value => value,
        }),
        _ => None,
    }
}

fn value_as_floats(value: Value) -> [f32; 4] {
    match value {
        Value::Float(values) => values,
        Value::Uint(values) => values.map(|value| value as f32),
    }
}

fn value_as_uints(value: Value) -> [u32; 4] {
    match value {
        Value::Uint(values) => values,
        Value::Float(values) => values.map(|value| value.max(0.0).round() as u32),
    }
}

fn read_f32(bytes: &[u8], offset: usize) -> crate::Result<f32> {
    Ok(f32::from_bits(read_u32(bytes, offset)?))
}

fn read_u32(bytes: &[u8], offset: usize) -> crate::Result<u32> {
    require(bytes, offset, 4)?;
    Ok(LE::read_u32(&bytes[offset..]))
}

fn read_u16(bytes: &[u8], offset: usize) -> crate::Result<u16> {
    require(bytes, offset, 2)?;
    Ok(LE::read_u16(&bytes[offset..]))
}

fn read_u8(bytes: &[u8], offset: usize) -> crate::Result<u8> {
    require(bytes, offset, 1)?;
    Ok(bytes[offset])
}

fn require(bytes: &[u8], offset: usize, len: usize) -> crate::Result<()> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| eyre::eyre!("vertex offset overflow"))?;
    if end > bytes.len() {
        eyre::bail!("vertex component is truncated");
    }
    Ok(())
}

fn float_to_unorm8(value: f32) -> u8 {
    float_to_unorm(value, 255) as u8
}

fn float_to_unorm(value: f32, max: u32) -> u32 {
    (value.clamp(0.0, 1.0) * max as f32).round() as u32
}

fn decode_octahedral(packed: u32) -> [f32; 4] {
    let mut x = (packed & 0x3ff) as f32 * (2.0 / 1023.0) - 1.0;
    let mut y = ((packed >> 10) & 0x3ff) as f32 * (2.0 / 1023.0) - 1.0;
    let z = 1.0 - x.abs() - y.abs();
    if z < 0.0 {
        (x, y) = ((1.0 - y.abs()) * x.signum(), (1.0 - x.abs()) * y.signum());
    }
    let n = normalized([x, y, z]);
    [n[0], n[1], n[2], 0.0]
}

fn encode_octahedral(values: [f32; 4]) -> u32 {
    let [mut x, mut y, z] = normalized([values[0], values[1], values[2]]);
    let divisor = x.abs() + y.abs() + z.abs();
    if divisor > f32::EPSILON {
        x /= divisor;
        y /= divisor;
    }
    if z < 0.0 {
        (x, y) = ((1.0 - y.abs()) * x.signum(), (1.0 - x.abs()) * y.signum());
    }
    float_to_unorm((x + 1.0) * 0.5, 1023) | (float_to_unorm((y + 1.0) * 0.5, 1023) << 10)
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = ((bits & 0x8000) as u32) << 16;
    let exponent = ((bits >> 10) & 0x1f) as i32;
    let fraction = (bits & 0x03ff) as u32;
    let converted = match exponent {
        0 if fraction == 0 => sign,
        0 => {
            let mut mantissa = fraction;
            let mut exp = -14;
            while mantissa & 0x400 == 0 {
                mantissa <<= 1;
                exp -= 1;
            }
            sign | (((exp + 127) as u32) << 23) | ((mantissa & 0x3ff) << 13)
        }
        31 => sign | 0x7f80_0000 | (fraction << 13),
        _ => sign | (((exponent - 15 + 127) as u32) << 23) | (fraction << 13),
    };
    f32::from_bits(converted)
}

fn f32_to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x7f_ffff;
    if exponent <= 0 {
        if exponent < -10 {
            return sign;
        }
        let shifted = (mantissa | 0x80_0000) >> (1 - exponent + 13);
        return sign | shifted as u16;
    }
    if exponent >= 31 {
        return sign | 0x7c00;
    }
    sign | ((exponent as u16) << 10) | ((mantissa >> 13) as u16)
}

fn multiply(left: &[f32; 16], right: &[f32; 16]) -> [f32; 16] {
    let mut output = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            output[column * 4 + row] = (0..4)
                .map(|index| left[index * 4 + row] * right[column * 4 + index])
                .sum();
        }
    }
    output
}

fn invert_affine(matrix: &[f32; 16]) -> crate::Result<[f32; 16]> {
    let a = matrix[0];
    let b = matrix[4];
    let c = matrix[8];
    let d = matrix[1];
    let e = matrix[5];
    let f = matrix[9];
    let g = matrix[2];
    let h = matrix[6];
    let i = matrix[10];
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() <= f32::EPSILON {
        eyre::bail!("mesh transform is not invertible");
    }
    let inv_det = det.recip();
    let mut output = [0.0; 16];
    output[0] = (e * i - f * h) * inv_det;
    output[4] = (c * h - b * i) * inv_det;
    output[8] = (b * f - c * e) * inv_det;
    output[1] = (f * g - d * i) * inv_det;
    output[5] = (a * i - c * g) * inv_det;
    output[9] = (c * d - a * f) * inv_det;
    output[2] = (d * h - e * g) * inv_det;
    output[6] = (b * g - a * h) * inv_det;
    output[10] = (a * e - b * d) * inv_det;
    output[15] = 1.0;
    let translation = [matrix[12], matrix[13], matrix[14]];
    output[12] =
        -(output[0] * translation[0] + output[4] * translation[1] + output[8] * translation[2]);
    output[13] =
        -(output[1] * translation[0] + output[5] * translation[1] + output[9] * translation[2]);
    output[14] =
        -(output[2] * translation[0] + output[6] * translation[1] + output[10] * translation[2]);
    Ok(output)
}

fn matrix_vector_mul(matrix: &[f32; 16], values: [f32; 4]) -> [f32; 3] {
    [
        matrix[0] * values[0] + matrix[4] * values[1] + matrix[8] * values[2],
        matrix[1] * values[0] + matrix[5] * values[1] + matrix[9] * values[2],
        matrix[2] * values[0] + matrix[6] * values[1] + matrix[10] * values[2],
    ]
}

fn normal_matrix_mul(matrix: &[f32; 16], values: [f32; 4]) -> [f32; 3] {
    let inverse = invert_affine(matrix).unwrap_or_else(|_| identity());
    [
        inverse[0] * values[0] + inverse[1] * values[1] + inverse[2] * values[2],
        inverse[4] * values[0] + inverse[5] * values[1] + inverse[6] * values[2],
        inverse[8] * values[0] + inverse[9] * values[1] + inverse[10] * values[2],
    ]
}

fn normalized(values: [f32; 3]) -> [f32; 3] {
    let length = (values[0] * values[0] + values[1] * values[1] + values[2] * values[2]).sqrt();
    if length <= f32::EPSILON {
        return [0.0, 0.0, 1.0];
    }
    [values[0] / length, values[1] / length, values[2] / length]
}

fn identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_round_trip_keeps_common_values() {
        for value in [-2.0, -0.5, 0.0, 0.5, 1.0, 12.0] {
            assert!((half_to_f32(f32_to_half(value)) - value).abs() < 0.01);
        }
    }

    #[test]
    fn relative_transform_moves_between_local_spaces() {
        let mut source = identity();
        source[12] = 4.0;
        let mut target = identity();
        target[12] = 1.0;
        let relative = relative_transform(&source, &target).unwrap();
        let Value::Float(position) =
            transform_position(Value::Float([1.0, 0.0, 0.0, 0.0]), &relative)
        else {
            panic!()
        };
        assert!((position[0] - 4.0).abs() < 0.001);
    }
}
