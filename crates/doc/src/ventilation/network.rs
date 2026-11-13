//! The network of ducts drawn from an air handling unit, read back from where their ends meet:
//! nothing about it is stored, so moving or erasing a duct changes it at the next reading.

use cadcraft_geom::Vec2;
use cadcraft_hvac::{DesignCriteria, Fitting, Network, NetworkReport, Section, Segment, SegmentKind, Terminal, size_network};

use super::duct::{AXIS_REACH, interior_cut};
use super::{
    AccessoryParams, AhuParams, AirDirection, DuctElbow, DuctKind, DuctParams, DuctReducer, DuctShape, HvacUnits, Loss, RiserParams, RiserSize,
    TerminalParams, accessory_of, ahu_of, air_terminal_of, invalid, riser_of,
};
use crate::{EntityKind, Handle, Result};

/// Most ducts one drawing's networks are read from.
pub const MAX_DUCTS: usize = 5_000;
/// What is kept back from the height available, mm, unless a sizing says otherwise.
pub const DEFAULT_MARGIN_MM: f64 = 100.0;
/// Distance, in millimetres, within which two ends are one point.
const MEET_MM: f64 = 10.0;
/// Distance, in millimetres, within which a duct end is on an air handling unit: a click near its face.
const UNIT_MEET_MM: f64 = 100.0;
/// Turn, in degrees, from which a duct run needs a 45° elbow, and a 90° elbow.
const ELBOW_45_DEG: f64 = 10.0;
const ELBOW_90_DEG: f64 = 67.5;
/// Wall roughness of a flexible duct, mm: far above spiral steel's 0.15. An indicative value.
pub const FLEX_ROUGHNESS_MM: f64 = 3.0;

/// One air handling unit's ducts of one air direction as a network ready to size. A double-flux
/// unit has two: its supply ducts with the supply terminals, its extract ducts with the extract ones.
#[derive(Clone, Debug, PartialEq)]
pub struct DuctNetwork {
    pub ahu: Handle,
    pub air: AirDirection,
    pub network: Network,
    /// The duct of each segment of `network`, in the same order.
    pub ducts: Vec<Handle>,
    /// How far each duct turns from the one it continues, degrees (positive to the left of the air's
    /// way), when that is a bare elbow: no tee, no more than a bend.
    pub elbow_turns_deg: Vec<Option<f64>>,
    /// The altitude of each duct's underside, mm, where a height zone it runs in gives one.
    pub bottoms_mm: Vec<Option<f64>>,
    /// Whether each duct is entered by its far end (the air runs from its end to its origin).
    pub entered_at_end: Vec<bool>,
    /// Ducts connected to the unit that lead to no terminal, so are left out of the sizing.
    pub dangling: Vec<Handle>,
    /// Terminals that no duct of their air direction reaches (set on the first network only).
    pub unconnected: Vec<Handle>,
    /// The accessories whose loss is in this network.
    pub accessories: Vec<Handle>,
    /// Accessories on no duct (set on the first network only).
    pub unplaced: Vec<Handle>,
    /// Ducts no unit reaches (set on the first network only), and how many ducts the drawing has.
    pub isolated: Vec<Handle>,
    pub duct_count: usize,
}

struct Duct {
    handle: Handle,
    params: DuctParams,
    a: Vec2,
    b: Vec2,
}

/// A terminal found in the drawing: where it is and how it is turned.
struct TerminalAt {
    handle: Handle,
    params: TerminalParams,
    at: Vec2,
    rotation: f64,
}

impl TerminalAt {
    /// Whether a duct ending at `end` reaches this terminal: it is on its face, or within `reach`
    /// of it (a click snapped to the face's edge is not at the centre).
    fn is_reached(&self, end: Vec2, reach: f64) -> bool {
        let half = self.params.half();
        let local = (end - self.at).rotate(-self.rotation);
        let neck = self.params.units.to_units(self.params.model.neck_mm) / 2.0;
        local.x.abs() <= half.x.max(neck) + reach && local.y.abs() <= half.y.max(neck) + reach
    }
}

/// A riser found in the drawing: its shaft outline in drawing coordinates. On a floor plan it is where
/// the ducts start; on the roof plan the ducts of a unit end on it, with the flow typed on it.
struct RiserAt {
    handle: Handle,
    params: RiserParams,
    outline: Vec<Vec2>,
}

impl RiserAt {
    /// Whether `q` is in the shaft or within `reach` of its edge (a click lands on a line's edge).
    fn reaches(&self, q: Vec2, reach: f64) -> bool {
        cadcraft_geom::point_in_polygon(&self.outline, q)
            || self.outline.iter().zip(self.outline.iter().cycle().skip(1)).any(|(a, b)| {
                let e = *b - *a;
                let t = if e.len() > 0.0 { ((q - *a).dot(e) / e.dot(e)).clamp(0.0, 1.0) } else { 0.0 };
                (*a + e * t - q).len() <= reach
            })
    }
}

struct Node {
    duct: usize,
    parent: Option<usize>,
    /// The end the run is entered by, and the end it leaves by.
    from: Vec2,
    to: Vec2,
    terminal: Option<(Handle, f64, f64)>,
    /// The run ends in the open: no terminal, but the air leaves or comes in freely there.
    open: bool,
    fittings: Vec<Fitting>,
    terminals_below: usize,
}

