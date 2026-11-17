//! A Paste event that is not a Ctrl+V (empty, or the primary selection a middle click pastes on Linux) must not cancel the command being drawn: it
//! used to start PASTECLIP, which cancelled LINE and then refused ("the clipboard is empty").

use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Modifiers, PointerButton, pos2, vec2};
use egui_kittest::Harness;

fn line_with_a_first_point() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.start("line");
    app.session.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

/// LINE is still the running command, and nothing was cancelled.
fn still_drawing(h: &Harness<'static, CadApp>) {
    let log = &h.state().session.log;
    assert_eq!(h.state().session.running.as_ref().map(|r| r.id.as_str()), Some("line"), "{:?}", log.iter().rev().take(5).collect::<Vec<_>>());
    assert!(!log.iter().any(|l| l.contains("*Cancel*") || l.contains("pasteclip")), "{:?}", log.iter().rev().take(5).collect::<Vec<_>>());
}

#[test]
fn an_empty_paste_does_not_cancel_the_command_being_drawn() {
    let mut h = line_with_a_first_point();
    h.event(Event::Paste(String::new()));
    h.run_steps(3);
    still_drawing(&h);
}

#[test]
fn a_paste_in_the_frame_of_a_middle_click_does_not_cancel_the_command_being_drawn() {
    let mut h = line_with_a_first_point();
    let at = pos2(800.0, 500.0);
    let button = |pressed| Event::PointerButton { pos: at, button: PointerButton::Middle, pressed, modifiers: Modifiers::NONE };
    // The primary selection arrives as a Paste with the button going down, and again as it goes up (the harness gives each event a frame of its
    // own, as a browser may).
    for pressed in [true, false] {
        h.event(Event::PointerMoved(at));
        h.event(button(pressed));
        h.event(Event::Paste("some text highlighted elsewhere".into()));
        h.run_steps(3);
        still_drawing(&h);
    }
}
