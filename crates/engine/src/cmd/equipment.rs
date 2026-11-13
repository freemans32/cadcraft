//! Heating and cooling equipment seen from above: EQUIPMENTADD places a chiller, an outdoor unit or an
//! indoor unit from a library of models (by name, or by power: the smallest that gives it), each value
//! of which can be given instead; EQUIPMENTEDIT changes one already placed, or gives it another model.

use cadcraft_doc::{
    Drawing, EquipmentKind, EquipmentModel, EquipmentParams, HvacUnits, equipment_model_for, equipment_model_named, equipment_models_of,
    equipment_of, standard_equipment_models,
};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::{CommandSpec, bad, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("equipmentadd", "Equipment", run_add)
            .menu(&["Draw", "HVAC", "Equipment"])
            .params(
                "{at, kind? (chiller|vrfOutdoor|splitOutdoor|cassette|ducted|wallUnit|ceilingUnit|floorUnit), model? (a library model's name) | capacity? (kW: the \\
                 smallest model that gives it), name?, reference?, capacityKw?, heatingKw?, length? width? height? (mm), weight? (kg), fans?, clearance? (mm), \\
                 connections?, rotation? (degrees), labelHeight? (mm)} → {handle, name, model} | {list: true, kind?} → {models}",
            )
            .interactive(|s| Ok(Box::new(EquipAddM::new(s)))),
        CommandSpec::new("equipmentedit", "Edit Equipment", run_edit).params(
            "{handle | handles, model? (take that library model's values), kind?, name?, reference?, capacityKw? or null, heatingKw? or null, length? width? height? (mm), \\
             weight? or null, fans?, clearance? (mm), connections?} → {changed}",
        ),
    ]
}

/// What EQUIPMENTADD remembers between uses: the kind and the model.
#[derive(Clone, Debug, PartialEq)]
pub struct EquipmentLastUsed {
    pub kind: EquipmentKind,
    pub model: String,
}

impl Default for EquipmentLastUsed {
    fn default() -> Self {
        EquipmentLastUsed { kind: EquipmentKind::Chiller, model: String::new() }
    }
}

/// Most equipment one EQUIPMENTEDIT changes.
const MAX_EDITED: usize = 10_000;

fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn doc_err(e: cadcraft_doc::DocError) -> EngineError {
    other(e.to_string())
}

fn number(cmd: &str, p: &Value, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|x| x.is_finite()).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a number"))),
    }
}

/// A number, or null to clear it; None when the key is absent.
fn clearable(cmd: &str, p: &Value, key: &str) -> Result<Option<Option<f64>>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(_) => Ok(Some(number(cmd, p, key)?)),
    }
}

fn kind_param(cmd: &str, p: &Value) -> Result<Option<EquipmentKind>> {
    match str_param(p, "kind") {
        None => Ok(None),
        Some(t) => EquipmentKind::parse(t).map(Some).ok_or_else(|| bad(cmd, "unknown `kind`")),
    }
}

fn count(d: &Drawing) -> usize {
    d.model.iter().filter(|e| equipment_of(e).is_some()).count()
}

/// The name a new equipment of `kind` takes: a short code and the next number.
fn next_name(d: &Drawing, kind: EquipmentKind) -> String {
    let code = match kind {
        EquipmentKind::Chiller => "GE",
        EquipmentKind::VrfOutdoor | EquipmentKind::SplitOutdoor => "UE",
        _ => "UI",
    };
    format!("{code}{}", count(d) + 1)
}

