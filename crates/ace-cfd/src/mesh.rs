use crate::CfdError;
use crate::vector::{Vector, add, norm, scale, sub};
use serde::{Deserialize, Serialize};

mod curved;
mod quadrature;

/// Body-fitted spherical forebody grid, revolved about the x axis.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SphereGrid {
    /// Sphere radius in m.
    pub radius: f64,
    /// Far-field distance from the sphere centre on the stagnation axis, in m.
    pub outer_radius: f64,
    /// Number of cells normal to the body.
    pub radial_cells: usize,
    /// Number of cells from stagnation line to equator.
    pub angular_cells: usize,
    /// Exponential radial stretching; zero gives a uniform radial distribution.
    pub stretching: f64,
    /// Optional Billig bow-shock shape guide; affects mesh geometry only.
    /// None gives concentric spherical grid lines. This does not impose a shock solution.
    #[serde(default)]
    pub shock_shape_mach: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Boundary {
    Wall,
    Inflow,
    Outflow,
    Axis,
}

#[derive(Debug, Clone)]
pub(crate) struct Cell {
    pub center: Vector,
    pub planar_center: Vector,
    pub volume: f64,
    pub planar_area: f64,
    pub faces: [usize; 4],
    pub radial_moments: Option<RadialMoments>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RadialMoments {
    pub scalar: [f64; 2],
    pub transverse: [f64; 2],
    pub covariance: [f64; 3],
    pub transverse_covariance: [f64; 3],
    pub scalar_covariance: [f64; 3],
    pub planar_covariance: [f64; 3],
    pub scalar_third_x: f64,
    pub planar_third_x: f64,
    pub scalar_mixed_xxz: f64,
    pub planar_mixed_xxz: f64,
    pub scalar_fourth_x: f64,
    pub planar_fourth_x: f64,
    pub reconstruction_points: [Vector; 64],
    pub transverse_y_squared: f64,
    pub planar_y_squared: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SurfacePoint {
    pub center: Vector,
    pub normal: Vector,
    pub area: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct Face {
    pub owner: usize,
    pub neighbor: Option<usize>,
    pub boundary: Boundary,
    pub center: Vector,
    pub normal: Vector,
    pub area: f64,
    pub quadrature: Option<[SurfacePoint; 4]>,
}

impl Face {
    pub(crate) fn integration_points(&self) -> impl Iterator<Item = SurfacePoint> + '_ {
        self.quadrature
            .iter()
            .flatten()
            .copied()
            .chain(self.quadrature.is_none().then_some(SurfacePoint {
                center: self.center,
                normal: self.normal,
                area: self.area,
            }))
    }

    pub(crate) fn at_point(&self, point: SurfacePoint) -> Self {
        Self {
            center: point.center,
            normal: point.normal,
            area: point.area,
            quadrature: None,
            ..self.clone()
        }
    }
}

/// Explicit cells and faces with per-radian axisymmetric measures.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub(crate) cells: Vec<Cell>,
    pub(crate) faces: Vec<Face>,
    pub(crate) radius: f64,
}

impl Mesh {
    /// Number of finite-volume cells.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Build a spherical forebody mesh. Cell order is angle-major, radius-minor.
    pub fn sphere(grid: SphereGrid) -> Result<Self, CfdError> {
        Self::from_vertices(grid, &grid.vertices()?)
    }

    /// Build a polar mesh with an exact spherical wall and integrated curved faces.
    pub fn curved_sphere(grid: SphereGrid) -> Result<Self, CfdError> {
        Self::curved_from_vertices(grid, &grid.vertices()?)
    }

    /// Integrate polar cells whose radial levels vary linearly between angular nodes.
    /// Each angular row must lie on one ray from the sphere centre.
    pub fn curved_from_vertices(grid: SphereGrid, vertices: &[Vector]) -> Result<Self, CfdError> {
        let mut mesh = Self::from_vertices(grid, vertices)?;
        curved::apply(&mut mesh, grid, vertices)?;
        Ok(mesh)
    }