impl crate::Drawing {
    /// The networks of every air handling unit in model space. Several units may not share a duct:
    /// a duct is read once, for the first unit that reaches it.
    pub fn duct_networks(&self) -> Result<Vec<DuctNetwork>> {
        self.duct_networks_with_margin(DEFAULT_MARGIN_MM)
    }

    /// [`Self::duct_networks`] keeping `margin_mm` back from the height available (for hangers,
    /// insulation and tolerances).
    pub fn duct_networks_with_margin(&self, margin_mm: f64) -> Result<Vec<DuctNetwork>> {
        let units = HvacUnits::of(self);
        let tol = units.to_units(MEET_MM);
        let mut ducts = Vec::new();
        let mut ahus: Vec<(Handle, AhuParams, Vec2, f64)> = Vec::new();
        let mut terminals: Vec<TerminalAt> = Vec::new();
        let mut accessories: Vec<(Handle, AccessoryParams, Vec2)> = Vec::new();
        let mut risers: Vec<RiserAt> = Vec::new();
        for e in self.model.iter() {
            let EntityKind::Insert(i) = &e.kind else { continue };
            if let Some((params, a, b)) = self.duct_ends(e.handle) {
                if ducts.len() >= MAX_DUCTS {
                    return Err(invalid(format!("The drawing has more than {MAX_DUCTS} ducts.")));
                }
                ducts.push(Duct { handle: e.handle, params, a, b });
            } else if let Some(p) = ahu_of(e) {
                ahus.push((e.handle, p, Vec2::new(i.insert.x, i.insert.y), i.rotation));
            } else if let Some(p) = air_terminal_of(e) {
                terminals.push(TerminalAt { handle: e.handle, params: p, at: Vec2::new(i.insert.x, i.insert.y), rotation: i.rotation });
            } else if let Some(p) = accessory_of(e) {
                accessories.push((e.handle, p, Vec2::new(i.insert.x, i.insert.y)));
            } else if let Some(p) = riser_of(e) {
                let (s, c) = i.rotation.sin_cos();
                let outline = p.outline.iter().map(|v| Vec2::new(i.insert.x + v.x * c - v.y * s, i.insert.y + v.x * s + v.y * c)).collect();
                risers.push(RiserAt { handle: e.handle, params: p, outline });
            }
        }
        // Where the height available is given: height zones first (a beam is more particular than a
        // room), then spaces. Each carries its outline, its height and the altitude of the underside.
        let mut voids: Vec<(Vec<Vec2>, f64, Option<f64>)> = Vec::new();
        for e in self.model.iter() {
            if let Some((z, outline)) = self.zone_outline(e.handle) {
                voids.push((outline, z.void_height_mm, z.bottom_mm));
            }
        }
        for e in self.model.iter() {
            if let Some((sp, outline)) = self.space_outline(e.handle)
                && let Some(h) = sp.void_height_mm
            {
                voids.push((outline, h, None));
            }
        }
        let mut scan = Scan {
            ducts: &ducts,
            terminals: &terminals,
            units,
            tol,
            taken: vec![false; ducts.len()],
            touched: vec![false; terminals.len()],
            risers: &risers,
            touched_risers: vec![false; risers.len()],
            voids,
            margin_mm,
        };
        let mut out = Vec::new();
        let unit_tol = units.to_units(UNIT_MEET_MM);
        for (handle, p, at, rotation) in &ahus {
            let half = p.half();
            // A duct end within a radius of the unit's face is on it (snapping lands on a line's edge).
            let inside = |q: Vec2, diameter_mm: f64| {
                let l = (q - *at).rotate(-rotation);
                let reach = unit_tol.max(units.to_units(AXIS_REACH * diameter_mm));
                l.x.abs() <= half.x + reach && l.y.abs() <= half.y + reach
            };
            // The direction of the collar a duct end is nearest to, if it is on the unit's face.
            let roles = p.roles();
            let tips: Vec<Vec2> = p.port_tips().into_iter().map(|(t, _)| *at + t.rotate(*rotation)).collect();
            // The collar a duct end is on, and every collar sharing its point (stacked ones): their directions.
            let port = |q: Vec2| {
                let (k, d) = tips.iter().enumerate().map(|(k, t)| (k, (*t - q).len())).min_by(|a, b| a.1.total_cmp(&b.1))?;
                let here = *tips.get(k)?;
                let same: Vec<AirDirection> =
                    tips.iter().enumerate().filter(|(_, t)| (**t - here).len() <= 1e-6).filter_map(|(i, _)| roles.get(i).copied()).collect();
                (d <= units.to_units(0.8 * half.y.min(half.x)) && !same.is_empty()).then_some((k, same))
            };
            let found = scan.read_unit(*handle, &inside, &port, true);
            out.extend(found);
        }
        // Then the risers: where a floor's ducts start when no unit is on that floor. A duct a unit
        // already reads stays that unit's.
        for r in &risers {
            let inside = |q: Vec2, diameter_mm: f64| r.reaches(q, unit_tol.max(units.to_units(AXIS_REACH * diameter_mm)));
            let air = r.params.air;
            let port = |_: Vec2| Some((0, vec![air]));
            out.extend(scan.read_unit(r.handle, &inside, &port, false));
        }
        // A terminal left unreached by every unit is reported on each network: say so once, on the first.
        let lost: Vec<Handle> = terminals.iter().zip(&scan.touched).filter(|(_, t)| !**t).map(|(t, _)| t.handle).collect();
        let unplaced = place_accessories(&ducts, &accessories, units, &mut out);
        if let Some(first) = out.first_mut() {
            first.unconnected = lost;
            first.unplaced = unplaced;
            first.isolated = ducts.iter().zip(&scan.taken).filter(|(_, t)| !**t).map(|(d, _)| d.handle).collect();
            first.duct_count = ducts.len();
        }
        Ok(out)
    }

