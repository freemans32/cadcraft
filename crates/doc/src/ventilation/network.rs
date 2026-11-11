//! The network of ducts drawn from an air handling unit, read back from where their ends meet:
//! nothing about it is stored, so moving or erasing a duct changes it at the next reading.

use cadcraft_geom::Vec2;
use cadcraft_hvac::{Fitting, Network, NetworkReport, Segment, SegmentKind, Terminal};

use super::duct::{AXIS_REACH, interior_cut};
use super::{
    AccessoryParams, AhuParams, AirDirection, DuctKind, DuctParams, HvacUnits, Loss, TerminalParams, accessory_of, ahu_of, air_terminal_of, invalid,
};
use crate::{EntityKind, Handle, Result};

/// Most ducts one drawing's networks are read from.
pub const MAX_DUCTS: usize = 5_000;
/// Distance, in millimetres, within which two ends are one point.
const MEET_MM: f64 = 10.0;
/// Distance, in millimetres, within which a duct end is on an air handling unit: a click near its face.
const UNIT_MEET_MM: f64 = 100.0;
/// Turn, in degrees, from which a duct run needs a 45° elbow, and a 90° elbow.
const ELBOW_45_DEG: f64 = 10.0;
const ELBOW_90_DEG: f64 = 67.5;

/// One air handling unit's ducts of one air direction as a network ready to size. A double-flux
/// unit has two: its supply ducts with the supply terminals, its extract ducts with the extract ones.
#[derive(Clone, Debug, PartialEq)]
pub struct DuctNetwork {
    pub ahu: Handle,
    pub air: AirDirection,
    pub network: Network,
    /// The duct of each segment of `network`, in the same order.
    pub ducts: Vec<Handle>,
    /// Ducts connected to the unit that lead to no terminal, so are left out of the sizing.
    pub dangling: Vec<Handle>,
    /// Terminals that no duct of their air direction reaches (set on the first network only).
    pub unconnected: Vec<Handle>,
    /// The accessories whose loss is in this network.
    pub accessories: Vec<Handle>,
    /// Accessories on no duct (set on the first network only).
    pub unplaced: Vec<Handle>,
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

struct Node {
    duct: usize,
    parent: Option<usize>,
    /// The end the run is entered by, and the end it leaves by.
    from: Vec2,
    to: Vec2,
    terminal: Option<(Handle, f64, f64)>,
    fittings: Vec<Fitting>,
    terminals_below: usize,
}

impl crate::Drawing {
    /// The networks of every air handling unit in model space. Several units may not share a duct:
    /// a duct is read once, for the first unit that reaches it.
    pub fn duct_networks(&self) -> Result<Vec<DuctNetwork>> {
        let units = HvacUnits::of(self);
        let tol = units.to_units(MEET_MM);
        let mut ducts = Vec::new();
        let mut ahus: Vec<(Handle, AhuParams, Vec2, f64)> = Vec::new();
        let mut terminals: Vec<TerminalAt> = Vec::new();
        let mut accessories: Vec<(Handle, AccessoryParams, Vec2)> = Vec::new();
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
            }
        }
        let mut scan =
            Scan { ducts: &ducts, terminals: &terminals, units, tol, taken: vec![false; ducts.len()], touched: vec![false; terminals.len()] };
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
            let mut found = Vec::new();
            for air in [AirDirection::Supply, AirDirection::Extract] {
                let net = scan.read(*handle, air, &inside);
                if !(net.ducts.is_empty() && net.dangling.is_empty()) {
                    found.push(net);
                }
            }
            // A unit none of whose ducts leave it still gets its (empty) supply network, to say so.
            if found.is_empty() {
                found.push(scan.read(*handle, AirDirection::Supply, &inside));
            }
            out.extend(found);
        }
        // A terminal left unreached by every unit is reported on each network: say so once, on the first.
        let lost: Vec<Handle> = terminals.iter().zip(&scan.touched).filter(|(_, t)| !**t).map(|(t, _)| t.handle).collect();
        let unplaced = place_accessories(&ducts, &accessories, units, &mut out);
        if let Some(first) = out.first_mut() {
            first.unconnected = lost;
            first.unplaced = unplaced;
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
                ducts.push((e.handle, a, b, p.units.to_units(p.diameter_mm)));
                points.extend([a, b]);
            } else if let (Some(_), EntityKind::Insert(i)) = (air_terminal_of(e), &e.kind) {
                points.push(Vec2::new(i.insert.x, i.insert.y));
            }
        }
        let mut cuts = 0;
        for p in points {
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
        Ok(cuts)
    }

    /// Gives every duct of `net` its diameter, flow and velocity from `report`, and the unit the
    /// flow and pressure it must supply.
    pub fn apply_duct_report(&mut self, net: &DuctNetwork, report: &NetworkReport) -> Result<()> {
        if report.segments.len() != net.ducts.len() {
            return Err(invalid("The report is not the network's."));
        }
        for (h, r) in net.ducts.iter().zip(&report.segments) {
            let Some((mut p, ..)) = self.duct_ends(*h) else { continue };
            if !p.imposed {
                p.diameter_mm = r.diameter_mm;
            }
            p.flow_m3h = Some(r.flow_m3h);
            p.velocity = Some(r.velocity);
            self.regenerate_duct(*h, &p)?;
        }
        let e = self.entity(net.ahu).ok_or(crate::DocError::NoEntity(net.ahu))?;
        if let Some(mut a) = ahu_of(e) {
            let (flow, pressure) = (Some(report.total_flow_m3h), Some(report.required_pressure_pa));
            match net.air {
                AirDirection::Supply => (a.flow_m3h, a.pressure_pa) = (flow, pressure),
                AirDirection::Extract => (a.extract_flow_m3h, a.extract_pressure_pa) = (flow, pressure),
            }
            self.regenerate_ahu(net.ahu, &a)?;
        }
        Ok(())
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
            let reach = units.to_units(MEET_MM).max(AXIS_REACH * d.params.units.to_units(d.params.diameter_mm));
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

/// What reading the networks of a drawing shares: the objects found, and which ducts and
/// terminals the units read so far have used.
struct Scan<'a> {
    ducts: &'a [Duct],
    terminals: &'a [TerminalAt],
    units: HvacUnits,
    tol: f64,
    taken: Vec<bool>,
    touched: Vec<bool>,
}

