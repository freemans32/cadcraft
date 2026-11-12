//! A ventilation network fed by one air handling unit: a tree of duct segments, each starting
//! at the unit or at the end of its parent segment, with the terminals (diffusers, grilles) at
//! their ends. Sizing works from the terminals back to the unit: the flow of each segment, its
//! smallest round diameter within the velocity and friction limits, its pressure loss, then the
//! least favoured terminal and the pressure the unit must supply to reach it.

use serde::{Deserialize, Serialize};

use crate::{
    Air, Fitting, FittingCoefficients, HvacError, ROUND_DIAMETERS_MM, Result, checked, equivalent_diameter_mm, pressure_gradient, rect_area_m2,
    velocity,
};

/// Most segments in one network.
pub const MAX_SEGMENTS: usize = 10_000;
/// Most fittings on one segment.
pub const MAX_FITTINGS: usize = 64;
/// Longest segment, m.
const MAX_LENGTH_M: f64 = 10_000.0;
/// Largest flow of one terminal, m³/h.
const MAX_FLOW_M3H: f64 = 1_000_000.0;
/// Largest pressure drop given for one terminal, Pa.
const MAX_TERMINAL_PA: f64 = 10_000.0;

/// Where a segment sits in the network: it sets the velocity limit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SegmentKind {
    /// The trunk leaving the unit.
    #[default]
    Main,
    /// A branch off the trunk serving several terminals.
    Branch,
    /// The last run to one terminal.
    Connection,
}

/// A diffuser or grille at the end of a segment.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Terminal {
    pub flow_m3h: f64,
    /// Pressure drop of the terminal at its flow (from its data sheet), Pa.
    #[serde(default)]
    pub pressure_drop_pa: f64,
}

/// One straight run of duct with its fittings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    /// The segment this one continues from, by index in [`Network::segments`]; `None` when it
    /// leaves the unit.
    pub parent: Option<usize>,
    pub length_m: f64,
    #[serde(default)]
    pub kind: SegmentKind,
    #[serde(default)]
    pub fittings: Vec<Fitting>,
    /// The terminal at the end of this segment, if any.
    #[serde(default)]
    pub terminal: Option<Terminal>,
    /// A diameter the user imposes, mm; sizing then only computes its losses.
    #[serde(default)]
    pub diameter_mm: Option<f64>,
    /// Pressure drop of accessories given in Pa (a silencer's, from its data sheet), added to the
    /// fittings' loss; the loss of an accessory given as a coefficient is a [`Fitting::Custom`].
    #[serde(default)]
    pub fixed_pa: f64,
    /// This run's wall roughness, mm, when it is not the criteria's (a flexible duct is far rougher
    /// than spiral steel).
    #[serde(default)]
    pub roughness_mm: Option<f64>,
    /// The tallest the duct may be, mm, where it runs (the space above a false ceiling less a
    /// margin). A run whose round size would not fit is made rectangular, as tall as this.
    #[serde(default)]
    pub max_height_mm: Option<f64>,
    /// A rectangular section the user imposes: width and height, mm.
    #[serde(default)]
    pub rect_mm: Option<(f64, f64)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Network {
    pub segments: Vec<Segment>,
}

/// The limits a network is sized to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignCriteria {
    pub max_velocity_main: f64,
    pub max_velocity_branch: f64,
    pub max_velocity_connection: f64,
    /// Highest friction loss per metre a chosen diameter may have, Pa/m.
    pub max_gradient_pa_m: f64,
    /// Wall roughness, mm: 0.15 for galvanised steel spiral duct.
    pub roughness_mm: f64,
    pub fittings: FittingCoefficients,
    pub air: Air,
    /// Diameters to choose from, mm, smallest first.
    pub diameters_mm: Vec<f64>,
    /// Rectangular ducts are made in widths and heights that are multiples of this, mm.
    pub side_step_mm: f64,
    /// Smallest and largest side of a rectangular duct, mm.
    pub min_side_mm: f64,
    pub max_side_mm: f64,
    /// Largest ratio of a rectangular duct's width to its height.
    pub max_aspect: f64,
}

impl Default for DesignCriteria {
    fn default() -> Self {
        DesignCriteria {
            max_velocity_main: 6.0,
            max_velocity_branch: 4.0,
            max_velocity_connection: 2.5,
            max_gradient_pa_m: 1.0,
            roughness_mm: 0.15,
            fittings: FittingCoefficients::default(),
            air: Air::default(),
            diameters_mm: ROUND_DIAMETERS_MM.to_vec(),
            side_step_mm: 50.0,
            min_side_mm: 100.0,
            max_side_mm: 2000.0,
            max_aspect: 4.0,
        }
    }
}