    /// Cuts every duct in model space where another duct ends on it, or a terminal sits on it, so
    /// the pieces meet at their ends and the network can be read. Returns how many cuts it made.
    /// Run it before reading the networks of a drawing whose ducts were not all drawn with DUCTADD.
    pub fn split_duct_taps(&mut self, space: &crate::Space) -> Result<usize> {
        let units = HvacUnits::of(self);
        let tol = units.to_units(MEET_MM);
        let mut ducts: Vec<(Handle, Vec2, Vec2, f64)> = Vec::new();
        let mut points: Vec<Vec2> = Vec::new();
        for e in self.model.iter() {
            if let Some((p, a, b)) = self.duct_ends(e.handle) {
                if ducts.len() >= MAX_DUCTS {
                    return Err(invalid(format!("The drawing has more than {MAX_DUCTS} ducts.")));
                }
                ducts.push((e.handle, a, b, p.units.to_units(p.plan_width_mm())));
                points.extend([a, b]);
            } else if let (Some(_), EntityKind::Insert(i)) = (air_terminal_of(e), &e.kind) {
                points.push(Vec2::new(i.insert.x, i.insert.y));
            }
        }
        let mut cuts = 0;
        for p in points.clone() {
            // Ducts cut on the way are appended: the point is tried against them too.
            let mut i = 0;
            while let Some(&(h, a, b, d)) = ducts.get(i) {
                i += 1;
                let Some(cut) = interior_cut(a, b, d, p, tol) else { continue };
                if ducts.len() >= MAX_DUCTS {
                    return Err(invalid(format!("The drawing would have more than {MAX_DUCTS} ducts.")));
                }
                if let Some(new) = self.split_duct_at(space, h, p, tol)? {
                    if let Some(first) = ducts.get_mut(i - 1) {
                        first.2 = cut;
                    }
                    ducts.push((new, cut, b, d));
                    cuts += 1;
                    break;
                }
            }
        }
        cuts += self.split_ducts_at_zones(space, tol)?;
        Ok(cuts)
    }

    /// Cuts every duct where it enters or leaves a height zone, so each piece lies wholly in or out
    /// of it and is sized for the height there.
    fn split_ducts_at_zones(&mut self, space: &crate::Space, tol: f64) -> Result<usize> {
        let zones: Vec<Vec<Vec2>> = self.model.iter().filter_map(|e| self.zone_outline(e.handle)).map(|(_, outline)| outline).collect();
        if zones.is_empty() {
            return Ok(0);
        }
        let ducts: Vec<Handle> = self.model.iter().filter(|e| self.duct_ends(e.handle).is_some()).map(|e| e.handle).collect();
        let (mut cuts, mut total) = (0, ducts.len());
        for h in ducts {
            let mut at = h;
            // The nearest crossing along what is left of the duct, until none is.
            while let Some((_, a, b)) = self.duct_ends(at) {
                let hit = zones
                    .iter()
                    .flat_map(|poly| poly.iter().zip(poly.iter().cycle().skip(1)))
                    .filter_map(|(c, d)| segments_cross(a, b, *c, *d))
                    .filter(|p| p.dist(a) > tol && p.dist(b) > tol)
                    .min_by(|x, y| x.dist(a).total_cmp(&y.dist(a)));
                let Some(p) = hit else { break };
                if total >= MAX_DUCTS {
                    return Err(invalid(format!("The drawing would have more than {MAX_DUCTS} ducts.")));
                }
                let Some(next) = self.split_duct_at(space, at, p, tol)? else { break };
                (at, cuts, total) = (next, cuts + 1, total + 1);
            }
        }
        Ok(cuts)
    }

