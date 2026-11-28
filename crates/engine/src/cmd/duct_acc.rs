//! Duct accessories: DUCTACCESSORYADD places a damper, fire damper, silencer, flexible connector or hood
//! on a duct (it takes the duct's diameter and direction; a hood goes on the end of a duct or on a unit's collar), DUCTACCESSORYEDIT changes them. Their
//! losses are added to the duct they sit on when DUCTSIZE sizes the network.

use cadcraft_doc::{AccessoryKind, AccessoryParams, EntityKind, Handle, HvacUnits};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::{CommandSpec, bad, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("ductaccessoryadd", "Duct Accessory", run_add)
            .menu(&["Draw", "HVAC", "Duct Accessory"])
            .params(
                "{at (on a duct), type?: damper|fireDamper|silencer|flexConnector|hood (a hood takes the unit collar or the duct end nearest `at`, else stays where it is put), length? (mm), zeta?, pressureDrop? (Pa, wins over zeta), \
                 labelHeight? (mm), model? (a hood's: standard|birdMesh|gooseneck|roofCap)} → {handle, duct, type}",
            )
            .interactive(|s| Ok(Box::new(AccessoryAddM::new(s)))),
        CommandSpec::new("ductaccessoryedit", "Edit Duct Accessory", run_edit)
            .params("{handle | handles, type?, length? (mm), width? (mm), height? (mm, 0 or null: round), overhang? (mm, a hood's casing beyond its opening; 0 or null: none), air? (supply|extract|fresh|exhaust, or null; for a hood: its colour and the takeoff), follow? (bool: a hood follows the size and air of the duct or collar it covers until one of those is set by hand), zeta? (number, or null for the default), pressureDrop? (Pa, or null), model? (a hood's: standard|birdMesh|gooseneck|roofCap)} → {changed}"),
    ]
}

/// What DUCTACCESSORYADD remembers between uses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccessoryLastUsed {
    pub kind: AccessoryKind,
}

/// Most accessories one DUCTACCESSORYEDIT changes.
const MAX_EDITED: usize = 10_000;

fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn doc_err(e: cadcraft_doc::DocError) -> EngineError {
    other(e.to_string())
}

fn kind_param(cmd: &str, p: &Value) -> Result<Option<AccessoryKind>> {
    match str_param(p, "type") {
        Some(t) => AccessoryKind::parse(t).map(Some).ok_or_else(|| bad(cmd, "`type` must be damper, fireDamper, silencer, flexConnector or hood")),
        None => Ok(None),
    }
}

fn number(cmd: &str, p: &Value, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            v.as_f64().filter(|x| x.is_finite() && *x >= 0.0).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a number, 0 or more")))
        }
    }
}

/// The accessory of `kind` for the duct under `at`: its parameters, the duct (none for a hood on a unit's collar or free), the foot of `at` on
/// the duct's axis and the duct's direction. `loss` is (length, zeta, pressure drop, label height).
fn on_duct(
    s: &Session,
    kind: AccessoryKind,
    at: Vec2,
    loss: (Option<f64>, Option<f64>, Option<f64>, f64),
) -> Result<(AccessoryParams, Option<Handle>, Vec2, f64)> {
    let (length, zeta, pa, label_mm) = loss;
    let d = s.doc()?;
    let (duct, foot, angle, width, height, air) = if kind == AccessoryKind::Hood {
        // On the end of a duct or a unit's collar, pointing out; else free at `at` (turn it afterwards).
        match d.hood_anchor(at) {
            Some(a) => (a.duct, a.origin, a.angle, a.width_mm, a.height_mm, a.air),
            None => (None, at, 0.0, 400.0, None, None),
        }
    } else {
        let (duct, foot, angle) = d.duct_under(at).ok_or_else(|| other("Place the accessory on a duct."))?;
        let (dp, ..) = d.duct_ends(duct).ok_or_else(|| other("Place the accessory on a duct."))?;
        (Some(duct), foot, angle, dp.plan_width_mm(), None, None)
    };
    let p = AccessoryParams {
        kind,
        length_mm: length.unwrap_or_else(|| kind.default_length_mm()),
        diameter_mm: width,
        zeta,
        pressure_drop_pa: pa,
        label_height_mm: label_mm,
        bottom_mm: None,
        units: HvacUnits::of(d),
        air,
        height_mm: height,
        fixed: false,
        overhang_mm: None,
        hood_model: Default::default(),
    };
    p.validate().map_err(doc_err)?;
    Ok((p, duct, foot, angle))
}

