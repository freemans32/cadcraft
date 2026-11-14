//! The pipe networks of a drawing, read from where the runs' ends meet (never stored), and their sizing: each
//! network of a service (and, for water, of a leg) starts at a run marked as its source; the loads typed at the free
//! ends are added up towards it; the flow follows from the load, and the size from the flow.

use cadcraft_geom::Vec2;
use cadcraft_hvac::{PipeCriteria, PipeGoverning, WaterTemperature, check_water_pipe, size_condensate_drain, size_water_pipe, water_flow_m3h};

use super::{PipeLeg, PipeParams, PipeService, invalid};
use crate::{Handle, Result};

/// How close two ends are to be one point, mm; and, as a share of the wider pipe, how close to its axis.
const MEET_MM: f64 = 15.0;
/// Most runs one network reads.
pub const MAX_PIPES: usize = 20_000;

/// The temperature differences the water networks are sized for, K, and the slope of the drains, %.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PipeSizingParams {
    pub delta_t_chilled: f64,
    pub delta_t_hot: f64,
    pub delta_t_change_over: f64,
    /// The slope of a condensate drain, %.
    pub drain_slope_percent: f64,
    pub criteria: PipeCriteria,
}

impl Default for PipeSizingParams {
    /// 7/12 °C for chilled water, 70/50 °C for hot water, and the chilled-water difference for the change-over network
    /// (its cooling duty is the larger flow).
    fn default() -> Self {
        PipeSizingParams {
            delta_t_chilled: 5.0,
            delta_t_hot: 20.0,
            delta_t_change_over: 5.0,
            drain_slope_percent: 1.0,
            criteria: PipeCriteria::default(),
        }
    }
}

/// One run of a sized network.
#[derive(Clone, Debug, PartialEq)]
pub struct PipeResult {
    pub handle: Handle,
    pub dn_mm: f64,
    /// The load the run carries, kW (l/h for a drain), and the flow, m³/h.
    pub load: f64,
    pub flow_m3h: f64,
    pub velocity_ms: f64,
    pub gradient_pa_m: f64,
    pub imposed: bool,
    pub governing: PipeGoverning,
}

/// A sized network: its service and leg, its source run, its total load and its runs.
#[derive(Clone, Debug, PartialEq)]
pub struct PipeNetworkReport {
    pub service: PipeService,
    pub leg: PipeLeg,
    pub source: Handle,
    pub total_load: f64,
    pub runs: Vec<PipeResult>,
    pub warnings: Vec<String>,
}

struct Run {
    handle: Handle,
    params: PipeParams,
    a: Vec2,
    b: Vec2,
}

struct Node {
    run: usize,
    parent: Option<usize>,
    /// Whether the run is entered by its start (the block's origin).
    at_start: bool,
}

fn group(p: &PipeParams) -> (PipeService, PipeLeg) {
    (p.service, if p.service.has_legs() { p.leg } else { PipeLeg::Supply })
}

impl crate::Drawing {
    /// Reads every pipe network of model space and sizes it (the runs whose size was given are checked, not changed),
    /// writing the flow, velocity and pressure loss on the runs and the size on the others. Refrigerant lines are
    /// not sized: their size is the user's. Returns a report per network, and warnings for runs that belong to none.
    pub fn size_pipes(&mut self, sizing: &PipeSizingParams) -> Result<(Vec<PipeNetworkReport>, Vec<String>)> {
        let runs: Vec<Run> = self
            .model
            .iter()
            .filter_map(|e| self.pipe_ends(e.handle).map(|(params, a, b)| Run { handle: e.handle, params, a, b }))
            .filter(|r| !r.params.service.is_refrigerant())
            .collect();
        if runs.len() > MAX_PIPES {
            return Err(invalid(format!("Pipes are sized {MAX_PIPES} at a time at most.")));
        }
        let mut taken = vec![false; runs.len()];
        let mut reports = Vec::new();
        let mut writes: Vec<(Handle, PipeParams)> = Vec::new();
        for (i, root) in runs.iter().enumerate() {
            if !root.params.source || taken.get(i).copied().unwrap_or(true) {
                continue;
            }
            let nodes = read_tree(&runs, &mut taken, i);
            let (report, changed) = size_tree(&runs, &nodes, sizing)?;
            writes.extend(changed);
            reports.push(report);
        }
        let mut warnings = Vec::new();
        for (r, t) in runs.iter().zip(&taken) {
            if !t {
                warnings.push(format!(
                    "the {} run at ({:.0}, {:.0}) belongs to no network: mark the run that starts at the source (\"Starts at the source\")",
                    r.params.tag(),
                    r.a.x,
                    r.a.y
                ));
            }
        }
        for (h, p) in writes {
            self.regenerate_pipe(h, &p)?;
        }
        Ok((reports, warnings))
    }
}