    /// Gives every duct of `net` its diameter, flow and velocity from `report`, and the unit the
    /// flow and pressure it must supply.
    pub fn apply_duct_report(&mut self, net: &DuctNetwork, report: &NetworkReport) -> Result<()> {
        if report.segments.len() != net.ducts.len() {
            return Err(invalid("The report is not the network's."));
        }
        // The new parameters of every duct of the network, then its bends, then the drawing.
        let mut next: Vec<Option<DuctParams>> = Vec::with_capacity(net.ducts.len());
        for (i, (h, r)) in net.ducts.iter().zip(&report.segments).enumerate() {
            let Some((mut p, ..)) = self.duct_ends(*h) else {
                next.push(None);
                continue;
            };
            if !p.imposed {
                (p.shape, p.diameter_mm) = (if r.section == Section::Rectangular { DuctShape::Rectangular } else { DuctShape::Round }, r.diameter_mm);
                (p.width_mm, p.height_mm) = if r.section == Section::Rectangular { (r.width_mm, r.height_mm) } else { (0.0, 0.0) };
            }
            p.flow_m3h = Some(r.flow_m3h);
            p.velocity = Some(r.velocity);
            // A duct whose direction was not fixed takes that of the network it serves.
            if !p.air_set {
                p.air = net.air;
            }
            // A reducer where the run joins one of another diameter.
            // (the width a plan shows: the diameter of a round duct, the width of a rectangular one)
            let parent = net.network.segments.get(i).and_then(|s| s.parent).and_then(|k| report.segments.get(k));
            p.reducer = parent
                .filter(|q| (q.width_mm - r.width_mm).abs() > 0.5)
                .map(|q| DuctReducer { from_mm: q.width_mm, at_end: net.entered_at_end.get(i).copied().unwrap_or(false) });
            (p.elbow, p.trim_start_mm, p.trim_end_mm) = (None, 0.0, 0.0);
            p.bottom_mm = net.bottoms_mm.get(i).copied().flatten();
            // Supply and exhaust air leave the unit, extract and fresh air come to it: with the way the
            // run is entered, that is the way along its block's x axis.
            p.flow_forward = Some(net.air.leaves_the_unit() != net.entered_at_end.get(i).copied().unwrap_or(false));
            next.push(Some(p));
        }
        draw_elbows(net, &mut next);
        for (h, p) in net.ducts.iter().zip(&next) {
            if let Some(p) = p {
                self.regenerate_duct(*h, p)?;
            }
        }
        // A stub hanging on the network that leads to no terminal is left out of the sizing but takes its
        // direction, so it is not left in the default colour.
        for h in &net.dangling {
            if let Some((mut p, ..)) = self.duct_ends(*h)
                && !p.air_set
                && p.air != net.air
            {
                p.air = net.air;
                self.regenerate_duct(*h, &p)?;
            }
        }
        let e = self.entity(net.ahu).ok_or(crate::DocError::NoEntity(net.ahu))?;
        if let Some(mut a) = ahu_of(e) {
            a.set_figures(net.air, report.total_flow_m3h, report.required_pressure_pa);
            self.regenerate_ahu(net.ahu, &a)?;
        } else if let Some(mut r) = riser_of(e) {
            // A riser carries what its floor draws and what comes from the other floors; unless its size
            // was typed, it is sized for that flow in the shaft.
            let total = report.total_flow_m3h + r.incoming_flow_m3h.unwrap_or(0.0);
            r.flow_m3h = Some(total);
            if !r.size_set {
                r.size = size_riser(&r, total);
            }
            self.regenerate_riser(net.ahu, &r)?;
        }
        Ok(())
    }
}

/// The section a riser takes for `flow_m3h` in its shaft: round if it fits the shaft's short side
/// less the margin, else rectangular as tall as the short side lets it be. None when nothing fits.
fn size_riser(r: &RiserParams, flow_m3h: f64) -> Option<RiserSize> {
    let (w, d) = r.shaft_mm();
    let (lo, hi) = (w.min(d) - DEFAULT_MARGIN_MM, w.max(d) - DEFAULT_MARGIN_MM);
    let criteria = DesignCriteria { max_side_mm: hi.min(10_000.0), ..DesignCriteria::default() };
    let segment = Segment {
        parent: None,
        length_m: 3.0,
        kind: SegmentKind::Main,
        fittings: Vec::new(),
        terminal: Some(Terminal { flow_m3h, pressure_drop_pa: 0.0 }),
        diameter_mm: None,
        rect_mm: None,
        max_height_mm: Some(lo),
        fixed_pa: 0.0,
        roughness_mm: None,
    };
    let report = size_network(&Network { segments: vec![segment] }, &criteria).ok()?;
    let s = report.segments.first()?;
    Some(match s.section {
        Section::Round => RiserSize::Round { diameter_mm: s.diameter_mm },
        Section::Rectangular => RiserSize::Rectangular { width_mm: s.width_mm, height_mm: s.height_mm },
    })
}

/// Gives each run that turns away from the one it continues (and only that: no tee) its elbow, and
/// both runs the length it takes off their drawn ends. A bend whose radius would not fit the shorter
/// of the two runs is drawn tighter; one too tight for the width is left as a corner.
fn draw_elbows(net: &DuctNetwork, params: &mut [Option<DuctParams>]) {
    for (i, turn) in net.elbow_turns_deg.iter().enumerate() {
        let (Some(turn), Some(parent)) = (*turn, net.network.segments.get(i).and_then(|s| s.parent)) else { continue };
        let (Some(Some(child)), Some(Some(up))) = (params.get(i), params.get(parent)) else { continue };
        if child.flexible || up.flexible {
            continue;
        }
        let width = child.plan_width_mm();
        let tan_half = (turn.abs().to_radians() / 2.0).tan();
        let room = 0.4 * (child.length * child.units.mm_per_unit).min(up.length * up.units.mm_per_unit);
        // An elbow whose centre radius is the width; tighter if the runs are short.
        let radius = width.min(room / tan_half);
        if !(radius > 0.55 * width && tan_half.is_finite()) {
            continue;
        }
        let at_end = net.entered_at_end.get(i).copied().unwrap_or(false);
        let up_exit_at_start = net.entered_at_end.get(parent).copied().unwrap_or(false);
        let elbow = DuctElbow { angle_deg: turn, radius_mm: radius, at_end };
        let t = elbow.tangent_mm();
        if let Some(Some(c)) = params.get_mut(i) {
            c.elbow = Some(elbow);
            if at_end {
                c.trim_end_mm = t
            } else {
                c.trim_start_mm = t
            };
        }
        if let Some(Some(u)) = params.get_mut(parent) {
            if up_exit_at_start {
                u.trim_start_mm = t
            } else {
                u.trim_end_mm = t
            };
        }
    }
}

