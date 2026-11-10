//! Points spread evenly inside a room's outline, where its terminals go.

use cadcraft_geom::{Vec2, point_in_polygon};

use super::invalid;
use crate::Result;

/// Most terminals placed at once.
pub const MAX_DISTRIBUTED: usize = 500;
/// Most grid refinements tried to fit a count into an awkward outline.
const MAX_REFINEMENTS: usize = 64;

/// How many terminals to spread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribution {
    /// This many, on the most even grid that puts them all inside.
    Count(usize),
    /// A grid of this many rows and columns over the outline's extents; cells whose centre is
    /// outside the outline get none.
    Grid { rows: usize, cols: usize },
}

fn bounds(poly: &[Vec2]) -> Option<(Vec2, Vec2)> {
    let first = *poly.first()?;
    Some(poly.iter().fold((first, first), |(lo, hi), p| (Vec2::new(lo.x.min(p.x), lo.y.min(p.y)), Vec2::new(hi.x.max(p.x), hi.y.max(p.y)))))
}

/// The centres of a `rows` × `cols` grid over `lo..hi` that are inside `poly`, row by row.
fn grid(poly: &[Vec2], lo: Vec2, hi: Vec2, rows: usize, cols: usize) -> Vec<Vec2> {
    let size = hi - lo;
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let p = Vec2::new(lo.x + size.x * (c as f64 + 0.5) / cols as f64, lo.y + size.y * (r as f64 + 0.5) / rows as f64);
            if point_in_polygon(poly, p) {
                out.push(p);
            }
        }
    }
    out
}

/// Points spread inside `poly` as `how` asks.
pub fn distribute_in_polygon(poly: &[Vec2], how: Distribution) -> Result<Vec<Vec2>> {
    let (lo, hi) = bounds(poly).ok_or_else(|| invalid("The space has no outline."))?;
    let size = hi - lo;
    if !(size.x > 0.0 && size.y > 0.0 && size.x.is_finite() && size.y.is_finite()) {
        return Err(invalid("The space's outline encloses no area."));
    }
    match how {
        Distribution::Grid { rows, cols } => {
            if rows == 0 || cols == 0 || rows.saturating_mul(cols) > MAX_DISTRIBUTED {
                return Err(invalid(format!("A grid has at least 1 row and 1 column, and at most {MAX_DISTRIBUTED} cells.")));
            }
            let pts = grid(poly, lo, hi, rows, cols);
            if pts.is_empty() {
                return Err(invalid("No cell of that grid falls inside the space."));
            }
            Ok(pts)
        }
        Distribution::Count(n) => {
            if n == 0 || n > MAX_DISTRIBUTED {
                return Err(invalid(format!("Place 1 to {MAX_DISTRIBUTED} terminals.")));
            }
            // Columns along the longer side, in proportion to the outline's extents.
            let aspect = size.x / size.y;
            let mut cols = ((n as f64 * aspect).sqrt().round() as usize).clamp(1, n);
            let mut rows = n.div_ceil(cols);
            for _ in 0..MAX_REFINEMENTS {
                let pts = grid(poly, lo, hi, rows, cols);
                if pts.len() == n {
                    return Ok(pts);
                }
                if pts.len() > n {
                    // Take `n` of them, evenly through the list.
                    let len = pts.len();
                    return Ok((0..n).filter_map(|k| pts.get(k * len / n).copied()).collect());
                }
                // Too few cells fall inside (an L-shaped room): refine the grid.
                if aspect >= 1.0 { cols += 1 } else { rows += 1 }
            }
            Err(invalid("The terminals don't fit inside the space: place them one by one."))
        }
    }
}