/// The tree of runs from source run `root`, entered at its start: a run is the child of the run whose far end it
/// starts on, of the same service and leg.
fn read_tree(runs: &[Run], taken: &mut [bool], root: usize) -> Vec<Node> {
    let mut nodes = vec![Node { run: root, parent: None, at_start: true }];
    if let Some(t) = taken.get_mut(root) {
        *t = true;
    }
    let Some(r0) = runs.get(root) else { return nodes };
    let wanted = group(&r0.params);
    let mut next = 0;
    while let Some(n) = nodes.get(next) {
        let Some(cur) = runs.get(n.run) else { break };
        let to = if n.at_start { cur.b } else { cur.a };
        let mut found = Vec::new();
        for (j, other) in runs.iter().enumerate() {
            if taken.get(j).copied().unwrap_or(true) || group(&other.params) != wanted {
                continue;
            }
            let tol = other
                .params
                .units
                .to_units(MEET_MM)
                .max(0.75 * other.params.units.to_units(other.params.plan_width_mm().max(cur.params.plan_width_mm())));
            if other.a.near(to, tol) {
                found.push((j, true));
            } else if other.b.near(to, tol) {
                found.push((j, false));
            }
        }
        for (j, at_start) in found {
            if let Some(t) = taken.get_mut(j) {
                *t = true;
            }
            nodes.push(Node { run: j, parent: Some(next), at_start });
        }
        next += 1;
        if nodes.len() > MAX_PIPES {
            break;
        }
    }
    nodes
}

fn size_tree(runs: &[Run], nodes: &[Node], sizing: &PipeSizingParams) -> Result<(PipeNetworkReport, Vec<(Handle, PipeParams)>)> {
    let mut warnings: Vec<String> = Vec::new();
    let Some(root_run) = nodes.first().and_then(|n| runs.get(n.run)) else { return Err(invalid("A network has a source.")) };
    let (service, leg) = group(&root_run.params);
    // The loads added up from the leaves to the source.
    let mut total: Vec<f64> = nodes.iter().map(|n| runs.get(n.run).and_then(|r| r.params.load_kw).unwrap_or(0.0)).collect();
    let own = total.clone();
    for i in (0..nodes.len()).rev() {
        let (t, parent) = (total.get(i).copied().unwrap_or(0.0), nodes.get(i).and_then(|n| n.parent));
        if let Some(p) = parent.and_then(|p| total.get_mut(p)) {
            *p += t;
        }
    }
    let has_child: Vec<bool> = (0..nodes.len()).map(|i| nodes.iter().any(|n| n.parent == Some(i))).collect();
    for (i, n) in nodes.iter().enumerate() {
        let leaf_without_load = !has_child.get(i).copied().unwrap_or(true) && own.get(i).copied().unwrap_or(0.0) <= 0.0;
        if let (true, Some(r)) = (leaf_without_load, runs.get(n.run)) {
            warnings.push(format!(
                "the {} run ending at ({:.0}, {:.0}) serves no load: type the load of the unit it feeds",
                r.params.tag(),
                r.b.x,
                r.b.y
            ));
        }
    }
    let water = match service {
        PipeService::ChilledWater => Some((WaterTemperature::Cold, sizing.delta_t_chilled)),
        PipeService::HotWater => Some((WaterTemperature::Hot, sizing.delta_t_hot)),
        PipeService::ChangeOver => Some((WaterTemperature::Mixed, sizing.delta_t_change_over)),
        _ => None,
    };
    let mut results = Vec::new();
    let mut changed = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        let Some(r) = runs.get(n.run) else { continue };
        let load = total.get(i).copied().unwrap_or(0.0);
        let mut p = r.params.clone();
        let (flow, size) = match water {
            Some((temp, dt)) => {
                let flow = water_flow_m3h(load, dt).map_err(|e| invalid(e.to_string()))?;
                let size =
                    if p.imposed { check_water_pipe(flow, p.dn_mm, temp, &sizing.criteria) } else { size_water_pipe(flow, temp, &sizing.criteria) }
                        .map_err(|e| invalid(e.to_string()))?;
                (flow, size)
            }
            None => {
                let size = size_condensate_drain(load, sizing.drain_slope_percent).map_err(|e| invalid(e.to_string()))?;
                (load / 1000.0, size)
            }
        };
        if !p.imposed {
            p.dn_mm = size.dn_mm;
        } else if water.is_some() && size.velocity_ms > cadcraft_hvac::max_velocity_ms(p.dn_mm) {
            warnings.push(format!("the {} run at ({:.0}, {:.0}) is too small for its flow: {:.2} m/s", p.tag(), r.a.x, r.a.y, size.velocity_ms));
        }
        (p.flow_m3h, p.velocity, p.loss_pa_m) = (Some(flow), Some(size.velocity_ms), water.map(|_| size.gradient_pa_m));
        // Which way the fluid runs along the block's x axis: away from the source for a supply, towards it for a return and a drain.
        let away = service.has_legs() && leg == PipeLeg::Supply;
        p.flow_forward = Some(away == n.at_start);
        results.push(PipeResult {
            handle: r.handle,
            dn_mm: p.dn_mm,
            load,
            flow_m3h: flow,
            velocity_ms: size.velocity_ms,
            gradient_pa_m: size.gradient_pa_m,
            imposed: p.imposed,
            governing: size.governing,
        });
        changed.push((r.handle, p));
    }
    let total_load = total.first().copied().unwrap_or(0.0);
    Ok((PipeNetworkReport { service, leg, source: root_run.handle, total_load, runs: results, warnings }, changed))
}