/// Adds each accessory's loss to the segment of the duct it sits on, and returns the accessories
/// that sit on no duct. One on a duct left out of every network is dropped without a word.
fn place_accessories(ducts: &[Duct], accessories: &[(Handle, AccessoryParams, Vec2)], units: HvacUnits, nets: &mut [DuctNetwork]) -> Vec<Handle> {
    let mut unplaced = Vec::new();
    for (h, a, at) in accessories {
        let mut best: Option<(f64, Handle)> = None;
        for d in ducts {
            let along = (d.b - d.a).normalized();
            let t = (*at - d.a).dot(along);
            let across = (*at - d.a).cross(along).abs();
            let reach = units.to_units(MEET_MM).max(AXIS_REACH * d.params.units.to_units(d.params.plan_width_mm()));
            if across <= reach && t >= 0.0 && t <= d.a.dist(d.b) && best.is_none_or(|x| across < x.0) {
                best = Some((across, d.handle));
            }
        }
        let Some((_, duct)) = best else {
            unplaced.push(*h);
            continue;
        };
        for net in nets.iter_mut() {
            let Some(i) = net.ducts.iter().position(|x| *x == duct) else { continue };
            if let Some(seg) = net.network.segments.get_mut(i) {
                match a.loss() {
                    Loss::Zeta(zeta) => seg.fittings.push(Fitting::Custom { zeta }),
                    Loss::Pascals(pa) => seg.fixed_pa += pa,
                }
                net.accessories.push(*h);
            }
        }
    }
    unplaced
}

/// How far run `n` turns from its parent, degrees, when its only fitting is one elbow.
fn bare_elbow_turn(nodes: &[Node], n: &Node) -> Option<f64> {
    let bare = n.fittings.iter().any(|f| matches!(f, Fitting::Elbow45 | Fitting::Elbow90))
        && !n.fittings.iter().any(|f| matches!(f, Fitting::TeeStraight | Fitting::TeeBranch));
    let up = nodes.get(n.parent?)?;
    bare.then(|| ((n.to - n.from).angle().to_degrees() - (up.to - up.from).angle().to_degrees() + 540.0).rem_euclid(360.0) - 180.0)
}

/// What reading the networks of a drawing shares: the objects found, and which ducts and
/// terminals the units read so far have used.
struct Scan<'a> {
    ducts: &'a [Duct],
    terminals: &'a [TerminalAt],
    units: HvacUnits,
    tol: f64,
    taken: Vec<bool>,
    touched: Vec<bool>,
    risers: &'a [RiserAt],
    touched_risers: Vec<bool>,
    /// Where a height available is given: outline, height and the altitude of the underside, mm.
    voids: Vec<(Vec<Vec2>, f64, Option<f64>)>,
    /// What is kept back from that height, mm.
    margin_mm: f64,
}

