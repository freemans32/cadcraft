//! The wall styles CADCraft offers in every drawing: common European constructions, measured in
//! millimetres from the exterior face to the interior face. These are typical build-ups to
//! start from, not thermal or structural recommendations; a drawing copies the ones it uses and
//! can change its copies.
//!
//! Hatches use CADCraft's own library patterns ([`crate::library`]).

use crate::wall::{ComponentFunction, WallComponent, WallStyle};

use ComponentFunction::{AirGap, Finish, Insulation, Membrane, Structure};

fn c(name: &str, thickness: f64, function: ComponentFunction) -> WallComponent {
    WallComponent::new(name, thickness, function)
}

fn style(name: &str, description: &str, components: Vec<WallComponent>) -> WallStyle {
    WallStyle { name: name.into(), description: description.into(), components, default_height: 2500.0 }
}

/// Plasterboard (12.5 mm), wrapping into openings.
fn plasterboard() -> WallComponent {
    c("Plasterboard", 12.5, Finish).returning()
}

/// The standard wall styles, in the order the style list shows them.
/// The core of a partition: weaker than any finish, so a partition stops on the face of the
/// wall it meets and only its plasterboard merges with that wall's.
fn partition_core(c: WallComponent) -> WallComponent {
    WallComponent { priority: 810, ..c }
}

pub fn standard_wall_styles() -> Vec<WallStyle> {
    let mut all = vec![
        style("Concrete 200", "Cast in-place concrete wall", vec![c("Concrete", 200.0, Structure).hatched("AR-CONC", 1.0)]),
        style(
            "Brick 200 + mineral wool 120 + plasterboard",
            "Hollow clay brick with an inside lining of mineral wool and plasterboard on metal studs",
            vec![
                c("Clay brick", 200.0, Structure).hatched("ANSI31", 6.0),
                c("Mineral wool", 120.0, Insulation).hatched("ZIGZAG", 8.0),
                plasterboard(),
            ],
        ),
        style(
            "Concrete block 200 + EPS 100 + plasterboard",
            "Rendered concrete block with a bonded lining of expanded polystyrene and plasterboard",
            vec![
                c("Render", 15.0, Finish).hatched("AR-SAND", 1.0),
                c("Concrete block", 200.0, Structure).hatched("ANSI37", 8.0),
                c("EPS insulation", 100.0, Insulation).hatched("DOTS", 8.0),
                plasterboard(),
            ],
        ),
        style(
            "Timber frame 145",
            "Timber frame: cladding on battens, OSB, insulated studs, service void and plasterboard",
            vec![
                c("Timber cladding", 22.0, Finish).hatched("ANSI32", 1.0),
                c("Ventilated air gap", 20.0, AirGap),
                c("Breather membrane", 0.0, Membrane),
                c("OSB", 12.0, Structure).hatched("ANSI33", 0.5),
                c("Studs 145 + mineral wool", 145.0, Insulation).hatched("ZIGZAG", 8.0),
                c("Vapour control layer", 0.0, Membrane),
                c("Service void", 45.0, AirGap),
                plasterboard(),
            ],
        ),
        style(
            "Partition 72/48",
            "Plasterboard partition on 48 mm metal studs with mineral wool",
            vec![plasterboard(), partition_core(c("Studs 48 + mineral wool", 48.0, Insulation).hatched("ZIGZAG", 4.0)), plasterboard()],
        ),
        style(
            "Partition 98/48",
            "Acoustic partition: two plasterboards each side of 48 mm metal studs",
            vec![plasterboard(), plasterboard(), partition_core(c("Studs 48", 48.0, AirGap)), plasterboard(), plasterboard()],
        ),
    ];
    all.extend(masonry_wall_styles());
    all
}

/// A one-component wall of `thickness` mm.
fn solid(name: &str, description: &str, thickness: f64, pattern: &str, scale: f64) -> WallStyle {
    style(name, description, vec![c(name, thickness, Structure).hatched(pattern, scale)])
}

/// A render coat of 15 mm.
fn render() -> WallComponent {
    c("Render", 15.0, Finish).hatched("AR-SAND", 1.0)
}