    /// Build the same forebody topology from explicit angle-major, wall-to-far-field vertices.
    /// Coordinates override the geometric generator; resolution and body radius come from `grid`.
    pub fn from_vertices(grid: SphereGrid, vertices: &[Vector]) -> Result<Self, CfdError> {
        grid.validate()?;
        let nr = grid.radial_cells;
        let nt = grid.angular_cells;
        validate_vertices(grid, vertices)?;
        let node = |i: usize, j: usize| vertices[j * (nr + 1) + i];
        let mut cells = Vec::with_capacity(nr * nt);
        for j in 0..nt {
            for i in 0..nr {
                let points = [
                    node(i, j),
                    node(i, j + 1),
                    node(i + 1, j + 1),
                    node(i + 1, j),
                ];
                validate_cell(points, j * nr + i)?;
                let (area, planar_center, center) = polygon(points);
                cells.push(Cell {
                    center,
                    planar_center,
                    volume: area * planar_center[1],
                    planar_area: area,
                    faces: [0; 4],
                    radial_moments: None,
                });
            }
        }
        let mut mesh = Self {
            cells,
            faces: Vec::new(),
            radius: grid.radius,
        };
        mesh.radial_faces(grid, vertices);
        mesh.angular_faces(grid, vertices);
        if mesh.cells.iter().any(|c| {
            !c.volume.is_finite() || c.volume <= 0.0 || c.center.iter().any(|v| !v.is_finite())
        }) || mesh
            .faces
            .iter()
            .any(|f| !f.area.is_finite() || f.area < 0.0 || f.normal.iter().any(|v| !v.is_finite()))
        {
            return Err(CfdError::Configuration(
                "degenerate or nonfinite sphere mesh".into(),
            ));
        }
        Ok(mesh)
    }

    fn radial_faces(&mut self, grid: SphereGrid, vertices: &[Vector]) {
        let nr = grid.radial_cells;
        let node = |i: usize, j: usize| vertices[j * (nr + 1) + i];
        for j in 0..grid.angular_cells {
            for i in 0..=nr {
                let (owner, neighbor, a, b, boundary) = if i == 0 {
                    (j * nr, None, node(i, j), node(i, j + 1), Boundary::Wall)
                } else {
                    (
                        j * nr + i - 1,
                        (i < nr).then_some(j * nr + i),
                        node(i, j + 1),
                        node(i, j),
                        Boundary::Inflow,
                    )
                };
                let id = self.faces.len();
                self.faces.push(face(owner, neighbor, a, b, boundary));
                self.cells[owner].faces[usize::from(i != 0)] = id;
                if let Some(n) = neighbor {
                    self.cells[n].faces[0] = id;
                }
            }
        }
    }

    fn angular_faces(&mut self, grid: SphereGrid, vertices: &[Vector]) {
        let nr = grid.radial_cells;
        let node = |i: usize, j: usize| vertices[j * (nr + 1) + i];
        for j in 0..=grid.angular_cells {
            for i in 0..nr {
                let (owner, neighbor, a, b, boundary) = if j == 0 {
                    (i, None, node(i + 1, j), node(i, j), Boundary::Axis)
                } else {
                    (
                        (j - 1) * nr + i,
                        (j < grid.angular_cells).then_some(j * nr + i),
                        node(i, j),
                        node(i + 1, j),
                        Boundary::Outflow,
                    )
                };
                let id = self.faces.len();
                self.faces.push(face(owner, neighbor, a, b, boundary));
                self.cells[owner].faces[2 + usize::from(j != 0)] = id;
                if let Some(n) = neighbor {
                    self.cells[n].faces[2] = id;
                }
            }
        }
    }
}

fn validate_vertices(grid: SphereGrid, vertices: &[Vector]) -> Result<(), CfdError> {
    let width = grid.radial_cells + 1;
    let expected = width * (grid.angular_cells + 1);
    if vertices.len() != expected {
        return Err(CfdError::Configuration(format!(
            "expected {expected} mesh vertices, got {}",
            vertices.len()
        )));
    }
    let tolerance = 1e-12 * grid.radius;
    for (index, position) in vertices.iter().enumerate() {
        let j = index / width;
        if position.iter().any(|v| !v.is_finite())
            || position[1] < 0.0
            || position[0] > tolerance
            || position[2] != 0.0
            || (j == 0 && position[1].abs() > tolerance)
            || (j == grid.angular_cells && position[0].abs() > tolerance)
            || (index % width == 0 && (norm(*position) - grid.radius).abs() > tolerance)
        {
            return Err(CfdError::Configuration(format!(
                "invalid forebody vertex {index}"
            )));
        }
    }
    Ok(())
}

fn validate_cell(points: [Vector; 4], cell: usize) -> Result<(), CfdError> {
    for k in 0..4 {
        let a = sub(points[(k + 1) % 4], points[k]);
        let b = sub(points[(k + 2) % 4], points[(k + 1) % 4]);
        let cross = a[0] * b[1] - a[1] * b[0];
        if !cross.is_finite() || cross <= 0.0 {
            return Err(CfdError::Configuration(format!(
                "nonconvex or degenerate cell {cell}"
            )));
        }
    }
    Ok(())
}

impl SphereGrid {
    /// Export the mesh vertices, ordered angle-major and wall-to-far-field.
    pub fn vertices(self) -> Result<Vec<Vector>, CfdError> {
        self.validate()?;
        Ok((0..=self.angular_cells)
            .flat_map(|j| (0..=self.radial_cells).map(move |i| self.node(i, j)))
            .collect())
    }

