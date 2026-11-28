//! The labels of the duct runs, kept from writing over each other: where the label of a run would cover one already placed (two runs laid over each
//! other, side by side, or meeting), it is moved along the run or across it to the nearest free spot, and put back when the room is free again.

use cadcraft_geom::Vec2;

use super::duct::reads_upside_down;
use super::network::MAX_DUCTS;
use crate::{Drawing, EntityKind, Handle, Result, Space};

/// Most runs the pass looks at: it compares every label with the ones placed before it.
const MAX_LABELLED: usize = 800;
/// The width of a character of the label, in label heights (the text is drawn in a proportional font).
const CHAR_WIDTH: f64 = 0.62;
/// How far, in fractions of the run, a label is tried along it, and in label heights across it.
const ALONG: [f64; 7] = [0.0, 0.2, -0.2, 0.35, -0.35, 0.45, -0.45];
const ACROSS: [f64; 7] = [0.0, 1.4, -1.4, 2.8, -2.8, 4.2, -4.2];

/// A label's box in the drawing: its centre, the way it reads and its half sides.
#[derive(Clone, Copy)]
struct Boxed {
    centre: Vec2,
    dir: Vec2,
    half: Vec2,
}

impl Boxed {
    /// Whether the two boxes share any area (separating axis test).
    fn overlaps(&self, o: &Boxed) -> bool {
        let axes = [self.dir, self.dir.perp(), o.dir, o.dir.perp()];
        let radius = |b: &Boxed, axis: Vec2| b.half.x * b.dir.dot(axis).abs() + b.half.y * b.dir.perp().dot(axis).abs();
        let gap = o.centre - self.centre;
        !axes.iter().any(|a| gap.dot(*a).abs() >= radius(self, *a) + radius(o, *a))
    }
}

impl Drawing {
    /// Moves the labels of the duct runs of `space` so that none covers another; returns how many runs were redrawn.
    pub fn space_duct_labels(&mut self, space: &Space) -> Result<usize> {
        let mut runs: Vec<(Handle, super::DuctParams, Vec2, f64, f64)> = self
            .space(space)
            .into_iter()
            .flat_map(|s| s.iter())
            .filter_map(|e| match (&e.kind, super::duct_of(e)) {
                // A run whose label is left off takes no room and is not moved.
                (EntityKind::Insert(i), Some(p)) if !p.hide_label && !crate::Drawing::label_was_moved(e) => {
                    Some((e.handle, p, Vec2::new(i.insert.x, i.insert.y), i.rotation, i.scale.x))
                }
                _ => None,
            })
            .take(MAX_DUCTS + 1)
            .collect();
        // ponytail: the pass compares each label with those placed before it (quadratic), so a drawing with more runs than this keeps its labels where they are.
        if runs.len() > MAX_LABELLED {
            return Ok(0);
        }
        runs.sort_by_key(|r| r.0.0);
        let mut placed: Vec<Boxed> = Vec::new();
        let mut redraw: Vec<(Handle, super::DuctParams)> = Vec::new();
        for (h, p, at, rot, scale) in runs {
            let r = p.units.to_units(p.plan_width_mm()) / 2.0;
            let flip = reads_upside_down(rot);
            let height = p.units.to_units(p.label_height_mm);
            let width = CHAR_WIDTH * height * p.label_text().chars().count() as f64;
            let base = p.label_base(r, flip);
            let dir = Vec2::from_angle(if flip { rot + std::f64::consts::PI } else { rot });
            let boxed = |shift: Vec2| {
                let local = base + shift;
                Boxed { centre: at + Vec2::new(local.x * scale, local.y * scale).rotate(rot), dir, half: Vec2::new(width / 2.0, height * 0.6) }
            };
            let mut chosen = Vec2::new(0.0, 0.0);
            let mut found = None;
            'search: for across in ACROSS {
                for along in ALONG {
                    let shift = Vec2::new(along * p.length, across * height);
                    // Staying on the run: a label too long for it is only moved across.
                    if shift.x != 0.0 && (base.x + shift.x < width / 2.0 || base.x + shift.x > p.length - width / 2.0) {
                        continue;
                    }
                    let b = boxed(shift);
                    if !placed.iter().any(|q| q.overlaps(&b)) {
                        (chosen, found) = (shift, Some(b));
                        break 'search;
                    }
                }
            }
            // Nowhere is free: the label stays where it would be, and the next ones go round it.
            placed.push(found.unwrap_or_else(|| boxed(chosen)));
            let unit = p.units.mm_per_unit;
            let shift = (chosen.x != 0.0 || chosen.y != 0.0).then_some([chosen.x * unit, chosen.y * unit]);
            let same = |a: Option<[f64; 2]>, b: Option<[f64; 2]>| match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6,
                _ => false,
            };
            if !same(p.label_shift_mm, shift) {
                let mut q = p;
                q.label_shift_mm = shift;
                redraw.push((h, q));
            }
        }
        let n = redraw.len();
        for (h, p) in redraw {
            self.regenerate_duct(h, &p)?;
        }
        Ok(n)
    }
}
