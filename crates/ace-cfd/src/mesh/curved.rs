use super::quadrature::{CELL_GAUSS, GAUSS};
use super::{Cell, Face, Mesh, RadialMoments, SphereGrid, SurfacePoint};
use crate::CfdError;
use crate::vector::{Vector, add, norm, scale};

pub(super) fn apply(
    mesh: &mut Mesh,
    grid: SphereGrid,
    vertices: &[Vector],
) -> Result<(), CfdError> {
    let nr = grid.radial_cells;
    let angles = validate_rays(grid, vertices)?;
    let radius = |i: usize, j: usize| {
        if i == 0 {
            grid.radius
        } else {
            norm(vertices[j * (nr + 1) + i])
        }
    };
    for j in 0..grid.angular_cells {
        let angular = [angles[j], angles[j + 1]];
        for i in 0..nr {
            let radii = [
                radius(i, j),
                radius(i, j + 1),
                radius(i + 1, j),
                radius(i + 1, j + 1),
            ];
            integrate_cell(&mut mesh.cells[j * nr + i], angular, radii, grid.radius);
        }
        for i in 0..=nr {
            let face = &mut mesh.faces[j * (nr + 1) + i];
            radial_face(
                face,
                angular,
                [radius(i, j), radius(i, j + 1)],
                if i == 0 { -1.0 } else { 1.0 },
            );
        }
    }
    let offset = grid.angular_cells * (nr + 1);
    for (j, angle) in angles.into_iter().enumerate() {
        for i in 0..nr {
            let face = &mut mesh.faces[offset + j * nr + i];
            angular_face(
                face,
                angle,
                [radius(i, j), radius(i + 1, j)],
                if j == 0 { -1.0 } else { 1.0 },
            );
        }
    }
    if mesh.cells.iter().any(invalid_cell)
        || mesh
            .faces
            .iter()
            .flat_map(Face::integration_points)
            .any(|p| {
                !p.area.is_finite()
                    || p.area < 0.0
                    || p.center.into_iter().chain(p.normal).any(|v| !v.is_finite())
            })
    {
        return Err(CfdError::Configuration(
            "nonfinite curved cell geometry".into(),
        ));
    }
    Ok(())
}

fn invalid_cell(c: &Cell) -> bool {
    !c.volume.is_finite()
        || c.volume <= 0.0
        || c.center.iter().any(|v| !v.is_finite())
        || c.radial_moments.as_ref().is_none_or(|m| {
            m.scalar
                .into_iter()
                .chain(m.transverse)
                .chain([
                    m.covariance[0],
                    m.covariance[2],
                    m.transverse_covariance[0],
                    m.transverse_covariance[2],
                    m.scalar_covariance[0],
                    m.scalar_covariance[2],
                    m.planar_covariance[0],
                    m.planar_covariance[2],
                    m.scalar_fourth_x,
                    m.planar_fourth_x,
                    m.transverse_y_squared,
                    m.planar_y_squared,
                ])
                .any(|v| !v.is_finite() || v <= 0.0)
                || [
                    m.covariance[1],
                    m.transverse_covariance[1],
                    m.scalar_covariance[1],
                    m.planar_covariance[1],
                    m.scalar_third_x,
                    m.planar_third_x,
                    m.scalar_mixed_xxz,
                    m.planar_mixed_xxz,
                ]
                .iter()
                .any(|v| !v.is_finite())
        })
}

fn validate_rays(grid: SphereGrid, vertices: &[Vector]) -> Result<Vec<f64>, CfdError> {
    let mut angles = Vec::with_capacity(grid.angular_cells + 1);
    for row in vertices.chunks_exact(grid.radial_cells + 1) {
        let angle = row[0][1].atan2(-row[0][0]);
        if angles.last().is_some_and(|last| *last >= angle)
            || row
                .iter()
                .any(|p| (p[1].atan2(-p[0]) - angle).abs() > 1e-12)
            || row.windows(2).any(|pair| norm(pair[0]) >= norm(pair[1]))
            || norm(row[1]) <= grid.radius
        {
            return Err(CfdError::Configuration(
                "curved cells require ordered polar rays and radii".into(),
            ));
        }
        angles.push(angle);
    }
    Ok(angles)
}

