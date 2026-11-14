//! Where ducts cross in plan, and which one passes under: the duct that passes under is drawn with
//! its sides broken across the other, as a plan shows it. Nothing about the network changes: a
//! crossing is not a junction, the ducts stay apart for the sizing.

use cadcraft_geom::Vec2;

use super::duct::{AXIS_REACH, CoverMark, DuctJog, MAX_GAPS, MAX_JOGS};
use super::{DuctParams, DuctShape, HvacUnits, MAX_DUCTS, duct_of, invalid};
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
    /// The ducts do not cross: `a` runs parallel to `b`, inside its width, over a stretch (as when the
    /// supply and the extract leave stacked collars together); `at` is the middle of that stretch.
    pub overlap: bool,
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
    // A touch at an end counts: the ends of ducts drawn by rotation are exact only to rounding.
    let range = -1e-9..=1.0 + 1e-9;
    (range.contains(&t) && range.contains(&u)).then(|| (a + r * t, t.max(0.0) * r.len(), u.max(0.0) * s.len()))
}

/// Where duct `a`–`b` (of width `wa`) lies in the width `wc` of the parallel duct `c`–`d` (its axis inside it, itself the narrower):
/// the stretch of `a`–`b` they share, drawing units from `a`. `None` when they are not parallel, not
/// one inside the other, or share less than `min`.
fn overlap_stretch(a: Vec2, b: Vec2, wa: f64, c: Vec2, d: Vec2, wc: f64, min: f64) -> Option<(f64, f64)> {
    let (u, v) = ((b - a).normalized(), (d - c).normalized());
    if u.cross(v).abs() > 0.02 {
        return None;
    }
    // The narrower duct's axis is inside the width of the wider (a click that snapped to an edge leaves it a few
    // centimetres off the axis, its side sticking out a little): it lies under.
    let offset = (c - a).cross(u).abs();
    if offset > wc / 2.0 + 1e-6 || wa > wc + 1e-6 {
        return None;
    }
    let (tc, td) = ((c - a).dot(u), (d - a).dot(u));
    let (lo, hi) = (tc.min(td).max(0.0), tc.max(td).min(a.dist(b)));
    (hi - lo > min).then_some((lo, hi))
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
                let Some((at, ta, tb)) = cross_point(*a0, *a1, *b0, *b1) else {
                    // Parallel and one inside the other: the narrower is the one that can lie under.
                    let (wa, wb) = (pa.units.to_units(pa.plan_width_mm()), pb.units.to_units(pb.plan_width_mm()));
                    let found = overlap_stretch(*a0, *a1, wa, *b0, *b1, wb, 10.0 * tol)
                        .map(|s| (*ha, *hb, pa, *a0, *a1, s))
                        .or_else(|| overlap_stretch(*b0, *b1, wb, *a0, *a1, wa, 10.0 * tol).map(|s| (*hb, *ha, pb, *b0, *b1, s)));
                    if let Some((inner, outer, p, s0, s1, (lo, hi))) = found
                        && out.len() < 10_000
                    {
                        let mid = (lo + hi) / 2.0;
                        out.push(DuctCrossing { a: inner, b: outer, at: s0 + (s1 - s0).normalized() * mid, resolved: covers(p, mid), overlap: true });
                    }
                    continue;
                };
                let (ra, rb) = (reach(pa), reach(pb));
                if ta > ra && ta < a0.dist(*a1) - ra && tb > rb && tb < b0.dist(*b1) - rb && out.len() < 10_000 {
                    out.push(DuctCrossing { a: *ha, b: *hb, at, resolved: covers(pa, ta) || covers(pb, tb), overlap: false });
                }
            }
        }
        out
    }

    /// Draws broken, at every crossing and overlap still to draw, the duct that is lower: where both altitudes
    /// are known and differ by a step or more, the one with the lower underside passes under. Returns how many.
    pub fn cross_ducts_by_altitude(&mut self, clearance_mm: f64) -> Result<usize> {
        self.refresh_cover_marks()?;
        let mut drawn = 0;
        for x in self.duct_crossings() {
            if x.resolved {
                continue;
            }
            let (Some((pa, ..)), Some((pb, ..))) = (self.duct_ends(x.a), self.duct_ends(x.b)) else { continue };
            let (Some(a), Some(b)) = (pa.altitude_mm(), pb.altitude_mm()) else { continue };
            if (a - b).abs() < super::duct::LEVEL_STEP_MIN_MM {
                continue;
            }
            // A duct lying inside the width of another can only be hidden by it when it is the lower one.
            let (under, over) = if a < b { (x.a, x.b) } else { (x.b, x.a) };
            if x.overlap && under != x.a {
                continue;
            }
            if self.pass_duct_under(under, over, clearance_mm).is_ok() {
                drawn += 1;
            }
        }
        Ok(drawn)
    }

    /// Gives the marks of crossings drawn before marks said whether the duct over runs along this one, and how
    /// wide it is (they read as unknown), what they would have been drawn with now.
    fn refresh_cover_marks(&mut self) -> Result<()> {
        let stale: Vec<Handle> =
            self.model.iter().filter(|e| duct_of(e).is_some_and(|p| p.covers.iter().any(|c| c.over_width_mm <= 0.0))).map(|e| e.handle).collect();
        for h in stale {
            let Some((mut p, a, b)) = self.duct_ends(h) else { continue };
            let mut changed = false;
            for c in &mut p.covers {
                let Some((po, c0, d0)) = (c.over_width_mm <= 0.0).then(|| self.duct_ends(Handle(c.over))).flatten() else { continue };
                (c.over_width_mm, c.along) = (po.plan_width_mm(), cross_point(a, b, c0, d0).is_none());
                changed = true;
            }
            if changed {
                self.regenerate_duct(h, &p)?;
            }
        }
        Ok(())
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
        let m = p.units.mm_per_unit;
        let length = a.dist(b) * m;
        // Each stretch it passes under, with the part of it inside the other's width (its sides are dashed there).
        let across = cross_point(a, b, c, d).is_some();
        let new_gaps: Vec<([f64; 2], [f64; 2])> = match cross_point(a, b, c, d) {
            Some((_, along, along_over)) => {
                let sin = (b - a).normalized().cross((d - c).normalized()).abs().max(0.05);
                let inside = po.plan_width_mm() / 2.0 / sin;
                let half = inside + clearance_mm;
                // The crossing was drawn the other way before: that duct is whole again.
                let mut reverse = po.clone();
                let u = along_over * po.units.mm_per_unit;
                if reverse.gaps_mm.iter().any(|g| g[0] <= u && u <= g[1]) {
                    reverse.gaps_mm.retain(|g| !(g[0] <= u && u <= g[1]));
                    reverse.covers.retain(|c| !(c.gap_mm[0] <= u && u <= c.gap_mm[1]));
                    self.regenerate_duct(over, &reverse)?;
                }
                let at = along * m;
                vec![([(at - half).max(0.0), (at + half).min(length)], [(at - inside).max(0.0), (at + inside).min(length)])]
            }
            None => {
                // Parallel: the duct lies under the other, and under the pieces of its run, along the stretches
                // they share, if it is wholly inside their width. Only the narrower can lie under: a click on two
                // overlapping ducts may have named them the wrong way round.
                let stretches = self.run_stretches(under, over);
                if stretches.is_empty() {
                    if self.run_stretches(over, under).is_empty() {
                        return Err(invalid("The two ducts do not cross, and the first is not wholly inside the width of the second."));
                    }
                    return self.pass_duct_under(over, under, clearance_mm);
                }
                stretches.into_iter().map(|(lo, hi)| ([lo * m, hi * m], [lo * m, hi * m])).collect()
            }
        };
        let mut hull = [f64::MAX, f64::MIN];
        for (gap, dash) in new_gaps {
            // A new stretch absorbs the ones it overlaps.
            let mut gaps: Vec<[f64; 2]> = p.gaps_mm.iter().copied().filter(|g| g[1] < gap[0] || g[0] > gap[1]).collect();
            let merged = p.gaps_mm.iter().filter(|g| !(g[1] < gap[0] || g[0] > gap[1])).fold(gap, |acc, g| [acc[0].min(g[0]), acc[1].max(g[1])]);
            gaps.push(merged);
            gaps.sort_by(|x, y| x[0].total_cmp(&y[0]));
            if gaps.len() > MAX_GAPS {
                return Err(invalid(format!("A duct passes under at most {MAX_GAPS} others.")));
            }
            p.gaps_mm = gaps;
            hull = [hull[0].min(merged[0]), hull[1].max(merged[1])];
            // Its mark: the stretches it absorbed are one, and the dashes cover what theirs did.
            let dash = p
                .covers
                .iter()
                .filter(|c| !(c.gap_mm[1] < gap[0] || c.gap_mm[0] > gap[1]))
                .fold(dash, |acc, c| [acc[0].min(c.dash_mm[0]), acc[1].max(c.dash_mm[1])]);
            p.covers.retain(|c| c.gap_mm[1] < gap[0] || c.gap_mm[0] > gap[1]);
            p.covers.push(CoverMark {
                gap_mm: merged,
                dash_mm: dash,
                over_air: po.air,
                over: over.0,
                over_width_mm: po.plan_width_mm(),
                along: !across,
            });
        }
        self.regenerate_duct(under, &p)?;
        // The duct over is drawn after the one under, so the mask of the crossing is not over its own sides.
        self.model.bring_to_front(over);
        Ok(hull)
    }

    /// The stretches of duct `inner` (drawing units from its start) that lie wholly inside the width of
    /// the run of duct `of`: `of` and the ducts parallel to it on the same axis and of the same width, as
    /// the pieces of a run cut where another duct started on it. Joined when they touch.
    fn run_stretches(&self, inner: Handle, of: Handle) -> Vec<(f64, f64)> {
        let (Some((pi, a, b)), Some((po, c, d))) = (self.duct_ends(inner), self.duct_ends(of)) else { return Vec::new() };
        let wa = pi.units.to_units(pi.plan_width_mm());
        let mm = po.units.to_units(1.0);
        let v = (d - c).normalized();
        let mut found = Vec::new();
        for e in self.model.iter() {
            if e.handle == inner {
                continue;
            }
            let Some((q, c2, d2)) = self.duct_ends(e.handle) else { continue };
            let same_run = e.handle == of
                || ((q.plan_width_mm() - po.plan_width_mm()).abs() < 0.5
                    && v.cross((d2 - c2).normalized()).abs() < 0.02
                    && (c2 - c).cross(v).abs() < mm
                    && (d2 - c).cross(v).abs() < mm);
            if same_run && let Some(s) = overlap_stretch(a, b, wa, c2, d2, q.units.to_units(q.plan_width_mm()), 0.0) {
                found.push(s);
            }
        }
        found.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut out: Vec<(f64, f64)> = Vec::new();
        for (lo, hi) in found {
            match out.last_mut() {
                Some(last) if lo <= last.1 + 2.0 * mm => last.1 = last.1.max(hi),
                _ => out.push((lo, hi)),
            }
        }
        out
    }

    /// Makes duct `duct` go over (or, with `down`, under) duct `other` by four 45° elbows: it leaves its
    /// line, runs level across `other` (its width and `clearance_mm` on each side), and comes back. The
    /// rise is `shift_mm`, else enough to clear the other duct's height by 50 mm; at 45° it takes as much
    /// length on each side. Returns the jog; an error when the ducts do not cross or the elbows do not
    /// fit on the duct.
    pub fn jog_duct_around(&mut self, duct: Handle, other: Handle, down: bool, shift_mm: Option<f64>, clearance_mm: f64) -> Result<DuctJog> {
        if !(clearance_mm.is_finite() && (0.0..=1000.0).contains(&clearance_mm)) {
            return Err(invalid("The clearance is 0 to 1000 mm."));
        }
        let (mut p, a, b) = self.duct_ends(duct).ok_or_else(|| invalid("Select ducts only."))?;
        let (po, c, d) = self.duct_ends(other).ok_or_else(|| invalid("Select ducts only."))?;
        let (_, along, _) = cross_point(a, b, c, d).ok_or_else(|| invalid("The two ducts do not cross."))?;
        let height = |p: &DuctParams| if p.shape == DuctShape::Rectangular { p.height_mm } else { p.diameter_mm };
        let shift = shift_mm.unwrap_or_else(|| (height(&p) + height(&po)) / 2.0 + 50.0);
        let sin = (b - a).normalized().cross((d - c).normalized()).abs().max(0.05);
        let jog = DuctJog { centre_mm: along * p.units.mm_per_unit, half_mm: po.plan_width_mm() / 2.0 / sin + clearance_mm, shift_mm: shift, down };
        // A new jog replaces the ones it overlaps.
        let (lo, hi) = (jog.elbows_mm()[0], jog.elbows_mm()[3]);
        p.jogs.retain(|j| j.elbows_mm()[3] < lo || j.elbows_mm()[0] > hi);
        p.jogs.push(jog);
        p.jogs.sort_by(|x, y| x.centre_mm.total_cmp(&y.centre_mm));
        if p.jogs.len() > MAX_JOGS {
            return Err(invalid(format!("A duct makes at most {MAX_JOGS} jogs.")));
        }
        p.validate().map_err(|_| invalid("The four elbows do not fit on the duct: the crossing is too near its ends."))?;
        self.regenerate_duct(duct, &p)?;
        Ok(jog)
    }
}
