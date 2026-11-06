//! WALLADD draws walls of a wall style (a block reference to the generated lines and hatches,
//! carrying the wall's data); WALLSTYLEEDIT lists, creates, changes, renames and deletes wall
//! styles, regenerating the walls whose block is still their own.

use std::sync::Arc;

use cadcraft_doc::{Drawing, Entity, EntityKind, Handle, Line, WALL_LAYER, WallJustification, WallParams, wall_of};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::{Value, json};

const BRICK: &str = "Brick 200 + mineral wool 120 + plasterboard";
const PARTITION: &str = "Partition 72/48";

fn metric() -> Session {
    let mut s = Session::new();
    *s.doc_mut().unwrap() = Drawing::new_metric();
    s
}

fn h(v: &Value) -> Handle {
    Handle::parse_hex(v["handle"].as_str().unwrap()).unwrap()
}

fn wall(s: &Session, hd: Handle) -> WallParams {
    wall_of(s.doc().unwrap().entity(hd).expect("wall object")).expect("wall data")
}

fn walls(s: &Session) -> Vec<(Handle, WallParams)> {
    s.doc().unwrap().model.iter().filter_map(|e| wall_of(e).map(|w| (e.handle, w))).collect()
}

fn path(w: &WallParams) -> Vec<(f64, f64)> {
    w.path.iter().map(|p| (p.x, p.y)).collect()
}

#[test]
fn walladd_draws_a_wall_of_a_standard_style_and_copies_the_style_into_the_drawing() {
    let mut s = metric();
    let r = s.execute("walladd", &json!({"points": [[0, 0], [5000, 0], [5000, -4000]], "style": BRICK})).unwrap();
    let hd = h(&r);
    let d = s.doc().unwrap();
    let e = d.entity(hd).unwrap();
    assert_eq!(e.common.layer, WALL_LAYER);
    let EntityKind::Insert(i) = &e.kind else { panic!("not a block reference") };
    assert_eq!(i.insert, Vec3::ZERO);
    let w = wall(&s, hd);
    assert_eq!((w.style.name.as_str(), w.justification, w.height), (BRICK, WallJustification::Left, 2500.0));
    assert_eq!(path(&w), [(0.0, 0.0), (5000.0, 0.0), (5000.0, -4000.0)]);
    assert!(d.wall_style(BRICK).is_some(), "the standard style is now the drawing's");
    assert!(d.wall_is_intact(hd));
    // 4 boundary lines, 2 end lines, 2 hatches.
    assert_eq!(d.block(&i.block).unwrap().entities.len(), 8);
}

#[test]
fn walladd_refuses_what_it_cannot_draw() {
    let mut s = metric();
    let bad = |s: &mut Session, p: Value| s.execute("walladd", &p).is_err();
    assert!(bad(&mut s, json!({})));
    assert!(bad(&mut s, json!({"points": [[0, 0]]})));
    assert!(bad(&mut s, json!({"points": "0,0 1,1"})));
    assert!(bad(&mut s, json!({"points": [[0, 0], [1000, 0], [0, 100]]})), "corner too sharp");
    assert!(bad(&mut s, json!({"points": [[0, 0], [1000, 0], [1000, 10], [2000, 10]]})), "segment too short");
    assert!(bad(&mut s, json!({"points": [[0, 0], [1000, 0]], "style": "nope"})));
    assert!(bad(&mut s, json!({"points": [[0, 0], [1000, 0]], "justification": "up"})));
    assert!(bad(&mut s, json!({"points": [[0, 0], [1000, 0]], "height": -1})));
    let many: Vec<Value> = (0..=64).map(|i| json!([i * 1000, (i % 2) * 1000])).collect();
    assert!(bad(&mut s, json!({ "points": many })));
    assert!(walls(&s).is_empty());
    assert!(s.doc().unwrap().wall_styles.is_empty(), "no style copied for a wall that wasn't drawn");
}