/// Places `kind` on the duct under `at`; returns the accessory and the duct.
fn place(s: &mut Session, kind: AccessoryKind, at: Vec2, loss: (Option<f64>, Option<f64>, Option<f64>, f64)) -> Result<(Handle, Option<Handle>)> {
    let (p, duct, foot, angle) = on_duct(s, kind, at, loss)?;
    let space = s.space();
    let common = s.current_common();
    let h = s.doc_mut()?.add_accessory(&space, common, &p, foot, angle).map_err(doc_err)?;
    Ok((h, duct))
}

/// A hood's model, when one is given.
fn model_param(cmd: &str, p: &Value) -> Result<Option<cadcraft_doc::HoodModel>> {
    match p.get("model") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .and_then(cadcraft_doc::HoodModel::parse)
            .map(Some)
            .ok_or_else(|| bad(cmd, "`model` must be standard, birdMesh, gooseneck or roofCap")),
    }
}

fn run_add(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ductaccessoryadd";
    let at = super::point_req(CMD, p, "at")?;
    let kind = kind_param(CMD, p)?.unwrap_or_default();
    let label = number(CMD, p, "labelHeight")?.unwrap_or(100.0);
    let model = model_param(CMD, p)?;
    let (h, duct) = place(s, kind, at, (number(CMD, p, "length")?, number(CMD, p, "zeta")?, number(CMD, p, "pressureDrop")?, label))?;
    if let Some(m) = model.filter(|_| kind == AccessoryKind::Hood) {
        let d = s.doc_mut()?;
        if let Some(mut a) = d.entity(h).and_then(|e| cadcraft_doc::accessory_of(e)) {
            a.hood_model = m;
            d.regenerate_accessory(h, &a).map_err(doc_err)?;
        }
    }
    Ok(json!({ "handle": h.hex(), "duct": duct.map(|d| d.hex()), "type": kind.name() }))
}

fn run_edit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ductaccessoryedit";
    let hs = super::arch::handles_param(CMD, p)?;
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} accessories")));
    }
    let kind = kind_param(CMD, p)?;
    let length = number(CMD, p, "length")?;
    // A hood's size and air: `width`, `height` (0 or null: round), `air` (null: none).
    let width = number(CMD, p, "width")?;
    let height = number(CMD, p, "height")?;
    let overhang = number(CMD, p, "overhang")?;
    let model = model_param(CMD, p)?;
    let air = match p.get("air") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(
            v.as_str().and_then(cadcraft_doc::AirDirection::parse).ok_or_else(|| bad(CMD, "`air` must be supply, extract, fresh or exhaust"))?,
        )),
    };
    // A key given as null clears the value, so the kind's default applies again.
    let (zeta, pa) = (number(CMD, p, "zeta")?, number(CMD, p, "pressureDrop")?);
    let (clear_zeta, clear_pa) = (p.get("zeta").is_some_and(Value::is_null), p.get("pressureDrop").is_some_and(Value::is_null));
    let d = s.doc()?;
    let mut next = Vec::with_capacity(hs.len());
    for h in &hs {
        let mut a = d.entity(*h).and_then(|e| cadcraft_doc::accessory_of(e)).ok_or_else(|| other("Select duct accessories only."))?;
        a.kind = kind.unwrap_or(a.kind);
        a.length_mm = length.unwrap_or(a.length_mm);
        if let Some(w) = width {
            a.diameter_mm = w;
        }
        if p.get("height").is_some() {
            a.height_mm = height.filter(|h| *h > 0.0);
        }
        if let Some(x) = air {
            a.air = x;
        }
        if p.get("overhang").is_some() {
            a.overhang_mm = overhang.filter(|o| *o > 0.0);
        }
        if let Some(m) = model {
            a.hood_model = m;
        }
        // Set by hand, a hood stops following its opening; `follow: true` makes it follow again (at the next sizing or duct change).
        if width.is_some() || p.get("height").is_some() || air.is_some() {
            a.fixed = true;
        }
        if let Some(f) = p.get("follow").and_then(Value::as_bool) {
            a.fixed = !f;
        }
        if let Some(z) = zeta {
            a.zeta = Some(z);
        }
        if let Some(x) = pa {
            a.pressure_drop_pa = Some(x);
        }
        if clear_zeta {
            a.zeta = None;
        }
        if clear_pa {
            a.pressure_drop_pa = None;
        }
        if let Some(v) = super::altitude_param(CMD, p)? {
            a.bottom_mm = v;
        }
        a.validate().map_err(doc_err)?;
        next.push((*h, a));
    }
    let d = s.doc_mut()?;
    for (h, a) in &next {
        d.regenerate_accessory(*h, a).map_err(doc_err)?;
    }
    // A hood given back to its opening takes its size and air now.
    d.refresh_hoods().map_err(doc_err)?;
    Ok(json!({ "changed": next.len() }))
}