impl DesignCriteria {
    fn checked(&self) -> Result<()> {
        checked("main velocity (m/s)", self.max_velocity_main, 0.1, 50.0)?;
        checked("branch velocity (m/s)", self.max_velocity_branch, 0.1, 50.0)?;
        checked("connection velocity (m/s)", self.max_velocity_connection, 0.1, 50.0)?;
        checked("friction limit (Pa/m)", self.max_gradient_pa_m, 0.01, 100.0)?;
        checked("roughness (mm)", self.roughness_mm, 0.0, 10.0)?;
        checked("rectangular side step (mm)", self.side_step_mm, 1.0, 1000.0)?;
        checked("smallest side (mm)", self.min_side_mm, 20.0, 10_000.0)?;
        checked("largest side (mm)", self.max_side_mm, self.min_side_mm, 10_000.0)?;
        checked("largest aspect ratio", self.max_aspect, 1.0, 50.0)?;
        self.fittings.checked()?;
        self.air.checked()?;
        if self.diameters_mm.is_empty() || self.diameters_mm.len() > 1000 {
            return Err(HvacError::BadValue("the diameter list must hold 1 to 1000 sizes".into()));
        }
        let mut last = 0.0;
        for &d in &self.diameters_mm {
            if checked("diameter (mm)", d, 1.0, 10_000.0)? <= last {
                return Err(HvacError::BadValue("diameters must be listed smallest first".into()));
            }
            last = d;
        }
        Ok(())
    }

    fn max_velocity(&self, kind: SegmentKind) -> f64 {
        match kind {
            SegmentKind::Main => self.max_velocity_main,
            SegmentKind::Branch => self.max_velocity_branch,
            SegmentKind::Connection => self.max_velocity_connection,
        }
    }
}

/// The shape of a duct's section.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Section {
    #[default]
    Round,
    Rectangular,
}

/// What set a segment's diameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Governing {
    /// The velocity limit (the friction limit needs this size or less).
    Velocity,
    /// The friction limit needs a larger size than the velocity limit.
    Friction,
    /// The user imposed it.
    Imposed,
    /// No listed size meets the limits: the largest is used and a warning says so.
    Largest,
}

/// The sizing of one segment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentResult {
    pub flow_m3h: f64,
    /// Round or rectangular.
    pub section: Section,
    /// The diameter of a round duct; for a rectangular one, the equivalent diameter that loses the
    /// same pressure per metre.
    pub diameter_mm: f64,
    /// The sides of the section, mm: both the diameter for a round duct.
    pub width_mm: f64,
    pub height_mm: f64,
    pub governing: Governing,
    pub velocity: f64,
    pub gradient_pa_m: f64,
    pub friction_pa: f64,
    pub fittings_pa: f64,
    pub terminal_pa: f64,
    /// Friction + fittings + terminal, Pa.
    pub total_pa: f64,
}

/// The sizing of a whole network.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkReport {
    /// One result per segment, in the network's order.
    pub segments: Vec<SegmentResult>,
    /// The segments from the unit to the least favoured terminal.
    pub critical_path: Vec<usize>,
    /// Pressure the unit must supply to the network, Pa (the critical path's losses).
    pub required_pressure_pa: f64,
    /// The unit's total flow, m³/h.
    pub total_flow_m3h: f64,
    pub warnings: Vec<String>,
}

/// Each segment's depth (0 when it leaves the unit), refusing a bad parent index or a loop.
/// Each segment is walked once: the walk up stops at the first segment whose depth is known.
fn depths(net: &Network) -> Result<Vec<usize>> {
    let n = net.segments.len();
    let mut depth: Vec<Option<usize>> = vec![None; n];
    let mut on_walk = vec![false; n];
    let mut walk = Vec::new();
    for i in 0..n {
        let mut at = i;
        let base = loop {
            if let Some(d) = depth.get(at).copied().flatten() {
                break d + 1;
            }
            if on_walk.get(at).copied().unwrap_or(false) {
                return Err(HvacError::BadNetwork(format!("segment {at} is part of a loop")));
            }
            if let Some(w) = on_walk.get_mut(at) {
                *w = true;
            }
            walk.push(at);
            match net.segments.get(at).and_then(|s| s.parent) {
                None => break 0,
                Some(p) if p >= n => {
                    return Err(HvacError::BadNetwork(format!("segment {at} continues from segment {p}, which does not exist")));
                }
                Some(p) => at = p,
            }
        };
        // The walk went from `i` up; the last segment walked sits at `base`.
        for (k, &s) in walk.iter().rev().enumerate() {
            if let Some(d) = depth.get_mut(s) {
                *d = Some(base + k);
            }
        }
        walk.clear();
    }
    Ok(depth.into_iter().map(|d| d.unwrap_or(0)).collect())
}

