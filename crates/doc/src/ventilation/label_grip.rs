//! The labels of the objects drawn by CadCraft (ducts, pipes, cables, units, terminals, accessories, equipment, devices…) can be moved by a grip of their
//! own. How far a label was moved is kept with the object, in the block's coordinates; each time the object is drawn again (a change, sizing), its texts
//! are moved by that much again, so the label stays where it was put and goes with the object when it moves or turns.

use cadcraft_geom::Vec2;
use serde::{Deserialize, Serialize};

use super::{space_of, v3, xdata};
use crate::{Drawing, EntityKind, Handle, Result, Space};

const LABEL_APP: &str = "CADCRAFT_LABELPOS";
const LABEL_FORMAT: &str = "LABELPOS1";

/// How far the label of an object was moved, and the block it was moved in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct LabelMove {
    /// In the block's coordinates (drawing units).
    dx: f64,
    dy: f64,
    /// The block whose texts are already moved: another one (the object was drawn again) is moved on.
    block: String,
}

fn label_move(e: &crate::Entity) -> Option<LabelMove> {
    xdata::from_xdata(LABEL_APP, LABEL_FORMAT, &e.common.xdata).filter(|m: &LabelMove| m.dx.is_finite() && m.dy.is_finite())
}

/// The texts of a block moved by `d`, in a copy of it named `name` (the block may be shared with a copy of the object).
fn moved_block(d: &mut Drawing, old: &str, delta: Vec2) -> Option<String> {
    let block = d.block(old)?.clone();
    let name = d.anonymous_block_name("*U");
    let mut b = (*block).clone();
    b.name = name.clone();
    b.entities = crate::EntityStore::new();
    for e in block.entities.iter() {
        let mut e = (**e).clone();
        e.handle = d.new_handle();
        if let EntityKind::Text(t) = &mut e.kind {
            t.insert = v3(t.insert.xy() + delta);
            if let Some(a) = &mut t.align_pt {
                *a = v3(a.xy() + delta);
            }
        }
        b.entities.push(e);
    }
    d.blocks.insert(name.clone(), std::sync::Arc::new(b));
    Some(name)
}

impl Drawing {
    /// Where the label grip of object `e` is (the middle of its texts), when it is an object drawn by CadCraft with a label; a space has its own.
    pub fn label_grip(&self, e: &crate::Entity) -> Option<Vec2> {
        let EntityKind::Insert(i) = &e.kind else { return None };
        if space_of(e).is_some() || !super::upright::is_ours(e) {
            return None;
        }
        let texts: Vec<Vec2> = self
            .block(&i.block)?
            .entities
            .iter()
            .filter_map(|x| match &x.kind {
                EntityKind::Text(t) => Some(t.align_pt.unwrap_or(t.insert).xy()),
                _ => None,
            })
            .collect();
        if texts.is_empty() {
            return None;
        }
        let mid = texts.iter().fold(Vec2::ZERO, |a, p| a + *p) * (1.0 / texts.len() as f64);
        Some(super::slab::block_to_world(i, mid))
    }