#[derive(Clone, Copy, PartialEq)]
enum Ask {
    Location,
    Length,
}

struct AccessoryAddM {
    kind: AccessoryKind,
    length: Option<f64>,
    ask: Ask,
}

impl AccessoryAddM {
    fn new(s: &Session) -> Self {
        AccessoryAddM { kind: s.last_used.accessory.kind, length: None, ask: Ask::Location }
    }
    fn settings(&self, s: &mut Session) {
        let length = self.length.unwrap_or_else(|| self.kind.default_length_mm());
        s.echo(format!("Current settings: Type = {}, Length = {length}", self.kind.name()));
    }
}

impl Interactive for AccessoryAddM {
    fn name(&self) -> &'static str {
        "DUCTACCESSORYADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            Ask::Location => {
                Prompt::new("Specify location of accessory on a duct", Accept::POINT).kw(&["Damper", "Fire", "Silencer", "Flex", "Hood", "Length"])
            }
            Ask::Length => Prompt::new("Enter accessory length (mm)", Accept::NUMBER)
                .default(format!("{}", self.length.unwrap_or_else(|| self.kind.default_length_mm()))),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.ask == Ask::Length {
            if let Input::Text(t) | Input::Keyword(t) = &i {
                self.length = Some(
                    t.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0).ok_or_else(|| other("Requires a length in millimetres."))?,
                );
            }
            self.ask = Ask::Location;
            self.settings(s);
            return Ok(Step::Continue);
        }
        match i {
            Input::Keyword(k) => {
                match k.as_str() {
                    "Length" => self.ask = Ask::Length,
                    other_kind => {
                        if let Some(kind) = AccessoryKind::parse(other_kind) {
                            self.kind = kind;
                            self.length = None;
                            self.settings(s);
                        }
                    }
                }
                Ok(Step::Continue)
            }
            Input::Point(at) => {
                match place(s, self.kind, at, (self.length, None, None, 100.0)) {
                    Ok(_) => s.last_used.accessory = AccessoryLastUsed { kind: self.kind },
                    Err(e) => s.echo(e.to_string()),
                }
                Ok(Step::Continue)
            }
            Input::Enter | Input::Cancel => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, _c: Vec2) -> Vec<EntityKind> {
        Vec::new()
    }
    fn preview_objects(&self, s: &Session, c: Vec2) -> Vec<cadcraft_doc::Entity> {
        // On the duct under the cursor, across it, before the click.
        match (self.ask, on_duct(s, self.kind, c, (self.length, None, None, 100.0))) {
            (Ask::Location, Ok((p, _, foot, angle))) => p.preview(foot, angle),
            _ => Vec::new(),
        }
    }
}