/// Each segment's flow: its terminal's plus every segment downstream.
fn flows(net: &Network, depth: &[usize]) -> Result<Vec<f64>> {
    let mut flow = vec![0.0; net.segments.len()];
    for (i, s) in net.segments.iter().enumerate() {
        checked(&format!("length of segment {i} (m)"), s.length_m, 0.0, MAX_LENGTH_M)?;
        checked(&format!("accessory pressure drop of segment {i} (Pa)"), s.fixed_pa, 0.0, MAX_TERMINAL_PA)?;
        if s.fittings.len() > MAX_FITTINGS {
            return Err(HvacError::BadValue(format!("segment {i} has more than {MAX_FITTINGS} fittings")));
        }
        if let Some(t) = s.terminal {
            if let Some(f) = flow.get_mut(i) {
                *f = checked(&format!("terminal flow of segment {i} (m³/h)"), t.flow_m3h, 0.0, MAX_FLOW_M3H)?;
            }
            checked(&format!("terminal pressure drop of segment {i} (Pa)"), t.pressure_drop_pa, 0.0, MAX_TERMINAL_PA)?;
        }
    }
    // Deepest first, so each segment's flow is complete before it is added to its parent's.
    let mut order: Vec<usize> = (0..net.segments.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(depth.get(i).copied().unwrap_or(0)));
    for i in order {
        let own = flow.get(i).copied().unwrap_or(0.0);
        if let Some(p) = net.segments.get(i).and_then(|s| s.parent)
            && let Some(f) = flow.get_mut(p)
        {
            *f += own;
        }
    }
    for (i, &f) in flow.iter().enumerate() {
        if f <= 0.0 {
            return Err(HvacError::BadNetwork(format!("segment {i} serves no terminal with a flow")));
        }
    }
    Ok(flow)
}

/// The smallest listed diameter within both limits for `flow` in a segment of `kind`.
fn choose_diameter(flow: f64, kind: SegmentKind, roughness_mm: f64, c: &DesignCriteria) -> Result<(f64, Governing)> {
    let vmax = c.max_velocity(kind);
    let mut by_velocity = None;
    for &d in &c.diameters_mm {
        let v_ok = velocity(flow, d) <= vmax;
        if v_ok && by_velocity.is_none() {
            by_velocity = Some(d);
        }
        if v_ok && pressure_gradient(flow, d, roughness_mm, &c.air)? <= c.max_gradient_pa_m {
            let governing = if Some(d) == by_velocity { Governing::Velocity } else { Governing::Friction };
            return Ok((d, governing));
        }
    }
    let largest = c.diameters_mm.last().copied().unwrap_or(1000.0);
    Ok((largest, Governing::Largest))
}

/// A size chosen for a run: round (its diameter) or rectangular (width, height).
#[derive(Clone, Copy, Debug)]
enum Chosen {
    Round(f64),
    Rect(f64, f64),
}

/// The smallest rectangular section as tall as `height_mm` (to the step) within the limits for `flow`,
/// preferring a width not below its height; the largest allowed if none meets them.
fn choose_rect(flow: f64, kind: SegmentKind, roughness_mm: f64, height_mm: f64, c: &DesignCriteria) -> Result<(Chosen, Governing)> {
    let step = c.side_step_mm;
    let h = (height_mm / step).floor() * step;
    if h < c.min_side_mm {
        return Err(HvacError::BadValue(format!(
            "the height available, {height_mm:.0} mm, is below the smallest duct side, {:.0} mm",
            c.min_side_mm
        )));
    }
    let vmax = c.max_velocity(kind);
    let widest = c.max_side_mm.min((h * c.max_aspect / step).floor() * step);
    let mut w = h;
    let mut by_velocity = false;
    while w <= widest {
        let v_ok = flow / 3600.0 / rect_area_m2(w, h) <= vmax;
        if v_ok && !by_velocity {
            by_velocity = true;
            if pressure_gradient(flow, equivalent_diameter_mm(w, h), roughness_mm, &c.air)? <= c.max_gradient_pa_m {
                return Ok((Chosen::Rect(w, h), Governing::Velocity));
            }
        } else if v_ok && pressure_gradient(flow, equivalent_diameter_mm(w, h), roughness_mm, &c.air)? <= c.max_gradient_pa_m {
            return Ok((Chosen::Rect(w, h), Governing::Friction));
        }
        w += step;
    }
    Ok((Chosen::Rect(widest.max(h), h), Governing::Largest))
}