impl Scan<'_> {
    /// The first zone or space that holds `at` and gives a height, with the altitude of the underside.
    fn void_at(&self, at: Vec2) -> Option<(f64, Option<f64>)> {
        self.voids.iter().find(|(outline, ..)| cadcraft_geom::point_in_polygon(outline, at)).map(|v| (v.1, v.2))
    }

    /// The tallest a duct may be at `at`, mm: the height available there less the margin.
    fn height_available(&self, at: Vec2) -> Option<f64> {
        self.void_at(at).map(|(h, _)| h - self.margin_mm)
    }

    fn is_free(&self, i: usize, air: AirDirection) -> bool {
        !self.taken.get(i).copied().unwrap_or(true) && self.ducts.get(i).is_some_and(|d| d.params.air == air || !d.params.air_set)
    }

    fn take(&mut self, i: usize) {
        if let Some(t) = self.taken.get_mut(i) {
            *t = true;
        }
    }

    /// The networks leaving unit `ahu`, whose face `inside` tells (a point and a duct diameter): one
    /// for each air direction that has a duct leading to a terminal. Each duct leaving the unit is
    /// read with its own tree of runs: its direction is the one it was given, else the one of the
    /// terminals the tree reaches, so a double-flux unit's supply and extract trees stay apart.
    fn read_unit(
        &mut self,
        ahu: Handle,
        inside: &impl Fn(Vec2, f64) -> bool,
        port: &impl Fn(Vec2) -> Option<(usize, Vec<AirDirection>)>,
        is_unit: bool,
    ) -> Vec<DuctNetwork> {
        let airs = AirDirection::ALL;
        let mut by_air: [Vec<Node>; 4] = Default::default();
        // The (collar, direction) pairs ducts on a collar have taken: stacked collars share a point.
        let mut claimed: Vec<(usize, usize)> = Vec::new();
        for i in 0..self.ducts.len() {
            let Some(d) = self.ducts.get(i).filter(|_| !self.taken.get(i).copied().unwrap_or(true)) else { continue };
            let (from, to) = if inside(d.a, d.params.plan_width_mm()) {
                (d.a, d.b)
            } else if inside(d.b, d.params.plan_width_mm()) {
                (d.b, d.a)
            } else {
                continue;
            };
            // A direction fixed by hand is the only one tried; else each in turn, the terminals deciding.
            // A direction fixed by hand is the only one tried. A duct on a collar tries the collar's
            // direction first, then the others: a collar set the other way round from the terminals
            // does not leave the duct unread.
            let on = port(from);
            let gid = on.as_ref().map(|(g, _)| *g);
            // The directions of the collars at this point, in order (more than one when they are stacked).
            let mut roles: Vec<usize> = Vec::new();
            for r in on.iter().flat_map(|(_, roles)| roles) {
                if let Some(k) = airs.iter().position(|a| a == r)
                    && !roles.contains(&k)
                {
                    roles.push(k);
                }
            }
            let candidates: Vec<usize> = if d.params.air_set {
                airs.iter().position(|a| *a == d.params.air).into_iter().collect()
            } else if roles.is_empty() {
                (0..airs.len()).collect()
            } else {
                roles.iter().copied().chain((0..airs.len()).filter(|k| !roles.contains(k))).collect()
            };
            let mut chosen = None;
            for &k in &candidates {
                let before = (self.taken.clone(), self.touched.clone(), self.touched_risers.clone());
                let tree = self.tree(i, from, to, airs[k]);
                if tree.first().is_some_and(|n| n.terminals_below > 0) {
                    chosen = Some((k, tree));
                    break;
                }
                (self.taken, self.touched, self.touched_risers) = before;
            }
            // A tree that reaches no terminal is read once more, to name what leads nowhere.
            let (k, tree) = chosen.unwrap_or_else(|| {
                // On stacked collars, the first direction no other open duct has taken at this point.
                let k = candidates
                    .iter()
                    .copied()
                    .find(|k| !roles.contains(k) || !gid.is_some_and(|g| claimed.contains(&(g, *k))))
                    .or(candidates.first().copied())
                    .unwrap_or(0);
                let mut tree = self.tree(i, from, to, airs[k]);
                // A duct on a collar, or one given its direction, that reaches no terminal is open at
                // its ends: air is blown or taken freely there. Any other is left out, as before.
                if d.params.air_set || gid.is_some() {
                    open_the_ends(&mut tree);
                }
                (k, tree)
            });
            if let Some(g) = gid {
                claimed.push((g, k));
            }
            if let Some(slot) = by_air.get_mut(k) {
                append_tree(slot, tree);
            }
        }
        self.flow_open_ends(&mut by_air);
        let mut nets: Vec<DuctNetwork> =
            airs.iter().zip(&by_air).filter(|(_, nodes)| !nodes.is_empty()).map(|(air, nodes)| self.build(ahu, *air, nodes)).collect();
        // A unit with no duct leaving it still gets its (empty) supply network, to say so; a riser none.
        if nets.is_empty() && is_unit {
            nets.push(self.build(ahu, AirDirection::Supply, &[]));
        }
        nets
    }

    /// Gives each open end its flow: the one set on its duct, else its share of what the opposite
    /// direction of the unit carries (fresh air of the supply, exhaust of the extract, and each of
    /// supply and extract of the other). An open end with no flow is left out, as a duct to nowhere.
    fn flow_open_ends(&self, by_air: &mut [Vec<Node>; 4]) {
        let airs = AirDirection::ALL;
        let carried = |nodes: &[Node]| nodes.iter().filter(|n| !n.open).filter_map(|n| n.terminal).map(|t| t.1).sum::<f64>();
        for (k, air) in airs.iter().enumerate() {
            let opposite = match air {
                AirDirection::Fresh => AirDirection::Supply,
                AirDirection::Exhaust => AirDirection::Extract,
                AirDirection::Supply => AirDirection::Extract,
                AirDirection::Extract => AirDirection::Supply,
            };
            let balance = airs.iter().position(|a| *a == opposite).and_then(|o| by_air.get(o)).map_or(0.0, |n| carried(n));
            let Some(nodes) = by_air.get_mut(k) else { continue };
            let set = |n: &Node| self.ducts.get(n.duct).and_then(|d| d.params.open_flow_m3h).filter(|f| *f > 0.0);
            let fixed: f64 = nodes.iter().filter(|n| n.open).filter_map(set).sum();
            let unset = nodes.iter().filter(|n| n.open && set(n).is_none()).count();
            let share = if unset == 0 { 0.0 } else { ((balance - fixed).max(0.0)) / unset as f64 };
            let flows: Vec<Option<f64>> = nodes.iter().map(|n| n.open.then(|| set(n).unwrap_or(share)).filter(|f| *f > 0.0)).collect();
            for (n, f) in nodes.iter_mut().zip(flows) {
                if n.open {
                    n.terminal = f.zip(self.ducts.get(n.duct)).map(|(f, d)| (d.handle, f, 0.0));
                    n.open = false;
                }
            }
            // The runs that lead to no flow are counted again.
            let mut first = 0;
            while first < nodes.len() {
                let end = nodes.iter().skip(first + 1).position(|n| n.parent.is_none()).map_or(nodes.len(), |e| first + 1 + e);
                if let Some(tree) = nodes.get_mut(first..end) {
                    recount(tree, first);
                }
                first = end;
            }
        }
    }

    /// The tree of runs starting with duct `root`, entered at `from`, as direction `air`, its
    /// terminals attached and counted.
    fn tree(&mut self, root: usize, from: Vec2, to: Vec2, air: AirDirection) -> Vec<Node> {
        self.take(root);
        let mut nodes = vec![Node { duct: root, parent: None, from, to, terminal: None, open: false, fittings: Vec::new(), terminals_below: 0 }];
        self.grow(&mut nodes, air);
        self.attach_terminals(&mut nodes, air);
        count_terminals(&mut nodes);
        nodes
    }

    /// Each run's far end leads on to the runs that start there.
    fn grow(&mut self, nodes: &mut Vec<Node>, air: AirDirection) {
        let mut next = 0;
        while let Some(n) = nodes.get(next) {
            let (to, from) = (n.to, n.from);
            let parent_diameter = self.ducts.get(n.duct).map_or(0.0, |d| d.params.plan_width_mm());
            let mut children: Vec<(usize, Vec2, Vec2)> = Vec::new();
            for i in 0..self.ducts.len() {
                let Some(d) = self.ducts.get(i).filter(|_| self.is_free(i, air)) else { continue };
                // Ends meet within a radius of the wider duct: snapping lands on a line's edge.
                let meet = self.tol.max(self.units.to_units(AXIS_REACH * d.params.plan_width_mm().max(parent_diameter)));
                let (f, t) = if d.a.near(to, meet) {
                    (d.a, d.b)
                } else if d.b.near(to, meet) {
                    (d.b, d.a)
                } else {
                    continue;
                };
                self.take(i);
                children.push((i, f, t));
            }
            // The straightest child goes through the tee, the others turn into it.
            let heading = (to - from).normalized();
            let turn = |f: Vec2, t: Vec2| heading.dot((t - f).normalized()).clamp(-1.0, 1.0).acos().to_degrees();
            children.sort_by(|x, y| turn(x.1, x.2).total_cmp(&turn(y.1, y.2)));
            let many = children.len() > 1;
            for (k, (i, f, t)) in children.into_iter().enumerate() {
                let fittings = junction_fittings(many, k, turn(f, t));
                nodes.push(Node { duct: i, parent: Some(next), from: f, to: t, terminal: None, open: false, fittings, terminals_below: 0 });
            }
            next += 1;
        }
    }

    /// The terminal of `air` at each run's far end.
    fn attach_terminals(&mut self, nodes: &mut [Node], air: AirDirection) {
        for n in nodes {
            let diameter = self.ducts.get(n.duct).map_or(0.0, |d| d.params.plan_width_mm());
            let reach = self.tol.max(self.units.to_units(AXIS_REACH * diameter));
            for (ti, t) in self.terminals.iter().enumerate() {
                if t.params.air == air && !self.touched.get(ti).copied().unwrap_or(true) && t.is_reached(n.to, reach) {
                    n.terminal = Some((t.handle, t.params.flow_m3h, t.params.model.pressure_drop_pa));
                    if let Some(x) = self.touched.get_mut(ti) {
                        *x = true;
                    }
                    break;
                }
            }
            // On the roof plan the ducts of a unit end on a riser, with the flow typed on it.
            if n.terminal.is_none() {
                for (ri, r) in self.risers.iter().enumerate() {
                    let flow = r.params.incoming_flow_m3h.filter(|f| *f > 0.0);
                    if let Some(flow) = flow
                        && r.params.air == air
                        && !self.touched_risers.get(ri).copied().unwrap_or(true)
                        && r.reaches(n.to, reach)
                    {
                        n.terminal = Some((r.handle, flow, 0.0));
                        if let Some(x) = self.touched_risers.get_mut(ri) {
                            *x = true;
                        }
                        break;
                    }
                }
            }
        }
    }

    /// The runs that lead to a terminal as a network; the others are left out and named.
    fn build(&self, ahu: Handle, air: AirDirection, nodes: &[Node]) -> DuctNetwork {
        let kept: Vec<usize> = (0..nodes.len()).filter(|i| nodes.get(*i).is_some_and(|n| n.terminals_below > 0)).collect();
        let dangling = (0..nodes.len())
            .filter(|i| nodes.get(*i).is_some_and(|n| n.terminals_below == 0))
            .filter_map(|i| nodes.get(i).and_then(|n| self.ducts.get(n.duct)).map(|d| d.handle))
            .collect();
        let index_of = |old: usize| kept.iter().position(|k| *k == old);
        let mut segments = Vec::with_capacity(kept.len());
        let mut handles = Vec::with_capacity(kept.len());
        let mut entered_at_end = Vec::with_capacity(kept.len());
        let mut elbow_turns_deg = Vec::with_capacity(kept.len());
        let mut bottoms_mm = Vec::with_capacity(kept.len());
        let mut main_chain = vec![false; nodes.len()];
        for &old in &kept {
            let Some(n) = nodes.get(old) else { continue };
            let Some(d) = self.ducts.get(n.duct) else { continue };
            let parent_chain = n.parent.is_none_or(|p| main_chain.get(p).copied().unwrap_or(false));
            let on_trunk = parent_chain && !n.fittings.contains(&Fitting::TeeBranch);
            if let Some(c) = main_chain.get_mut(old) {
                *c = on_trunk;
            }
            segments.push(Segment {
                parent: n.parent.and_then(index_of),
                length_m: d.params.length_m(),
                kind: segment_kind(d.params.kind, n.terminals_below, on_trunk),
                fittings: n.fittings.clone(),
                terminal: n.terminal.map(|(_, flow, pa)| Terminal { flow_m3h: flow, pressure_drop_pa: pa }),
                diameter_mm: (d.params.imposed && d.params.shape == DuctShape::Round).then_some(d.params.diameter_mm),
                rect_mm: (d.params.imposed && d.params.shape == DuctShape::Rectangular).then_some((d.params.width_mm, d.params.height_mm)),
                max_height_mm: self.height_available(d.a.mid(d.b)),
                fixed_pa: 0.0,
                roughness_mm: d.params.flexible.then_some(FLEX_ROUGHNESS_MM),
            });
            entered_at_end.push(n.from == d.b && d.a != d.b);
            elbow_turns_deg.push(bare_elbow_turn(nodes, n));
            bottoms_mm.push(self.void_at(d.a.mid(d.b)).and_then(|(_, b)| b));
            handles.push(d.handle);
        }
        DuctNetwork {
            ahu,
            air,
            network: Network { segments },
            ducts: handles,
            elbow_turns_deg,
            bottoms_mm,
            entered_at_end,
            dangling,
            unconnected: Vec::new(),
            accessories: Vec::new(),
            unplaced: Vec::new(),
            isolated: Vec::new(),
            duct_count: 0,
        }
    }
}