#[test]
fn walladd_at_the_command_line_takes_style_justification_and_points() {
    let mut s = metric();
    for t in ["walladd", "st", PARTITION, "j", "c", "0,0", "3000,0", "3000,2000", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let all = walls(&s);
    assert_eq!(all.len(), 1);
    let w = &all[0].1;
    assert_eq!((w.style.name.as_str(), w.justification), (PARTITION, WallJustification::Center));
    // The next wall takes the same settings, through the French alias.
    for t in ["mur", "0,5000", "2000,5000", ""] {
        s.cmdline(t).unwrap();
    }
    let all = walls(&s);
    assert_eq!(all.len(), 2);
    assert_eq!((all[1].1.style.name.as_str(), all[1].1.justification), (PARTITION, WallJustification::Center));
}

#[test]
fn walladd_skips_a_point_that_would_make_the_wall_impossible() {
    let mut s = metric();
    for t in ["walladd", "0,0", "1000,0", "0,100", "1000,1000", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.log.iter().any(|l| l.contains("turn this sharply")), "{:?}", s.log);
    let all = walls(&s);
    assert_eq!(all.len(), 1);
    assert_eq!(path(&all[0].1), [(0.0, 0.0), (1000.0, 0.0), (1000.0, 1000.0)]);
}

#[test]
fn walladd_close_draws_a_closed_wall() {
    let mut s = metric();
    for t in ["walladd", "0,0", "4000,0", "4000,-3000", "0,-3000", "c"] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let all = walls(&s);
    assert_eq!(all.len(), 1);
    assert!(all[0].1.closed);
    assert_eq!(all[0].1.path.len(), 4);
}

#[test]
fn walladd_is_undone_in_one_step() {
    let mut s = metric();
    s.execute("walladd", &json!({"points": [[0, 0], [3000, 0]]})).unwrap();
    assert_eq!(walls(&s).len(), 1);
    s.undo().unwrap();
    assert!(walls(&s).is_empty());
}

#[test]
fn wallstyleedit_lists_the_drawing_s_styles_then_the_standard_ones() {
    let mut s = metric();
    let r = s.execute("wallstyleedit", &json!({})).unwrap();
    let styles = r["styles"].as_array().unwrap();
    assert_eq!(styles.len(), 6);
    assert!(styles.iter().all(|st| st["standard"] == json!(true)));
    assert_eq!(styles[1]["thickness"], json!(332.5));
    s.execute("wallstyleedit", &json!({"name": "Mine", "copyFrom": "Concrete 200", "description": "A copy"})).unwrap();
    let r = s.execute("wallstyleedit", &json!({})).unwrap();
    let names: Vec<&str> = r["styles"].as_array().unwrap().iter().map(|st| st["name"].as_str().unwrap()).collect();
    assert_eq!(names.first(), Some(&"Mine"));
    assert_eq!(names.len(), 7);
}

#[test]
fn changing_a_style_regenerates_its_walls_but_not_one_edited_elsewhere() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [3000, 0]], "style": BRICK})).unwrap());
    let b = h(&s.execute("walladd", &json!({"points": [[0, 5000], [3000, 5000]], "style": BRICK})).unwrap());
    // Wall b's block is edited, as another program might.
    let d = s.doc_mut().unwrap();
    let EntityKind::Insert(i) = &d.entity(b).unwrap().kind else { panic!() };
    let name = i.block.clone();
    let mut blk = d.block(&name).unwrap().as_ref().clone();
    blk.entities.push(Entity::new(Handle(0x9999), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })));
    d.blocks.insert(name, Arc::new(blk));
    let r = s
        .execute(
            "wallstyleedit",
            &json!({"name": BRICK, "components": [
                {"name": "Clay brick", "thickness": 200, "pattern": "ANSI31", "patternScale": 6},
                {"name": "Mineral wool", "thickness": 140, "pattern": "ZIGZAG", "patternScale": 8, "function": "insulation"},
                {"name": "Plasterboard", "thickness": 12.5, "function": "finish", "returnsAtOpenings": true}
            ]}),
        )
        .unwrap();
    assert_eq!((r["regenerated"].as_u64(), r["kept"].as_u64()), (Some(1), Some(1)));
    assert_eq!(wall(&s, a).style.thickness(), 352.5);
    assert_eq!(wall(&s, b).style.thickness(), 332.5, "the edited wall keeps its old style copy");
    assert!(s.doc().unwrap().wall_is_intact(a));
    s.undo().unwrap();
    assert_eq!(wall(&s, a).style.thickness(), 332.5, "one undo restores the style and its walls");
}

