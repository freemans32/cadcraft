//! Recognising the doors and windows of a plan drawn without CADCraft's walls and openings: the block
//! references whose block or layer is named like a door or a window, and whose insertion point lies on a wall, are
//! proposed as openings of that wall, with the width the block takes along it. Nothing is changed here;
//! the caller makes the openings and sets the old blocks aside.

use cadcraft_geom::Vec2;

use crate::extents::entity_bounds;
use crate::{Drawing, EntityKind, Handle, OpeningKind, opening_of, wall_of};

/// Words that name a door, in a block or a layer name (lower case, without accents).
pub const DOOR_WORDS: &[&str] = &["door", "porte"];
/// Words that name a window.
pub const WINDOW_WORDS: &[&str] = &["window", "fenetre", "vitr", "glaz"];
/// Words that say a door has two leaves.
const DOUBLE_WORDS: &[&str] = &["double", "dbl"];
/// The frozen layer the converted blocks are set aside on; blocks on it are not proposed again.
pub const OLD_OPENING_LAYER: &str = "A-OPENING-OLD";
/// A width outside these, mm, is not a door or a window.
const WIDTH_RANGE_MM: (f64, f64) = (300.0, 5000.0);
/// Most block references looked at.
const MAX_CANDIDATES: usize = 20_000;

/// A block reference that is probably a door or a window on a wall.
#[derive(Clone, Debug, PartialEq)]
pub struct OpeningCandidate {
    /// The block reference to replace.
    pub block_ref: Handle,
    pub kind: OpeningKind,
    /// The width the block takes along the wall, rounded to 10 mm.
    pub width_mm: f64,
    /// Where its centre is, drawing coordinates.
    pub centre: Vec2,
    /// The wall it is in, the segment of its centre line and the distance of its centre from that
    /// segment's start (drawing units).
    pub wall: Handle,
    pub segment: usize,
    pub along: f64,
}

/// `text` in lower case with the French accents taken off, to compare names.
fn plain(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' => 'e',
            'à' | 'â' | 'À' => 'a',
            'ô' | 'ö' => 'o',
            'î' | 'ï' => 'i',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

fn names_one_of(text: &str, words: &[String]) -> bool {
    let text = plain(text);
    words.iter().any(|w| !w.is_empty() && text.contains(w.as_str()))
}

impl Drawing {
    /// The door and window block references of model space (those in `only` when it is given) that
    /// lie on a wall: named like one by `doors` / `windows` (the defaults when empty) in their block
    /// or their layer, a plausible width along the wall, and the insertion point within the wall's thickness.
    /// A reference that is already an opening or a wall is left out.
    pub fn recognise_openings(&self, only: Option<&[Handle]>, doors: &[String], windows: &[String]) -> Vec<OpeningCandidate> {
        let words = |given: &[String], default: &[&str]| -> Vec<String> {
            if given.is_empty() { default.iter().map(|w| (*w).to_string()).collect() } else { given.iter().map(|w| plain(w.trim())).collect() }
        };
        let (doors, windows) = (words(doors, DOOR_WORDS), words(windows, WINDOW_WORDS));
        let double: Vec<String> = DOUBLE_WORDS.iter().map(|w| (*w).to_string()).collect();
        let mut out = Vec::new();
        for e in self.model.iter().take(MAX_CANDIDATES * 4) {
            let EntityKind::Insert(i) = &e.kind else { continue };
            if only.is_some_and(|o| !o.contains(&e.handle))
                || opening_of(e).is_some()
                || wall_of(e).is_some()
                || e.common.layer.eq_ignore_ascii_case(OLD_OPENING_LAYER)
            {
                continue;
            }
            let label = format!("{} {}", i.block, e.common.layer);
            let kind = if names_one_of(&label, &windows) {
                OpeningKind::Window
            } else if names_one_of(&label, &doors) {
                if names_one_of(&label, &double) { OpeningKind::DoubleDoor } else { OpeningKind::Door }
            } else {
                continue;
            };
            let Some(block) = self.block(&i.block) else { continue };
            // The block at its own size, unturned: its box, and the box's centre as the reference places it.
            let mut flat = e.as_ref().clone();
            if let EntityKind::Insert(f) = &mut flat.kind {
                (f.insert, f.rotation, f.cols, f.rows) = (cadcraft_geom::Vec3::ZERO, 0.0, 1, 1);
            }
            let local = entity_bounds(self, &flat, 0);
            if !(local.min.is_finite() && local.max.is_finite()) || block.entities.is_empty() {
                continue;
            }
            let (w, h) = (local.max.x - local.min.x, local.max.y - local.min.y);
            let centre = Vec2::new(i.insert.x, i.insert.y) + ((local.min + local.max) / 2.0).rotate(i.rotation);
            // A door block is drawn with its swing, so its box's centre is beside the wall: the wall
            // is the one the box's centre is nearest, and the block must be in it by its insertion point.
            let Some(hit) = self.wall_under(centre, w.max(h)) else { continue };
            let Some(at_origin) = self.wall_under(Vec2::new(i.insert.x, i.insert.y), 1e-6) else { continue };
            if at_origin.wall != hit.wall {
                continue;
            }
            // Along the wall, the box's width: its sides turned to the wall's direction.
            let phi = i.rotation - hit.angle;
            let along_wall = w * phi.cos().abs() + h * phi.sin().abs();
            // The wall's units give the millimetres of a drawing unit.
            let Some(params) = self.entity(hit.wall).and_then(|x| wall_of(x)) else { continue };
            let per_unit = params.units.mm_per_unit;
            let width_mm = (along_wall * per_unit / 10.0).round() * 10.0;
            if !(WIDTH_RANGE_MM.0..=WIDTH_RANGE_MM.1).contains(&width_mm) {
                continue;
            }
            out.push(OpeningCandidate { block_ref: e.handle, kind, width_mm, centre, wall: hit.wall, segment: hit.segment, along: hit.along });
            if out.len() >= MAX_CANDIDATES {
                break;
            }
        }
        out
    }
}