    /// Moves the label of object `h` so that its grip is at `to`. `Ok(false)` when `h` has no label grip.
    pub fn move_label(&mut self, h: Handle, to: Vec2) -> Result<bool> {
        let Some(e) = self.entity(h).cloned() else { return Ok(false) };
        let (Some(at), EntityKind::Insert(i)) = (self.label_grip(&e), &e.kind) else { return Ok(false) };
        if !to.is_finite() || i.scale.x == 0.0 || i.scale.y == 0.0 {
            return Ok(false);
        }
        // The move in the block's coordinates: the reference's turn and scale taken back.
        let (s, c) = i.rotation.sin_cos();
        let w = to - at;
        let local = Vec2::new((w.x * c + w.y * s) / i.scale.x, (-w.x * s + w.y * c) / i.scale.y);
        let Some(block) = moved_block(self, &i.block, local) else { return Ok(false) };
        let old = label_move(&e).map_or(Vec2::ZERO, |m| Vec2::new(m.dx, m.dy));
        let total = old + local;
        let tags = xdata::to_xdata(LABEL_APP, LABEL_FORMAT, &LabelMove { dx: total.x, dy: total.y, block: block.clone() })?;
        let old_block = i.block.clone();
        self.modify_entity(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                i.block = block;
            }
            e.common.xdata = xdata::replace(LABEL_APP, &e.common.xdata, tags);
        })?;
        self.drop_unreferenced_hvac_block(&old_block);
        Ok(true)
    }

    /// Whether object `e`'s label was moved by its grip (a duct's label is then left where it was put by the pass that keeps labels apart).
    pub fn label_was_moved(e: &crate::Entity) -> bool {
        label_move(e).is_some_and(|m| m.dx != 0.0 || m.dy != 0.0)
    }

    /// Whether an object of `space` added or changed since `before` has a moved label to move again (so the drawing is not copied for nothing).
    pub fn labels_to_move_since(&self, before: &Drawing, space: &Space) -> bool {
        let (Some(now), Some(then)) = (self.space(space), before.space(space)) else { return false };
        now.iter().any(|e| {
            then.get(e.handle).is_none_or(|old| !std::sync::Arc::ptr_eq(old, e))
                && matches!(&e.kind, EntityKind::Insert(i) if label_move(e).is_some_and(|m| m.block != i.block))
        })
    }

    /// Moves the object `h` (a block reference) so that its insertion point is at `to`. `Ok(false)` when it is not one.
    pub fn move_insert_to(&mut self, h: Handle, to: Vec2) -> Result<bool> {
        if !to.is_finite() || !matches!(self.entity(h).map(|e| &e.kind), Some(EntityKind::Insert(_))) {
            return Ok(false);
        }
        self.modify_entity(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                i.insert = v3(to);
            }
        })?;
        Ok(true)
    }

    /// Moves again the labels of the objects of `space` added or changed since `before` that were drawn again since their label was moved. Returns how
    /// many were.
    pub fn keep_moved_labels_since(&mut self, before: &Drawing, space: &Space) -> usize {
        let (Some(now), Some(then)) = (self.space(space), before.space(space)) else { return 0 };
        let todo: Vec<(Handle, LabelMove, String)> = now
            .iter()
            .filter(|e| then.get(e.handle).is_none_or(|old| !std::sync::Arc::ptr_eq(old, e)))
            .filter_map(|e| match &e.kind {
                EntityKind::Insert(i) => label_move(e).filter(|m| m.block != i.block).map(|m| (e.handle, m, i.block.clone())),
                _ => None,
            })
            .collect();
        let mut done = 0;
        for (h, m, old) in todo {
            let Some(block) = moved_block(self, &old, Vec2::new(m.dx, m.dy)) else { continue };
            let Ok(tags) = xdata::to_xdata(LABEL_APP, LABEL_FORMAT, &LabelMove { block: block.clone(), ..m }) else { continue };
            if self
                .modify_entity(h, |e| {
                    if let EntityKind::Insert(i) = &mut e.kind {
                        i.block = block;
                    }
                    e.common.xdata = xdata::replace(LABEL_APP, &e.common.xdata, tags);
                })
                .is_ok()
            {
                self.drop_unreferenced_hvac_block(&old);
                done += 1;
            }
        }
        done
    }

    /// Object `h` now refers to block `new` in place of `old`, with the same texts (turned upright): its label is already moved there.
    pub(super) fn label_block_renamed(&mut self, h: Handle, old: &str, new: &str) {
        let Some(m) = self.entity(h).and_then(|e| label_move(e)).filter(|m| m.block == old) else { return };
        let Ok(tags) = xdata::to_xdata(LABEL_APP, LABEL_FORMAT, &LabelMove { block: new.to_string(), ..m }) else { return };
        let _ = self.modify_entity(h, |e| e.common.xdata = xdata::replace(LABEL_APP, &e.common.xdata, tags));
    }
}

/// Whether `e` is an object drawn by CadCraft whose label has a grip (not a space, which has its own).
pub fn ventilation_labelled(e: &crate::Entity) -> bool {
    matches!(&e.kind, EntityKind::Insert(_)) && space_of(e).is_none() && super::upright::is_ours(e)
}