/// Adds `tree` to `nodes`, its parent indices moved past the nodes already there.
fn append_tree(nodes: &mut Vec<Node>, tree: Vec<Node>) {
    let offset = nodes.len();
    nodes.extend(tree.into_iter().map(|mut n| {
        n.parent = n.parent.map(|p| p + offset);
        n
    }));
}

/// The fittings at the start of child `k` of a node, `many` when the node has several children and
/// `angle` degrees the child turns away from the way the air was going.
fn junction_fittings(many: bool, k: usize, angle: f64) -> Vec<Fitting> {
    let mut fittings = Vec::new();
    if many {
        fittings.push(if k == 0 { Fitting::TeeStraight } else { Fitting::TeeBranch });
    }
    if !many || k > 0 {
        if angle >= ELBOW_90_DEG {
            fittings.push(Fitting::Elbow90);
        } else if angle >= ELBOW_45_DEG {
            fittings.push(Fitting::Elbow45);
        }
    }
    fittings
}

/// The velocity class of a run: the one given, else a connection to one terminal, a main along the
/// trunk, or a branch.
fn segment_kind(kind: DuctKind, terminals_below: usize, on_trunk: bool) -> SegmentKind {
    match kind {
        DuctKind::Main => SegmentKind::Main,
        DuctKind::Branch => SegmentKind::Branch,
        DuctKind::Connection => SegmentKind::Connection,
        DuctKind::Auto if terminals_below <= 1 => SegmentKind::Connection,
        DuctKind::Auto if on_trunk => SegmentKind::Main,
        DuctKind::Auto => SegmentKind::Branch,
    }
}

