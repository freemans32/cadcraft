//! The texts of the objects drawn by CadCraft (units, terminals, accessories, equipment, devices…) are kept readable: a text that a turned object
//! would show upside down is turned half a turn, so every label reads left to right or from the bottom up, never upside down.

use super::duct::reads_upside_down;
use crate::{Drawing, EntityKind, HAlign, Handle, Space};

impl Drawing {
    /// The objects of `space` added or changed since `before` (an older state of this drawing) that show a text upside down.
    pub fn upside_down_labels_since(&self, before: &Drawing, space: &Space) -> Vec<Handle> {
        let (Some(now), Some(then)) = (self.space(space), before.space(space)) else { return Vec::new() };
        now.iter()
            .filter(|e| then.get(e.handle).is_none_or(|old| !std::sync::Arc::ptr_eq(old, e)) && self.shows_upside_down(e))
            .map(|e| e.handle)
            .collect()
    }

    /// Whether object `e`, drawn by CadCraft and not mirrored (a mirrored one turns its texts the other way: it is left as it is), shows a text
    /// upside down.
    fn shows_upside_down(&self, e: &crate::Entity) -> bool {
        let EntityKind::Insert(i) = &e.kind else { return false };
        is_ours(e)
            && i.scale.x * i.scale.y > 0.0
            && self.block(&i.block).is_some_and(|b| {
                b.anonymous && b.entities.iter().any(|t| matches!(&t.kind, EntityKind::Text(t) if reads_upside_down(i.rotation + t.rotation)))
            })
    }

    /// Turns upright the texts of object `h`, in a block of its own; whether any was turned.
    pub fn upright_labels(&mut self, h: Handle) -> bool {
        let Some(e) = self.entity(h).filter(|e| self.shows_upside_down(e)) else { return false };
        let EntityKind::Insert(i) = &e.kind else { return false };
        let (turn, old) = (i.rotation, i.block.clone());
        let Some(block) = self.block(&old).cloned() else { return false };
        // A copy of the block, which the object may share with a copy of it turned another way.
        let name = self.anonymous_block_name("*U");
        let mut b = (*block).clone();
        b.name = name.clone();
        b.entities = crate::EntityStore::new();
        for e in block.entities.iter() {
            let mut e = (**e).clone();
            e.handle = self.new_handle();
            if let EntityKind::Text(t) = &mut e.kind
                && reads_upside_down(turn + t.rotation)
            {
                t.rotation = (t.rotation + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU);
                // Turned about where it is written, a text that started there ends there, on the same side of the point.
                t.halign = match t.halign {
                    HAlign::Left => HAlign::Right,
                    HAlign::Right => HAlign::Left,
                    other => other,
                };
                t.align_pt.get_or_insert(t.insert);
            }
            b.entities.push(e);
        }
        self.blocks.insert(name.clone(), std::sync::Arc::new(b));
        let name_kept = name.clone();
        if self
            .modify_entity(h, |e| {
                if let EntityKind::Insert(i) = &mut e.kind {
                    i.block = name;
                }
            })
            .is_err()
        {
            return false;
        }
        self.drop_unreferenced_hvac_block(&old);
        self.label_block_renamed(h, &old, &name_kept);
        true
    }
}

/// An object drawn by CadCraft: a reference to an anonymous block that carries CadCraft's data.
pub(super) fn is_ours(e: &crate::Entity) -> bool {
    matches!(&e.kind, EntityKind::Insert(i) if i.block.starts_with("*U"))
        && e.common.xdata.iter().any(|t| t.code == 1001 && t.value.trim().to_ascii_uppercase().starts_with("CADCRAFT_"))
}