#[test]
fn renaming_a_style_renames_it_on_its_walls_and_deleting_it_keeps_the_walls() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [3000, 0]], "style": PARTITION})).unwrap());
    s.execute("wallstyleedit", &json!({"name": PARTITION, "rename": "Cloison 72/48"})).unwrap();
    assert_eq!(wall(&s, a).style.name, "Cloison 72/48");
    assert!(s.doc().unwrap().wall_style("Cloison 72/48").is_some());
    assert!(s.execute("wallstyleedit", &json!({"name": "Cloison 72/48", "rename": " "})).is_err());
    s.execute("wallstyleedit", &json!({"delete": "Cloison 72/48"})).unwrap();
    assert!(s.doc().unwrap().wall_style("Cloison 72/48").is_none());
    assert_eq!(wall(&s, a).style.name, "Cloison 72/48", "the wall keeps its own copy");
    assert!(s.execute("wallstyleedit", &json!({"delete": "Concrete 200"})).is_err(), "standard styles can't be deleted");
}

#[test]
fn wallstyleedit_refuses_unusable_styles() {
    let mut s = metric();
    let bad = |s: &mut Session, p: Value| s.execute("wallstyleedit", &p).is_err();
    let c = |t: f64| json!({"name": "X", "thickness": t});
    assert!(bad(&mut s, json!({"name": "Nope"})), "neither components nor copyFrom");
    assert!(bad(&mut s, json!({"name": "New", "components": []})));
    assert!(bad(&mut s, json!({"name": "New", "components": vec![c(10.0); 17]})));
    assert!(bad(&mut s, json!({"name": "New", "components": [c(-5.0)]})));
    assert!(bad(&mut s, json!({"name": "New", "components": [{"name": "X", "thickness": 10, "function": "magic"}]})));
    assert!(bad(&mut s, json!({"name": "New", "components": "brick"})));
    assert!(bad(&mut s, json!({"name": "New", "copyFrom": "Concrete 200", "defaultHeight": 0})));
    assert!(bad(&mut s, json!({"name": "x".repeat(65), "copyFrom": "Concrete 200"})));
    assert!(s.doc().unwrap().wall_styles.is_empty());
    s.execute("wallstyleedit", &json!({"name": "New", "components": [c(150.0)]})).unwrap();
    assert_eq!(s.doc().unwrap().wall_style("new").map(|st| st.thickness()), Some(150.0));
}

#[test]
fn a_moved_or_copied_wall_stays_a_wall() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [3000, 0]]})).unwrap());
    s.execute("move", &json!({"handles": [a.hex()], "from": [0, 0], "to": [1000, 1000]})).unwrap();
    assert!(s.doc().unwrap().wall_is_intact(a));
    s.execute("copy", &json!({"handles": [a.hex()], "from": [0, 0], "to": [0, 2000]})).unwrap();
    let all = walls(&s);
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|(hd, _)| s.doc().unwrap().wall_is_intact(*hd)));
}

#[test]
fn extreme_wall_values_are_refused_without_drawing_anything() {
    let mut s = metric();
    let long = "x".repeat(100_000);
    for p in [
        json!({"points": [[0, 0], [1e300, 0]]}),
        json!({"points": [[0, 0], [1e-300, 0]]}),
        json!({"points": [[0, 0], [1e6, 0]], "height": 1e308}),
        json!({"points": [[0, 0], [1000, 0]], "style": long}),
    ] {
        assert!(s.execute("walladd", &p).is_err(), "{p}");
    }
    for c in [
        json!({"name": "X", "thickness": 1e308}),
        json!({"name": "X", "thickness": 10, "patternScale": 1e308}),
        json!({"name": "X", "thickness": 10, "priority": 70000}),
        json!({"name": long, "thickness": 10}),
    ] {
        assert!(s.execute("wallstyleedit", &json!({"name": "N", "components": [c]})).is_err(), "{c}");
    }
    assert!(walls(&s).is_empty());
    assert!(s.doc().unwrap().wall_styles.is_empty());
}
