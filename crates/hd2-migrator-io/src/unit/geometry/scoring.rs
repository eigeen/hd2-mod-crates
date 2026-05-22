use super::{Point3, UnitGeometrySignature};

pub fn score_signatures(source: &UnitGeometrySignature, target: &UnitGeometrySignature) -> f64 {
    let scale = source.diagonal.max(target.diagonal).max(1e-6);
    let cloud_score = symmetric_cloud_distance(source, target) / scale;
    let quantile_score = tuple_distance(&source.axis_quantiles, &target.axis_quantiles) / scale;
    let radial_score = tuple_distance(&source.radial_quantiles, &target.radial_quantiles) / scale;
    let bbox_score = bbox_score(source, target, scale);
    let count_score = ((source.vertex_count as f64 + 1.0) / (target.vertex_count as f64 + 1.0))
        .ln()
        .abs();
    0.55 * cloud_score
        + 0.20 * quantile_score
        + 0.10 * radial_score
        + 0.10 * bbox_score
        + 0.05 * count_score
}

pub(super) fn bounding_box_stats(points: &[Point3]) -> (Point3, Point3, f64) {
    let mut mins = [f64::INFINITY; 3];
    let mut maxs = [f64::NEG_INFINITY; 3];
    for &(x, y, z) in points {
        let arr = [x, y, z];
        for i in 0..3 {
            if arr[i] < mins[i] {
                mins[i] = arr[i];
            }
            if arr[i] > maxs[i] {
                maxs[i] = arr[i];
            }
        }
    }
    let center = (
        (mins[0] + maxs[0]) / 2.0,
        (mins[1] + maxs[1]) / 2.0,
        (mins[2] + maxs[2]) / 2.0,
    );
    let extents = (maxs[0] - mins[0], maxs[1] - mins[1], maxs[2] - mins[2]);
    let diag = (extents.0 * extents.0 + extents.1 * extents.1 + extents.2 * extents.2).sqrt();
    (center, extents, diag)
}

pub(super) fn axis_quantiles(points: &[Point3], quantiles: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(quantiles.len() * 3);
    for axis in 0..3 {
        let mut ordered: Vec<f64> = points.iter().map(|p| axis_value(*p, axis)).collect();
        ordered.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        out.extend(quantile_values(&ordered, quantiles));
    }
    out
}

pub(super) fn radial_quantiles(points: &[Point3], center: Point3, quantiles: &[f64]) -> Vec<f64> {
    let mut ordered: Vec<f64> = points.iter().map(|p| vector_distance(*p, center)).collect();
    ordered.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    quantile_values(&ordered, quantiles)
}

fn quantile_values(ordered: &[f64], quantiles: &[f64]) -> Vec<f64> {
    if ordered.is_empty() {
        return vec![0.0; quantiles.len()];
    }
    let last = (ordered.len() - 1) as f64;
    quantiles
        .iter()
        .map(|q| ordered[python_round(last * q) as usize])
        .collect()
}

pub(crate) fn downsample_points(points: &[Point3], sample_count: usize) -> Vec<Point3> {
    if points.len() <= sample_count {
        return points.to_vec();
    }
    let last = (points.len() - 1) as f64;
    let n = sample_count;
    (0..n)
        .map(|index| {
            let i = python_round(last * index as f64 / (n - 1) as f64) as usize;
            points[i]
        })
        .collect()
}

fn symmetric_cloud_distance(source: &UnitGeometrySignature, target: &UnitGeometrySignature) -> f64 {
    let left = mean_nearest_distance(&source.sample_points, &target.sample_points);
    let right = mean_nearest_distance(&target.sample_points, &source.sample_points);
    (left + right) / 2.0
}

fn mean_nearest_distance(points: &[Point3], candidates: &[Point3]) -> f64 {
    if points.is_empty() || candidates.is_empty() {
        return f64::INFINITY;
    }
    let sum: f64 = points
        .iter()
        .map(|p| nearest_distance(*p, candidates))
        .sum();
    sum / points.len() as f64
}

fn nearest_distance(point: Point3, candidates: &[Point3]) -> f64 {
    let mut min_d = f64::INFINITY;
    for c in candidates {
        let d = vector_distance(point, *c);
        if d < min_d {
            min_d = d;
        }
    }
    min_d
}

fn bbox_score(source: &UnitGeometrySignature, target: &UnitGeometrySignature, scale: f64) -> f64 {
    let center_score = vector_distance(source.center, target.center) / scale;
    let extent_score = vector_distance(source.extents, target.extents) / scale;
    (center_score + extent_score) / 2.0
}

fn tuple_distance(left: &[f64], right: &[f64]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let n = left.len().min(right.len());
    let sum: f64 = (0..n).map(|i| (left[i] - right[i]).abs()).sum();
    sum / n as f64
}

pub(crate) fn vector_distance(left: Point3, right: Point3) -> f64 {
    let dx = left.0 - right.0;
    let dy = left.1 - right.1;
    let dz = left.2 - right.2;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn axis_value(p: Point3, axis: usize) -> f64 {
    match axis {
        0 => p.0,
        1 => p.1,
        _ => p.2,
    }
}

/// Python `round` uses banker's rounding (round-half-to-even). Reproduced
/// here for parity in quantile/sample index selection.
fn python_round(x: f64) -> f64 {
    let floor = x.floor();
    let diff = x - floor;
    if (diff - 0.5).abs() < f64::EPSILON {
        if (floor as i64) % 2 == 0 {
            floor
        } else {
            floor + 1.0
        }
    } else {
        x.round()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_round_half_to_even() {
        assert_eq!(python_round(0.5), 0.0);
        assert_eq!(python_round(1.5), 2.0);
        assert_eq!(python_round(2.5), 2.0);
        assert_eq!(python_round(3.5), 4.0);
        assert_eq!(python_round(-0.5), 0.0);
    }

    #[test]
    fn bounding_box_simple() {
        let pts = vec![(0.0, 0.0, 0.0), (2.0, 0.0, 0.0), (0.0, 2.0, 0.0)];
        let (center, extents, diag) = bounding_box_stats(&pts);
        assert_eq!(center, (1.0, 1.0, 0.0));
        assert_eq!(extents, (2.0, 2.0, 0.0));
        assert!((diag - (8.0f64).sqrt()).abs() < 1e-9);
    }

    #[test]
    fn downsample_returns_all_when_fewer() {
        let pts: Vec<Point3> = (0..5).map(|i| (i as f64, 0.0, 0.0)).collect();
        let out = downsample_points(&pts, 10);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn score_identical_zero() {
        let sig = UnitGeometrySignature {
            file_id: 1,
            points: vec![(0.0, 0.0, 0.0), (1.0, 0.0, 0.0)],
            sample_points: vec![(0.0, 0.0, 0.0), (1.0, 0.0, 0.0)],
            vertex_count: 2,
            center: (0.5, 0.0, 0.0),
            extents: (1.0, 0.0, 0.0),
            diagonal: 1.0,
            axis_quantiles: vec![0.0, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            radial_quantiles: vec![0.0, 0.5, 1.0],
        };
        let s = score_signatures(&sig, &sig);
        assert!(s.abs() < 1e-9, "got {}", s);
    }
}