/// Common masonry and partition build-ups, in the order the Architecture tab lists them.
fn masonry_wall_styles() -> Vec<WallStyle> {
    let lined = |name: &str, desc: &str, mut layers: Vec<WallComponent>, insulation: WallComponent| {
        layers.push(insulation);
        layers.push(plasterboard());
        style(name, desc, layers)
    };
    let wool = || c("Mineral wool", 100.0, Insulation).hatched("ZIGZAG", 8.0);
    let eps = || c("EPS insulation", 80.0, Insulation).hatched("DOTS", 8.0);
    vec![
        lined(
            "Rendered masonry + lining",
            "Rendered concrete block with a mineral wool lining and plasterboard",
            vec![render(), c("Concrete block", 200.0, Structure).hatched("ANSI37", 8.0)],
            wool(),
        ),
        lined(
            "Rendered concrete + lining",
            "Rendered cast concrete with an EPS lining and plasterboard",
            vec![render(), c("Concrete", 180.0, Structure).hatched("AR-CONC", 1.0)],
            eps(),
        ),
        lined(
            "Rendered brick + lining",
            "Rendered clay brick with a mineral wool lining and plasterboard",
            vec![render(), c("Clay brick", 200.0, Structure).hatched("ANSI31", 6.0)],
            wool(),
        ),
        lined(
            "Facing brick + concrete + lining",
            "Facing brick on cast concrete with an EPS lining and plasterboard",
            vec![c("Facing brick", 100.0, Finish).hatched("BRICK", 12.0), c("Concrete", 160.0, Structure).hatched("AR-CONC", 1.0)],
            eps(),
        ),
        lined(
            "Facing brick + masonry + lining",
            "Facing brick, air gap, concrete block, mineral wool and plasterboard",
            vec![
                c("Facing brick", 100.0, Finish).hatched("BRICK", 12.0),
                c("Ventilated air gap", 20.0, AirGap),
                c("Concrete block", 200.0, Structure).hatched("ANSI37", 8.0),
            ],
            wool(),
        ),
        lined(
            "Rubble stone + air gap + lining",
            "Rough stone with an air gap, mineral wool and plasterboard",
            vec![c("Rubble stone", 400.0, Structure).hatched("EARTH", 8.0), c("Air gap", 30.0, AirGap)],
            wool(),
        ),
        lined(
            "Cut stone + air gap + lining",
            "Dressed stone with an air gap, mineral wool and plasterboard",
            vec![c("Cut stone", 300.0, Structure).hatched("NET", 8.0), c("Air gap", 30.0, AirGap)],
            wool(),
        ),
        solid("Facing brick 220", "Solid facing brick wall", 220.0, "BRICK", 12.0),
        solid("Shear wall 180", "Cast concrete shear wall", 180.0, "AR-CONC", 1.0),
        WallStyle {
            default_height: 300.0,
            ..solid("Foundation 550 x 300", "Concrete strip foundation, 550 mm wide and 300 mm high", 550.0, "AR-CONC", 1.0)
        },
        style(
            "Partition 100/70",
            "Plasterboard partition on 70 mm metal studs with mineral wool",
            vec![plasterboard(), partition_core(c("Studs 70 + mineral wool", 70.0, Insulation).hatched("ZIGZAG", 4.0)), plasterboard()],
        ),
        solid("Plaster block 50", "Plaster block partition, 50 mm", 50.0, "ANSI34", 8.0),
        solid("Plaster block 70", "Plaster block partition, 70 mm", 70.0, "ANSI34", 8.0),
        solid("Plaster block 100", "Plaster block partition, 100 mm", 100.0, "ANSI34", 8.0),
        solid("Concrete block 100", "Hollow concrete block partition, 100 mm", 100.0, "ANSI37", 8.0),
        solid("Concrete block 150", "Hollow concrete block wall, 150 mm", 150.0, "ANSI37", 8.0),
        solid("Concrete block 200 wall", "Hollow concrete block wall, 200 mm", 200.0, "ANSI37", 8.0),
    ]
}