impl Scan<'_> {
    fn is_free(&self, i: usize, air: AirDirection) -> bool {
        !self.taken.get(i).copied().unwrap_or(true) && self.ducts.get(i).is_some_and(|d| d.params.air == air)
    }

    fn take(&mut self, i: usize) {
        if let Some(t) = self.taken.get_mut(i) {
            *t = true;
        }
    }

    /// The network of `air` leaving unit `ahu`, whose face `inside` tells (a point and a duct diameter).
    fn read(&mut self, ahu: Handle, air: AirDirection, inside: &impl Fn(Vec2, f64) -> bool) -> DuctNetwork {
        let mut nodes = self.roots(air, inside);
        self.grow(&mut nodes, air);
        self.attach_terminals(&mut nodes, air);
        count_terminals(&mut nodes);
        self.build(ahu, air, &nodes)
    }

    /// The ducts of `air` that leave the unit.
    fn roots(&mut self, air: AirDirection, inside: &impl Fn(Vec2, f64) -> bool) -> Vec<Node> {
        let mut nodes = Vec::new();
        for i in 0..self.ducts.len() {
            let Some(d) = self.ducts.get(i).filter(|_| self.is_free(i, air)) else { continue };
            let (from, to) = if inside(d.a, d.params.diameter_mm) {
                (d.a, d.b)
            } else if inside(d.b, d.params.diameter_mm) {
                (d.b, d.a)
            } else {
                continue;
            };
            self.take(i);
            nodes.push(Node { duct: i, parent: None, from, to, terminal: None, fittings: Vec::new(), terminals_below: 0 });
        }
        nodes
    }

    /// Each run's far end leads on to the runs that start there.
    fn grow(&mut self, nodes: &mut Vec<Node>, air: AirDirection) {
        let mut next = 0;
        while let Some(n) = nodes.get(next) {
            let (to, from) = (n.to, n.from);
            let parent_diameter = self.ducts.get(n.duct).map_or(0.0, |d| d.params.diameter_mm);
            let mut children: Vec<(usize, Vec2, Vec2)> = Vec::new();
            for i in 0..self.ducts.len() {
                let Some(d) = self.ducts.get(i).filter(|_| self.is_free(i, air)) else { continue };
                // Ends meet within a radius of the wider duct: snapping lands on a line's edge.
                let meet = self.tol.max(self.units.to_units(AXIS_REACH * d.params.diameter_mm.max(parent_diameter)));
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
                nodes.push(Node { duct: i, parent: Some(next), from: f, to: t, terminal: None, fittings, terminals_below: 0 });
            }
            next += 1;
        }
    }

    /// The terminal of `air` at each run's far end.
    fn attach_terminals(&mut self, nodes: &mut [Node], air: AirDirection) {
        for n in nodes {
            let diameter = self.ducts.get(n.duct).map_or(0.0, |d| d.params.diameter_mm);
            let reach = self.tol.max(self.units.to_units(AXIS_REACH * diameter));
            for (ti, t) in self.terminals.iter().enumerate() {
                if t.params.air == air && t.is_reached(n.to, reach) {
                    n.terminal = Some((t.handle, t.params.flow_m3h, t.params.model.pressure_drop_pa));
                    if let Some(x) = self.touched.get_mut(ti) {
                        *x = true;
                    }
                    break;
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
                diameter_mm: d.params.imposed.then_some(d.params.diameter_mm),
                fixed_pa: 0.0,
            });
            handles.push(d.handle);
        }
        DuctNetwork {
            ahu,
            air,
            network: Network { segments },
            ducts: handles,
            dangling,
            unconnected: Vec::new(),
            accessories: Vec::new(),
            unplaced: Vec::new(),
        }
    }
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

/// How many terminals each run serves, itself and all the runs below it.
fn count_terminals(nodes: &mut [Node]) {
    for i in (0..nodes.len()).rev() {
        let own = usize::from(nodes.get(i).is_some_and(|n| n.terminal.is_some()));
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