fn size_segment(i: usize, s: &Segment, flow: f64, c: &DesignCriteria, warnings: &mut Vec<String>) -> Result<SegmentResult> {
    let roughness_mm = checked(&format!("roughness of segment {i} (mm)"), s.roughness_mm.unwrap_or(c.roughness_mm), 0.0, 10.0)?;
    let (chosen, governing) = match (s.rect_mm, s.diameter_mm) {
        (Some((w, h)), _) => (
            Chosen::Rect(
                checked(&format!("imposed width of segment {i} (mm)"), w, 1.0, 10_000.0)?,
                checked(&format!("imposed height of segment {i} (mm)"), h, 1.0, 10_000.0)?,
            ),
            Governing::Imposed,
        ),
        (None, Some(d)) => (Chosen::Round(checked(&format!("imposed diameter of segment {i} (mm)"), d, 1.0, 10_000.0)?), Governing::Imposed),
        (None, None) => {
            let (d, g) = choose_diameter(flow, s.kind, roughness_mm, c)?;
            match s.max_height_mm {
                Some(h) if d > checked(&format!("available height of segment {i} (mm)"), h, 1.0, 10_000.0)? => {
                    choose_rect(flow, s.kind, roughness_mm, h, c)?
                }
                _ => (Chosen::Round(d), g),
            }
        }
    };
    let (section, diameter_mm, width_mm, height_mm, area) = match chosen {
        Chosen::Round(d) => (Section::Round, d, d, d, crate::round_area_m2(d)),
        Chosen::Rect(w, h) => (Section::Rectangular, equivalent_diameter_mm(w, h), w, h, rect_area_m2(w, h)),
    };
    let v = flow / 3600.0 / area;
    // The friction of a rectangular duct is that of its equivalent round one at the same flow.
    let gradient_pa_m = pressure_gradient(flow, diameter_mm, roughness_mm, &c.air)?;
    let mut zeta = 0.0;
    for &f in &s.fittings {
        zeta += c.fittings.zeta(f)?;
    }
    let size = match section {
        Section::Round => format!("Ø{diameter_mm:.0}"),
        Section::Rectangular => format!("{width_mm:.0}×{height_mm:.0}"),
    };
    if governing == Governing::Largest {
        warnings.push(format!("segment {i}: {flow:.0} m³/h exceeds the limits even at {size}"));
    } else if v > c.max_velocity(s.kind) {
        warnings.push(format!("segment {i}: {v:.1} m/s at the imposed {size} is above the limit"));
    }
    let friction_pa = gradient_pa_m * s.length_m;
    let fittings_pa = zeta * c.air.dynamic_pressure(v) + s.fixed_pa;
    let terminal_pa = s.terminal.map(|t| t.pressure_drop_pa).unwrap_or(0.0);
    Ok(SegmentResult {
        flow_m3h: flow,
        section,
        diameter_mm,
        width_mm,
        height_mm,
        governing,
        velocity: v,
        gradient_pa_m,
        friction_pa,
        fittings_pa,
        terminal_pa,
        total_pa: friction_pa + fittings_pa + terminal_pa,
    })
}

/// The path from the unit to the terminal needing the most pressure, and that pressure.
fn critical_path(net: &Network, results: &[SegmentResult]) -> (Vec<usize>, f64) {
    let mut best: (Vec<usize>, f64) = (Vec::new(), 0.0);
    for (i, s) in net.segments.iter().enumerate() {
        if s.terminal.is_none() {
            continue;
        }
        // The terminal's own loss counts; the terminal losses of segments passed on the way
        // (a diffuser part-way along a run) do not, as that air leaves before.
        let mut path = vec![i];
        let mut pressure = results.get(i).map(|r| r.total_pa).unwrap_or(0.0);
        let mut at = s.parent;
        while let Some(p) = at {
            if path.len() > net.segments.len() {
                break;
            }
            path.push(p);
            pressure += results.get(p).map(|r| r.total_pa - r.terminal_pa).unwrap_or(0.0);
            at = net.segments.get(p).and_then(|s| s.parent);
        }
        if pressure > best.1 || best.0.is_empty() {
            path.reverse();
            best = (path, pressure);
        }
    }
    best
}

/// Size every segment of `net` to `criteria`.
pub fn size_network(net: &Network, criteria: &DesignCriteria) -> Result<NetworkReport> {
    criteria.checked()?;
    if net.segments.is_empty() {
        return Err(HvacError::BadNetwork("the network has no segment".into()));
    }
    if net.segments.len() > MAX_SEGMENTS {
        return Err(HvacError::BadNetwork(format!("more than {MAX_SEGMENTS} segments")));
    }
    let depth = depths(net)?;
    let flow = flows(net, &depth)?;
    let mut warnings = Vec::new();
    let mut segments = Vec::with_capacity(net.segments.len());
    for (i, s) in net.segments.iter().enumerate() {
        let f = flow.get(i).copied().unwrap_or(0.0);
        segments.push(size_segment(i, s, f, criteria, &mut warnings)?);
    }
    let (critical_path, required_pressure_pa) = critical_path(net, &segments);
    let total_flow_m3h = net.segments.iter().zip(&flow).filter(|(s, _)| s.parent.is_none()).map(|(_, f)| *f).sum();
    Ok(NetworkReport { segments, critical_path, required_pressure_pa, total_flow_m3h, warnings })
}