fn integrate_cell(cell: &mut Cell, angles: [f64; 2], radii: [f64; 4], wall_radius: f64) {
    let [a, b] = angles;
    let mut area = 0.0;
    let mut volume = 0.0;
    let mut planar = [0.0; 3];
    let mut centroid = [0.0; 3];
    let mut moments = [0.0; 2];
    let mut transverse = [0.0; 2];
    let mut points = [([0.0; 3], 0.0); 64];
    let mut transverse_y_squared = 0.0;
    let mut planar_y_squared = 0.0;
    for (it, (t, wt)) in CELL_GAUSS.into_iter().enumerate() {
        let angle = a + t * (b - a);
        let lower = (1.0 - t) * radii[0] + t * radii[1];
        let upper = (1.0 - t) * radii[2] + t * radii[3];
        for (is, (s, ws)) in CELL_GAUSS.into_iter().enumerate() {
            let radius = lower + s * (upper - lower);
            let position = [-radius * angle.cos(), radius * angle.sin(), 0.0];
            let da = wt * ws * (b - a) * (upper - lower) * radius;
            let dv = da * position[1];
            area += da;
            volume += dv;
            planar = add(planar, scale(position, da));
            centroid = add(centroid, scale(position, dv));
            points[it * 8 + is] = (position, dv);
            transverse_y_squared += position[1].powi(3) * dv;
            planar_y_squared += position[1] * position[1] * da;
            let distance = radius - wall_radius;
            moments[0] += distance * dv;
            moments[1] += distance * distance * dv;
            transverse[0] += distance * dv * position[1];
            transverse[1] += distance * distance * dv * position[1];
        }
    }
    cell.volume = volume;
    cell.planar_area = area;
    cell.center = scale(centroid, 1.0 / volume);
    cell.planar_center = scale(planar, 1.0 / area);
    cell.radial_moments = Some(cell_moments(
        cell,
        &points,
        moments.map(|m| m / volume),
        transverse.map(|m| m / centroid[1]),
        [transverse_y_squared / centroid[1], planar_y_squared / area],
    ));
}

fn cell_moments(
    cell: &Cell,
    points: &[(Vector, f64); 64],
    scalar: [f64; 2],
    transverse: [f64; 2],
    yy: [f64; 2],
) -> RadialMoments {
    let physical_covariance = covariance(points, cell.center, cell.volume);
    let volume_higher = higher_moments(
        points,
        [
            cell.center[0],
            cell.center[1].powi(2) + physical_covariance[2],
        ],
        cell.center[1],
        cell.volume,
        false,
    );
    let planar_higher = higher_moments(
        points,
        [cell.planar_center[0], yy[1]],
        cell.center[1],
        cell.planar_area,
        true,
    );
    RadialMoments {
        scalar,
        transverse,
        covariance: physical_covariance,
        transverse_covariance: transverse_covariance(points, cell.center),
        scalar_covariance: even_covariance(
            points,
            [
                cell.center[0],
                cell.center[1].powi(2) + physical_covariance[2],
            ],
            cell.center[1],
            cell.volume,
            false,
        ),
        planar_covariance: even_covariance(
            points,
            [cell.planar_center[0], yy[1]],
            cell.center[1],
            cell.planar_area,
            true,
        ),
        scalar_third_x: volume_higher[0],
        planar_third_x: planar_higher[0],
        scalar_mixed_xxz: volume_higher[1],
        planar_mixed_xxz: planar_higher[1],
        scalar_fourth_x: volume_higher[2],
        planar_fourth_x: planar_higher[2],
        reconstruction_points: points.map(|(position, _)| position),
        transverse_y_squared: yy[0],
        planar_y_squared: yy[1],
    }
}

fn higher_moments(
    points: &[(Vector, f64); 64],
    center: [f64; 2],
    radius: f64,
    measure: f64,
    planar: bool,
) -> [f64; 3] {
    points
        .iter()
        .fold([0.0; 3], |mut sum, (p, volume_weight)| {
            let weight = if planar {
                volume_weight / p[1]
            } else {
                *volume_weight
            };
            let x = p[0] - center[0];
            let z = (p[1] * p[1] - center[1]) / (2.0 * radius);
            for (total, value) in sum.iter_mut().zip([x.powi(3), x * x * z, x.powi(4)]) {
                *total += weight * value;
            }
            sum
        })
        .map(|v| v / measure)
}