/// Where segment `a`–`b` crosses segment `c`–`d`, if it does.
fn segments_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<Vec2> {
    let (r, s) = (b - a, d - c);
    let denom = r.cross(s);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).cross(s) / denom;
    let u = (c - a).cross(r) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a + r * t)
}

/// Marks every run no other run leaves from as open at its far end, and counts again.
fn open_the_ends(tree: &mut [Node]) {
    let has_child: Vec<bool> = (0..tree.len()).map(|i| tree.iter().any(|n| n.parent == Some(i))).collect();
    for (n, child) in tree.iter_mut().zip(has_child) {
        n.open = !child && n.terminal.is_none();
    }
    recount(tree, 0);
}

/// [`count_terminals`] on a tree whose parent indices start at `offset` (a tree appended after others).
fn recount(tree: &mut [Node], offset: usize) {
    for n in tree.iter_mut() {
        n.terminals_below = 0;
        n.parent = n.parent.map(|p| p.saturating_sub(offset));
    }
    count_terminals(tree);
    for n in tree.iter_mut() {
        n.parent = n.parent.map(|p| p + offset);
    }
}

/// How many terminals each run serves, itself and all the runs below it.
fn count_terminals(nodes: &mut [Node]) {
    for i in (0..nodes.len()).rev() {
        let own = usize::from(nodes.get(i).is_some_and(|n| n.terminal.is_some() || n.open));
        let below = nodes.get(i).map_or(0, |n| n.terminals_below) + own;
        if let Some(n) = nodes.get_mut(i) {
            n.terminals_below = below;
        }
        if let Some(p) = nodes.get(i).and_then(|n| n.parent)
            && let Some(parent) = nodes.get_mut(p)
        {
            parent.terminals_below += below;
        }
    }
}
