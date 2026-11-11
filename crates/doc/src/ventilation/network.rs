//! The network of ducts drawn from an air handling unit, read back from where their ends meet:
//! nothing about it is stored, so moving or erasing a duct changes it at the next reading.

use cadcraft_geom::Vec2;
use cadcraft_hvac::{Fitting, Network, NetworkReport, Segment, SegmentKind, Terminal};

use super::{AhuParams, DuctKind, DuctParams, HvacUnits, TerminalParams, ahu_of, air_terminal_of, invalid};
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

/// One air handling unit's ducts as a network ready to size.
#[derive(Clone, Debug, PartialEq)]
pub struct DuctNetwork {
    pub ahu: Handle,
    pub network: Network,
    /// The duct of each segment of `network`, in the same order.
    pub ducts: Vec<Handle>,
    /// Ducts connected to the unit that lead to no terminal, so are left out of the sizing.
    pub dangling: Vec<Handle>,
    /// Terminals that no duct reaches.
    pub unconnected: Vec<Handle>,
}

struct Duct {
    handle: Handle,
    params: DuctParams,
    a: Vec2,
    b: Vec2,
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
        let mut terminals: Vec<(Handle, TerminalParams, Vec2)> = Vec::new();
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
                terminals.push((e.handle, p, Vec2::new(i.insert.x, i.insert.y)));
            }
        }
        let mut taken = vec![false; ducts.len()];
        let mut touched_terminals = vec![false; terminals.len()];
        let mut out = Vec::new();
        let unit_tol = units.to_units(UNIT_MEET_MM);
        for (handle, p, at, rotation) in &ahus {
            let half = p.half();
            let inside = |q: Vec2| {
                let l = (q - *at).rotate(-rotation);
                l.x.abs() <= half.x + unit_tol && l.y.abs() <= half.y + unit_tol
            };
            let net = read_network(*handle, &ducts, &mut taken, &terminals, &mut touched_terminals, tol, inside);
            out.push(net);
        }
        // A terminal left unreached by every unit is reported on each network: say so once, on the first.
        let lost: Vec<Handle> = terminals.iter().zip(&touched_terminals).filter(|(_, t)| !**t).map(|((h, ..), _)| *h).collect();
        if let Some(first) = out.first_mut() {
            first.unconnected = lost;
        }
        Ok(out)
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
            a.flow_m3h = Some(report.total_flow_m3h);
            a.pressure_pa = Some(report.required_pressure_pa);
            self.regenerate_ahu(net.ahu, &a)?;
        }
        Ok(())
    }
}

fn read_network(
    ahu: Handle,
    ducts: &[Duct],
    taken: &mut [bool],
    terminals: &[(Handle, TerminalParams, Vec2)],
    touched: &mut [bool],
    tol: f64,
    inside: impl Fn(Vec2) -> bool,
) -> DuctNetwork {
    let mut nodes: Vec<Node> = Vec::new();
    // Ducts leaving the unit.
    for (i, d) in ducts.iter().enumerate() {
        if taken.get(i).copied().unwrap_or(true) {
            continue;
        }
        let (from, to) = if inside(d.a) {
            (d.a, d.b)
        } else if inside(d.b) {
            (d.b, d.a)
        } else {
            continue;
        };
        if let Some(t) = taken.get_mut(i) {
            *t = true;
        }
        nodes.push(Node { duct: i, parent: None, from, to, terminal: None, fittings: Vec::new(), terminals_below: 0 });
    }
    // Each run's far end leads on to the runs that start there.
    let mut next = 0;
    while let Some(n) = nodes.get(next) {
        let (here, to, from) = (next, n.to, n.from);
        let mut children: Vec<(usize, Vec2, Vec2)> = Vec::new();
        for (i, d) in ducts.iter().enumerate() {
            if taken.get(i).copied().unwrap_or(true) {
                continue;
            }
            let (f, t) = if d.a.near(to, tol) {
                (d.a, d.b)
            } else if d.b.near(to, tol) {
                (d.b, d.a)
            } else {
                continue;
            };
            if let Some(x) = taken.get_mut(i) {
                *x = true;
            }
            children.push((i, f, t));
        }
        // The straightest child goes through the tee, the others turn into it.
        let heading = (to - from).normalized();
        let turn = |f: Vec2, t: Vec2| heading.dot((t - f).normalized()).clamp(-1.0, 1.0).acos().to_degrees();
        children.sort_by(|x, y| turn(x.1, x.2).total_cmp(&turn(y.1, y.2)));
        let many = children.len() > 1;
        for (k, (i, f, t)) in children.into_iter().enumerate() {
            let angle = turn(f, t);
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
            nodes.push(Node { duct: i, parent: Some(here), from: f, to: t, terminal: None, fittings, terminals_below: 0 });
        }
        next += 1;
    }
    // The terminal at each run's far end.
    for n in &mut nodes {
        for (ti, (h, p, at)) in terminals.iter().enumerate() {
            let reach = tol + p.units.to_units(p.model.neck_mm) / 2.0;
            if at.near(n.to, reach) {
                n.terminal = Some((*h, p.flow_m3h, p.model.pressure_drop_pa));
                if let Some(t) = touched.get_mut(ti) {
                    *t = true;
                }
                break;
            }
        }
    }
    // Runs that lead to no terminal are left out; each count comes from the runs below it.
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
    let kept: Vec<usize> = (0..nodes.len()).filter(|i| nodes.get(*i).is_some_and(|n| n.terminals_below > 0)).collect();
    let dangling = (0..nodes.len())
        .filter(|i| nodes.get(*i).is_some_and(|n| n.terminals_below == 0))
        .filter_map(|i| nodes.get(i).and_then(|n| ducts.get(n.duct)).map(|d| d.handle))
        .collect();
    let index_of = |old: usize| kept.iter().position(|k| *k == old);
    let mut segments = Vec::with_capacity(kept.len());
    let mut handles = Vec::with_capacity(kept.len());
    let mut main_chain = vec![false; nodes.len()];
    for &old in &kept {
        let Some(n) = nodes.get(old) else { continue };
        let Some(d) = ducts.get(n.duct) else { continue };
        let parent_chain = n.parent.is_none_or(|p| main_chain.get(p).copied().unwrap_or(false));
        let straight = !n.fittings.contains(&Fitting::TeeBranch);
        let on_trunk = parent_chain && straight;
        if let Some(c) = main_chain.get_mut(old) {
            *c = on_trunk;
        }
        let kind = match d.params.kind {
            DuctKind::Main => SegmentKind::Main,
            DuctKind::Branch => SegmentKind::Branch,
            DuctKind::Connection => SegmentKind::Connection,
            DuctKind::Auto if n.terminals_below <= 1 => SegmentKind::Connection,
            DuctKind::Auto if on_trunk => SegmentKind::Main,
            DuctKind::Auto => SegmentKind::Branch,
        };
        segments.push(Segment {
            parent: n.parent.and_then(index_of),
            length_m: d.params.length_m(),
            kind,
            fittings: n.fittings.clone(),
            terminal: n.terminal.map(|(_, flow, pa)| Terminal { flow_m3h: flow, pressure_drop_pa: pa }),
            diameter_mm: d.params.imposed.then_some(d.params.diameter_mm),
        });
        handles.push(d.handle);
    }
    DuctNetwork { ahu, network: Network { segments }, ducts: handles, dangling, unconnected: Vec::new() }
}