/// Everything about the equipment a call asks for, over the model it names (or the one its power asks for).
fn params_from(s: &Session, p: &Value, cmd: &str, last: &EquipmentLastUsed) -> Result<EquipmentParams> {
    let models = standard_equipment_models();
    let d = s.doc()?;
    let kind = kind_param(cmd, p)?;
    let by_name =
        str_param(p, "model").map(|n| equipment_model_named(&models, n).ok_or_else(|| bad(cmd, format!("unknown model \"{n}\"")))).transpose()?;
    let kind_of = by_name.as_ref().map(|m| m.kind).or(kind).unwrap_or(last.kind);
    let model: Option<EquipmentModel> = match (by_name, number(cmd, p, "capacity")?) {
        (Some(m), _) => Some(m),
        (None, Some(kw)) => equipment_model_for(&models, kind_of, kw),
        (None, None) if !last.model.is_empty() && kind.is_none() => equipment_model_named(&models, &last.model),
        (None, None) => equipment_models_of(&models, kind_of).into_iter().next(),
    };
    let units = HvacUnits::of(d);
    let name = str_param(p, "name").map(str::to_string).unwrap_or_else(|| next_name(d, kind_of));
    let mut e = match &model {
        Some(m) => EquipmentParams::from_model(m, &name, units),
        None => return Err(bad(cmd, "no model for that kind")),
    };
    if let Some(r) = str_param(p, "reference") {
        e.reference = r.to_string();
    }
    for (key, slot) in [("length", &mut e.length_mm), ("width", &mut e.width_mm), ("height", &mut e.height_mm)] {
        if let Some(v) = number(cmd, p, key)? {
            *slot = v;
        }
    }
    if let Some(v) = number(cmd, p, "capacityKw")? {
        e.capacity_kw = Some(v);
    }
    if let Some(v) = number(cmd, p, "heatingKw")? {
        e.heating_kw = Some(v);
    }
    if let Some(v) = number(cmd, p, "weight")? {
        e.weight_kg = Some(v);
    }
    if let Some(v) = number(cmd, p, "fans")? {
        e.fans = u32::try_from(v as i64).map_err(|_| bad(cmd, "`fans` must be a whole number"))?;
    }
    if let Some(v) = number(cmd, p, "clearance")? {
        e.clearance_mm = v;
    }
    if let Some(c) = str_param(p, "connections") {
        e.connections = c.to_string();
    }
    if let Some(v) = number(cmd, p, "labelHeight")? {
        e.label_height_mm = v;
    }
    e.validate().map_err(doc_err)?;
    Ok(e)
}

fn models_json(kind: Option<EquipmentKind>) -> Vec<Value> {
    let models = standard_equipment_models();
    EquipmentKind::ALL
        .iter()
        .filter(|k| kind.is_none_or(|w| w == **k))
        .flat_map(|k| equipment_models_of(&models, *k))
        .map(|m| serde_json::to_value(&m).unwrap_or(Value::Null))
        .collect()
}

fn place(s: &mut Session, e: &EquipmentParams, at: Vec2, rotation: f64) -> Result<cadcraft_doc::Handle> {
    let space = s.space();
    let common = s.current_common();
    let h = s.doc_mut()?.add_equipment(&space, common, e, at, rotation).map_err(doc_err)?;
    s.last_used.equipment = EquipmentLastUsed { kind: e.kind, model: e.model.clone() };
    Ok(h)
}

fn run_add(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "equipmentadd";
    if p.get("list").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(json!({ "models": models_json(kind_param(CMD, p)?) }));
    }
    let at = super::point_req(CMD, p, "at")?;
    let last = s.last_used.equipment.clone();
    let e = params_from(s, p, CMD, &last)?;
    let h = place(s, &e, at, super::f64_or(p, "rotation", 0.0).to_radians())?;
    Ok(json!({ "handle": h.hex(), "name": e.name, "model": e.model }))
}

fn run_edit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "equipmentedit";
    let hs = super::arch::handles_param(CMD, p)?;
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} equipment")));
    }
    let models = standard_equipment_models();
    let model =
        str_param(p, "model").map(|n| equipment_model_named(&models, n).ok_or_else(|| bad(CMD, format!("unknown model \"{n}\"")))).transpose()?;
    let kind = kind_param(CMD, p)?;
    let (cap, heat, weight) = (clearable(CMD, p, "capacityKw")?, clearable(CMD, p, "heatingKw")?, clearable(CMD, p, "weight")?);
    let sizes = [number(CMD, p, "length")?, number(CMD, p, "width")?, number(CMD, p, "height")?];
    let (fans, clearance) = (number(CMD, p, "fans")?, number(CMD, p, "clearance")?);
    let d = s.doc()?;
    let mut next = Vec::with_capacity(hs.len());
    for h in &hs {
        let mut e = d.entity(*h).and_then(|x| equipment_of(x)).ok_or_else(|| other("Select equipment only."))?;
        if let Some(m) = &model {
            // The model's values, the name and the units kept.
            e = EquipmentParams { name: e.name.clone(), label_height_mm: e.label_height_mm, ..EquipmentParams::from_model(m, &e.name, e.units) };
        }
        e.kind = kind.unwrap_or(e.kind);
        if let Some(n) = str_param(p, "name") {
            e.name = n.to_string();
        }
        if let Some(r) = str_param(p, "reference") {
            e.reference = r.to_string();
        }
        if let Some(c) = str_param(p, "connections") {
            e.connections = c.to_string();
        }
        if let Some(v) = cap {
            e.capacity_kw = v;
        }
        if let Some(v) = heat {
            e.heating_kw = v;
        }
        if let Some(v) = weight {
            e.weight_kg = v;
        }
        for (v, slot) in sizes.iter().zip([&mut e.length_mm, &mut e.width_mm, &mut e.height_mm]) {
            if let Some(v) = v {
                *slot = *v;
            }
        }
        if let Some(v) = fans {
            e.fans = u32::try_from(v as i64).map_err(|_| bad(CMD, "`fans` must be a whole number"))?;
        }
        if let Some(v) = clearance {
            e.clearance_mm = v;
        }
        e.validate().map_err(doc_err)?;
        next.push((*h, e));
    }
    let d = s.doc_mut()?;
    for (h, e) in &next {
        d.regenerate_equipment(*h, e).map_err(doc_err)?;
    }
    Ok(json!({ "changed": next.len() }))
}

