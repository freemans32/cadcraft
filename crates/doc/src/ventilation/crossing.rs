//! Where ducts cross in plan, and which one passes under: the duct that passes under is drawn with
//! its sides broken across the other, as a plan shows it. Nothing about the network changes: a
//! crossing is not a junction, the ducts stay apart for the sizing.

use cadcraft_geom::Vec2;

use super::duct::{AXIS_REACH, MAX_GAPS};
use super::{DuctParams, HvacUnits, MAX_DUCTS, invalid};
use crate::{Handle, Result};

/// Distance, in millimetres, within which two ends are one point (as for the network).
const MEET_MM: f64 = 10.0;

/// Two ducts whose axes cross away from the ends of both: not a tee or a corner.
#[derive(Clone, Debug, PartialEq)]
pub struct DuctCrossing {
    pub a: Handle,
    pub b: Handle,
    /// Where the axes cross.
    pub at: Vec2,
    /// Whether one of the two is already drawn broken there.
    pub resolved: bool,
}

/// Where segment `a`–`b` crosses segment `c`–`d`: the point and how far along each (drawing units).
fn cross_point(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<(Vec2, f64, f64)> {
    let (r, s) = (b - a, d - c);
    let denom = r.cross(s);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).cross(s) / denom;
    let u = (c - a).cross(r) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| (a + r * t, t * r.len(), u * s.len()))
}

fn covers(p: &DuctParams, along_units: f64) -> bool {
    let mm = along_units * p.units.mm_per_unit;
    p.gaps_mm.iter().any(|g| g[0] <= mm && mm <= g[1])
}

impl crate::Drawing {
    /// The crossings of the ducts in model space, those still to draw broken and those already drawn so.
    pub fn duct_crossings(&self) -> Vec<DuctCrossing> {
        let units = HvacUnits::of(self);
        let tol = units.to_units(MEET_MM);
        let ducts: Vec<(Handle, DuctParams, Vec2, Vec2)> =
            self.model.iter().filter_map(|e| self.duct_ends(e.handle).map(|(p, a, b)| (e.handle, p, a, b))).take(MAX_DUCTS).collect();
        let reach = |p: &DuctParams| tol.max(AXIS_REACH * p.units.to_units(p.plan_width_mm()));
        let mut out = Vec::new();
        for (i, (ha, pa, a0, a1)) in ducts.iter().enumerate() {
            for (hb, pb, b0, b1) in ducts.iter().skip(i + 1) {
                let (lo, hi) = (Vec2::new(a0.x.min(a1.x), a0.y.min(a1.y)), Vec2::new(a0.x.max(a1.x), a0.y.max(a1.y)));
                if b0.x.max(b1.x) < lo.x || b0.x.min(b1.x) > hi.x || b0.y.max(b1.y) < lo.y || b0.y.min(b1.y) > hi.y {
                    continue;
                }
                let Some((at, ta, tb)) = cross_point(*a0, *a1, *b0, *b1) else { continue };
                let (ra, rb) = (reach(pa), reach(pb));
                if ta > ra && ta < a0.dist(*a1) - ra && tb > rb && tb < b0.dist(*b1) - rb && out.len() < 10_000 {
                    out.push(DuctCrossing { a: *ha, b: *hb, at, resolved: covers(pa, ta) || covers(pb, tb) });
                }
            }
        }
        out
    }

    /// Draws duct `under` with its sides broken across duct `over`, with `clearance_mm` more than
    /// the width of `over` (and more still when they cross at a slant). Returns the stretch, mm from
    /// the start of `under`.
    pub fn pass_duct_under(&mut self, under: Handle, over: Handle, clearance_mm: f64) -> Result<[f64; 2]> {
        if !(clearance_mm.is_finite() && (0.0..=1000.0).contains(&clearance_mm)) {
            return Err(invalid("The clearance is 0 to 1000 mm."));
        }
        let (mut p, a, b) = self.duct_ends(under).ok_or_else(|| invalid("Select ducts only."))?;
        let (po, c, d) = self.duct_ends(over).ok_or_else(|| invalid("Select ducts only."))?;
        let (_, along, _) = cross_point(a, b, c, d).ok_or_else(|| invalid("The two ducts do not cross."))?;
        let sin = (b - a).normalized().cross((d - c).normalized()).abs().max(0.05);
        let m = p.units.mm_per_unit;
        let half = po.plan_width_mm() / 2.0 / sin + clearance_mm;
        let (centre, length) = (along * m, a.dist(b) * m);
        let gap = [(centre - half).max(0.0), (centre + half).min(length)];
        // A new stretch absorbs the ones it overlaps.
        let mut gaps: Vec<[f64; 2]> = p.gaps_mm.iter().copied().filter(|g| g[1] < gap[0] || g[0] > gap[1]).collect();
        let merged = p.gaps_mm.iter().filter(|g| !(g[1] < gap[0] || g[0] > gap[1])).fold(gap, |acc, g| [acc[0].min(g[0]), acc[1].max(g[1])]);
        gaps.push(merged);
        gaps.sort_by(|x, y| x[0].total_cmp(&y[0]));
        if gaps.len() > MAX_GAPS {
            return Err(invalid(format!("A duct passes under at most {MAX_GAPS} others.")));
        }
        p.gaps_mm = gaps;
        self.regenerate_duct(under, &p)?;
        Ok(merged)
    }
}
