//! The system clipboard: Cut/Copy/Paste key presses, and the drawing clipboard mirrored as DXF
//! text so it can be pasted into another CADCraft window.
//!
//! egui turns Cmd/Ctrl+C, X and V into `Copy`, `Cut` and `Paste(text)` events instead of key
//! presses, so the clipboard shortcuts are read from those. A `Paste` arrives only when the system
//! clipboard holds text; every copy therefore puts its DXF text there.

use std::hash::{DefaultHasher, Hash, Hasher};

use cadcraft_engine::cmd::clipboard;

use crate::CadApp;

/// egui memory: the clipboard copy last mirrored, and the hash of the text it became.
fn serial_id() -> egui::Id {
    egui::Id::unique("cc_clipboard_serial")
}
fn text_id() -> egui::Id {
    egui::Id::unique("cc_clipboard_text")
}

/// egui memory: the time of the last frame with a middle button press, hold or release.
fn middle_id() -> egui::Id {
    egui::Id::unique("cc_middle_click_time")
}

/// A Paste this soon (s) after a middle click is that click's, not a Ctrl+V: the browser reports the paste a frame or two after the button.
const MIDDLE_PASTE_S: f64 = 0.5;

/// Line endings normalised (pasted text arrives with `\n`), so our own text is recognised.
fn hash(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.replace("\r\n", "\n").hash(&mut h);
    h.finish()
}

/// Put a new copy (COPYCLIP, CUTCLIP, COPYBASE from anywhere) on the system clipboard, once.
pub fn mirror(app: &CadApp, ctx: &egui::Context) {
    let Some(src) = &app.session.clipboard_source else { return };
    if src.uid.is_none() || ctx.data(|d| d.get_temp::<u64>(serial_id())) == Some(src.serial) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(serial_id(), src.serial));
    if let Some(text) = clipboard::system_text(&app.session) {
        ctx.data_mut(|d| d.insert_temp(text_id(), hash(&text)));
        ctx.copy_text(text);
    }
}

/// The commands the Cut/Copy/Paste events of this frame ask for. Pasted DXF text that isn't our
/// own last copy (another CADCraft's) becomes the clipboard first.
pub fn shortcut_commands(app: &mut CadApp, ctx: &egui::Context) -> Vec<&'static str> {
    let (events, shift, middle, now) = ctx.input(|i| {
        let ev: Vec<egui::Event> =
            i.events.iter().filter(|e| matches!(e, egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_))).cloned().collect();
        // A middle click that is pressed, held or let go in this frame (see the Paste branch below, which also ignores the frames just after it).
        let middle = i.pointer.button_down(egui::PointerButton::Middle) || i.pointer.button_released(egui::PointerButton::Middle);
        (ev, i.modifiers.shift, middle, i.time)
    });
    if middle {
        ctx.data_mut(|d| d.insert_temp(middle_id(), now));
    }
    let after_middle = ctx.data(|d| d.get_temp::<f64>(middle_id())).is_some_and(|t| now - t < MIDDLE_PASTE_S);
    let mut out = Vec::new();
    for e in events {
        match e {
            egui::Event::Copy => out.push(if shift { "copybase" } else { "copyclip" }),
            egui::Event::Cut => out.push("cutclip"),
            egui::Event::Paste(text) => {
                // Not a paste of ours: on Linux a middle click pastes the primary selection (whatever text was last
                // highlighted anywhere) into the page, and the browser reports it as a Paste. `pasteclip` would
                // cancel the command being drawn and then refuse, the clipboard being empty. Ctrl+V is never a
                // middle click, so a real paste is not affected.
                if text.is_empty() || after_middle {
                    continue;
                }
                let h = hash(&text);
                if ctx.data(|d| d.get_temp::<u64>(text_id())) != Some(h) && clipboard::load_system_text(&mut app.session, &text) {
                    ctx.data_mut(|d| d.insert_temp(text_id(), h));
                }
                out.push(if shift { "pasteblock" } else { "pasteclip" });
            }
            _ => {}
        }
    }
    out
}