fn even_covariance(
    points: &[(Vector, f64); 64],
    center: [f64; 2],
    radius: f64,
    measure: f64,
    planar: bool,
) -> [f64; 3] {
    points
        .iter()
        .fold([0.0; 3], |mut sum, (p, volume_weight)| {
            let weight = if planar {
                volume_weight / p[1]
            } else {
                *volume_weight
            };
            let x = p[0] - center[0];
            let z = (p[1].powi(2) - center[1]) / (2.0 * radius);
            for (s, value) in sum.iter_mut().zip([x * x, x * z, z * z]) {
                *s += value * weight;
            }
            sum
        })
        .map(|value| value / measure)
}

fn covariance(points: &[(Vector, f64); 64], center: Vector, volume: f64) -> [f64; 3] {
    // Center first so thin-cell variances do not subtract nearly equal squared radii.
    points
        .iter()
        .fold([0.0; 3], |mut sum, (p, weight)| {
            let x = p[0] - center[0];
            let y = p[1] - center[1];
            for (s, value) in sum.iter_mut().zip([x * x, x * y, y * y]) {
                *s += value * weight;
            }
            sum
        })
        .map(|value| value / volume)
}

fn transverse_covariance(points: &[(Vector, f64); 64], center: Vector) -> [f64; 3] {
    // The even field v/y uses an extra y weight and coordinates (x, y²).
    // Center those coordinates before multiplying to retain thin-cell precision.
    let measure: f64 = points.iter().map(|(p, weight)| p[1] * weight).sum();
    let mean_x = points
        .iter()
        .map(|(p, weight)| p[0] * p[1] * weight)
        .sum::<f64>()
        / measure;
    let mean_yy = points
        .iter()
        .map(|(p, weight)| p[1].powi(3) * weight)
        .sum::<f64>()
        / measure;
    points
        .iter()
        .fold([0.0; 3], |mut sum, (p, weight)| {
            let x = p[0] - mean_x;
            let yy = (p[1] * p[1] - mean_yy) / (2.0 * center[1]);
            for (s, value) in sum.iter_mut().zip([x * x, x * yy, yy * yy]) {
                *s += value * p[1] * weight;
            }
            sum
        })
        .map(|value| value / measure)
}

fn radial_face(face: &mut Face, angles: [f64; 2], radii: [f64; 2], sign: f64) {
    let span = angles[1] - angles[0];
    let slope = (radii[1] - radii[0]) / span;
    let points = GAUSS.map(|(t, weight)| {
        let angle = angles[0] + t * span;
        let radius = radii[0] + t * (radii[1] - radii[0]);
        let tangent = [
            -slope * angle.cos() + radius * angle.sin(),
            slope * angle.sin() + radius * angle.cos(),
            0.0,
        ];
        let length = norm(tangent);
        SurfacePoint {
            center: [-radius * angle.cos(), radius * angle.sin(), 0.0],
            normal: scale([-tangent[1], tangent[0], 0.0], sign / length),
            area: weight * span * length * radius * angle.sin(),
        }
    });
    set_face(face, points);
}

fn angular_face(face: &mut Face, angle: f64, radii: [f64; 2], sign: f64) {
    let points = GAUSS.map(|(t, weight)| {
        let radius = radii[0] + t * (radii[1] - radii[0]);
        SurfacePoint {
            center: [-radius * angle.cos(), radius * angle.sin(), 0.0],
            normal: scale([angle.sin(), angle.cos(), 0.0], sign),
            area: weight * (radii[1] - radii[0]) * radius * angle.sin(),
        }
    });
    set_face(face, points);
}

fn set_face(face: &mut Face, points: [SurfacePoint; 4]) {
    let area = points.iter().map(|p| p.area).sum::<f64>();
    if area > 0.0 {
        let centroid = points
            .iter()
            .fold([0.0; 3], |s, p| add(s, scale(p.center, p.area)));
        let normal = points
            .iter()
            .fold([0.0; 3], |s, p| add(s, scale(p.normal, p.area)));
        face.center = scale(centroid, 1.0 / area);
        face.normal = scale(normal, 1.0 / norm(normal));
    }
    face.area = area;
    face.quadrature = Some(points);
}

#[cfg(test)]
mod tests;