    fn validate(self) -> Result<(), CfdError> {
        if self
            .shock_shape_mach
            .is_some_and(|m| !m.is_finite() || m <= 1.0 || !shock_radius_ratio(m, 0.5).is_finite())
        {
            return Err(CfdError::Configuration(
                "invalid bow-shock mesh guide Mach number".into(),
            ));
        }
        if !self.radius.is_finite()
            || !self.outer_radius.is_finite()
            || self.radius <= 0.0
            || self.outer_radius <= self.radius
            || self.radial_cells < 4
            || self.angular_cells < 4
            || !self.stretching.is_finite()
            || !(0.0..=8.0).contains(&self.stretching)
            || self
                .radial_cells
                .checked_mul(self.angular_cells)
                .is_none_or(|n| n > 10_000_000)
        {
            return Err(CfdError::Configuration(
                "invalid sphere radii, resolution, or stretching".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn node(self, i: usize, j: usize) -> Vector {
        let eta = i as f64 / self.radial_cells as f64;
        let fraction = if self.stretching == 0.0 {
            eta
        } else {
            (self.stretching * eta).exp_m1() / self.stretching.exp_m1()
        };
        let theta = std::f64::consts::FRAC_PI_2 * j as f64 / self.angular_cells as f64;
        let shape = self.shock_shape_mach.map_or(1.0, |mach| {
            (shock_radius_ratio(mach, theta) - 1.0) / (shock_radius_ratio(mach, 0.0) - 1.0)
        });
        let radius = self.radius + (self.outer_radius - self.radius) * fraction * shape;
        [-radius * theta.cos(), radius * theta.sin(), 0.0]
    }
}

// Billig's sphere hyperbola (doi:10.2514/3.28969), used only as a grid guide.
// The rationalized positive root avoids cancellation at the stagnation axis.
fn shock_radius_ratio(mach: f64, theta: f64) -> f64 {
    let nose = 1.0 + 0.143 * (3.24 / mach.powi(2)).exp();
    let curvature = 1.143 * (0.54 / (mach - 1.0).powf(1.2)).exp();
    let cotangent_squared = mach.powi(2) - 1.0;
    let offset = curvature * cotangent_squared;
    let constant = nose * (nose + 2.0 * offset);
    let discriminant =
        offset.powi(2) * theta.cos().powi(2) + cotangent_squared * constant * theta.sin().powi(2);
    constant / ((nose + offset) * theta.cos() + discriminant.sqrt())
}

fn face(owner: usize, neighbor: Option<usize>, a: Vector, b: Vector, boundary: Boundary) -> Face {
    let delta = sub(b, a);
    let length = norm(delta);
    let middle = scale(add(a, b), 0.5);
    let center = if middle[1] > 0.0 {
        add(middle, scale(delta, delta[1] / (12.0 * middle[1])))
    } else {
        middle
    };
    Face {
        owner,
        neighbor,
        boundary,
        center,
        normal: [delta[1] / length, -delta[0] / length, 0.0],
        area: length * middle[1],
        quadrature: None,
    }
}

fn polygon(points: [Vector; 4]) -> (f64, Vector, Vector) {
    let mut area = 0.0;
    let mut center = [0.0; 3];
    let mut xy = 0.0;
    let mut yy = 0.0;
    for i in 0..4 {
        let a = points[i];
        let b = points[(i + 1) % 4];
        let cross = a[0] * b[1] - b[0] * a[1];
        area += cross;
        center = add(center, scale(add(a, b), cross));
        xy += (2.0 * a[0] * a[1] + a[0] * b[1] + b[0] * a[1] + 2.0 * b[0] * b[1]) * cross / 24.0;
        yy += (a[1] * a[1] + a[1] * b[1] + b[1] * b[1]) * cross / 12.0;
    }
    let planar_center = scale(center, 1.0 / (3.0 * area));
    let volume = 0.5 * area * planar_center[1];
    (0.5 * area, planar_center, [xy / volume, yy / volume, 0.0])
}

#[cfg(test)]
mod tests;