#[derive(Clone, Copy, PartialEq)]
enum Ask {
    Location,
    Type,
    Size,
    Name,
}

struct EquipAddM {
    last: EquipmentLastUsed,
    name: Option<String>,
    ask: Ask,
}

impl EquipAddM {
    fn new(s: &Session) -> Self {
        EquipAddM { last: s.last_used.equipment.clone(), name: None, ask: Ask::Location }
    }
    fn params(&self, s: &Session) -> Result<EquipmentParams> {
        let mut p = json!({ "kind": self.last.kind.name() });
        if !self.last.model.is_empty() {
            p["model"] = json!(self.last.model);
        }
        if let Some(n) = &self.name {
            p["name"] = json!(n);
        }
        params_from(s, &p, "equipmentadd", &self.last)
    }
    fn settings(&self, s: &mut Session) {
        match self.params(s) {
            Ok(e) => s.echo(format!(
                "Current settings: Type = {}, Model = {} ({} x {} x {} mm), Name = {}",
                e.kind.name(),
                e.model,
                e.length_mm,
                e.width_mm,
                e.height_mm,
                e.name
            )),
            Err(e) => s.echo(e.to_string()),
        }
    }
    fn option(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let text = match &i {
            Input::Text(t) | Input::Keyword(t) => Some(t.trim().to_string()),
            _ => None,
        };
        match (self.ask, text.as_deref()) {
            (Ask::Type, Some(t)) => {
                self.last = EquipmentLastUsed {
                    kind: EquipmentKind::parse(t).ok_or_else(|| other("Enter Chiller, Vrv, Split, Cassette, Ducted, Wall, Ceiling or Floor."))?,
                    model: String::new(),
                };
            }
            // A model's name, or a power in kW: the smallest model of the kind that gives it.
            (Ask::Size, Some(t)) if !t.is_empty() => {
                let models = standard_equipment_models();
                let m = match t.parse::<f64>() {
                    Ok(kw) => equipment_model_for(&models, self.last.kind, kw),
                    Err(_) => equipment_model_named(&models, t),
                };
                let m = m.ok_or_else(|| other("Enter a model's name, or a power in kW (? lists the models)."))?;
                self.last = EquipmentLastUsed { kind: m.kind, model: m.name };
            }
            (Ask::Name, Some(t)) if !t.is_empty() => self.name = Some(t.to_string()),
            _ => return Ok(Step::Continue),
        }
        self.ask = Ask::Location;
        self.settings(s);
        Ok(Step::Continue)
    }
}

impl Interactive for EquipAddM {
    fn name(&self) -> &'static str {
        "EQUIPMENTADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            Ask::Type => Prompt::new("Enter equipment type", super::curves::KW)
                .kw(&["Chiller", "Vrv", "Split", "Cassette", "Ducted", "Wall", "Ceiling", "Floor"])
                .default(self.last.kind.name()),
            Ask::Size => Prompt::new("Enter a model name or a power (kW)", Accept::TEXT).default(self.last.model.clone()),
            Ask::Name => Prompt::new("Enter equipment name", Accept::TEXT),
            Ask::Location => Prompt::new("Specify insertion point", Accept::POINT).kw(&["Type", "Size", "Name"]),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.ask != Ask::Location {
            return self.option(s, i);
        }
        match i {
            Input::Keyword(k) => {
                self.ask = match k.as_str() {
                    "Type" => Ask::Type,
                    "Size" => Ask::Size,
                    "Name" => Ask::Name,
                    _ => Ask::Location,
                };
                Ok(Step::Continue)
            }
            // Equipment keeps being placed, each under the next name, until Enter or Esc.
            Input::Point(at) => {
                let e = self.params(s)?;
                place(s, &e, at, 0.0)?;
                self.name = None;
                Ok(Step::Continue)
            }
            Input::Enter | Input::Cancel => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
    fn preview_objects(&self, s: &Session, c: Vec2) -> Vec<cadcraft_doc::Entity> {
        if self.ask != Ask::Location {
            return Vec::new();
        }
        self.params(s).map(|e| e.preview(c, 0.0)).unwrap_or_default()
    }
}
