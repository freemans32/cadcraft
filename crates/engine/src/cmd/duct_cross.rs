//! DUCTCROSS: where two ducts cross, the one that passes under is drawn with its sides broken across
//! the other. With no ducts named it lists the crossings; DUCTSIZE lists the ones still to draw.

use cadcraft_doc::{Drawing, DuctCrossing, Handle};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::{CommandSpec, bad};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("ductcross", "Duct Crossing", run)
            .menu(&["Draw", "HVAC", "Duct Crossing"])
            .params(
                "{under (the duct that passes under), over (the one that passes over), clearance? (mm kept on each side of it, default 30), \
                 jog? (under|over: that duct makes the four 45° elbows to rise or dip round the other), shift? (mm, the rise; default clears the other duct by 50) | \\
                 clear: handle(s) (draw them whole again) | none: list the crossings} → {gap: [from, to] (mm from the start of `under`)} | {crossings}",
            )
            .interactive(|_| Ok(Box::new(CrossM { ask: Ask::Under, under: None, over: None }))),
    ]
}

/// Default clearance on each side of the duct that passes over, mm.
const CLEARANCE_MM: f64 = 30.0;

fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn doc_err(e: cadcraft_doc::DocError) -> EngineError {
    other(e.to_string())
}

/// The crossings as JSON: the two ducts, where, and whether one is already drawn broken there.
pub(super) fn crossings_json(c: &[DuctCrossing]) -> Vec<Value> {
    c.iter().map(|x| json!({ "a": x.a.hex(), "b": x.b.hex(), "at": [x.at.x, x.at.y], "resolved": x.resolved })).collect()
}

fn handle(cmd: &str, p: &Value, key: &str) -> Result<Option<Handle>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_str().and_then(Handle::parse_hex).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a handle"))),
    }
}

fn clear(s: &mut Session, hs: &[Handle]) -> Result<usize> {
    let d = s.doc()?;
    let mut next = Vec::new();
    for h in hs {
        let (mut p, ..) = d.duct_ends(*h).ok_or_else(|| other("Select ducts only."))?;
        p.gaps_mm.clear();
        p.jogs.clear();
        next.push((*h, p));
    }
    let d = s.doc_mut()?;
    for (h, p) in &next {
        d.regenerate_duct(*h, p).map_err(doc_err)?;
    }
    Ok(next.len())
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ductcross";
    if p.get("clear").is_some() {
        let hs = super::arch::handles_param(CMD, &json!({ "handles": p["clear"] }))?;
        return Ok(json!({ "cleared": clear(s, &hs)? }));
    }
    let (under, over) = (handle(CMD, p, "under")?, handle(CMD, p, "over")?);
    let (Some(under), Some(over)) = (under, over) else {
        return Ok(json!({ "crossings": crossings_json(&s.doc()?.duct_crossings()) }));
    };
    let clearance =
        p.get("clearance").map_or(Some(CLEARANCE_MM), Value::as_f64).ok_or_else(|| bad(CMD, "`clearance` must be a number of millimetres"))?;
    let way = match p.get("jog").and_then(Value::as_str).map(|t| t.trim().to_ascii_lowercase()).as_deref() {
        None | Some("" | "none" | "no") => None,
        Some("under") => Some(true),
        Some("over") => Some(false),
        Some(_) => return Err(bad(CMD, "`jog` is under, over or none")),
    };
    let shift = match p.get("shift") {
        None | Some(Value::Null) => None,
        Some(v) => {
            Some(v.as_f64().filter(|x| x.is_finite() && *x > 0.0).ok_or_else(|| bad(CMD, "`shift` must be a positive number of millimetres"))?)
        }
    };
    let d = s.doc_mut()?;
    let gap = d.pass_duct_under(under, over, clearance).map_err(doc_err)?;
    // The duct that makes the elbows is the one named; it goes over the other, or dips under it.
    let jog = match way {
        Some(down) => {
            let (duct, other) = if down { (under, over) } else { (over, under) };
            Some(d.jog_duct_around(duct, other, down, shift, clearance).map_err(doc_err)?)
        }
        None => None,
    };
    Ok(json!({ "gap": gap, "jog": jog.map(|j| json!({ "elbows": j.elbows_mm(), "shift": j.shift_mm, "down": j.down })) }))
}

#[derive(Clone, Copy, PartialEq)]
enum Ask {
    Under,
    Over,
    Jog,
    Clear,
}

struct CrossM {
    ask: Ask,
    under: Option<Handle>,
    over: Option<Handle>,
}

fn is_duct(d: &Drawing, h: Handle) -> bool {
    d.duct_ends(h).is_some()
}

impl Interactive for CrossM {
    fn name(&self) -> &'static str {
        "DUCTCROSS"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            Ask::Under => Prompt::new("Select the duct that passes under or [List/Clear]", Accept::SELECT).kw(&["List", "Clear"]),
            Ask::Over => Prompt::new("Select the duct that passes over", Accept::SELECT),
            Ask::Jog => Prompt::new("Which duct makes the four 45° elbows? [Over/Under/None]", Accept::TEXT)
                .kw(&["Over", "Under", "None"])
                .default("None".to_string()),
            Ask::Clear => Prompt::new("Select ducts to draw whole again", Accept::SELECT),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match (self.ask, i) {
            (Ask::Under, Input::Keyword(k)) if k == "List" => {
                let list = s.doc()?.duct_crossings();
                let todo = list.iter().filter(|c| !c.resolved).count();
                s.echo(format!("{} crossing(s), {todo} still to draw.", list.len()));
                for c in list.iter().filter(|c| !c.resolved).take(20) {
                    s.echo(format!("  at ({:.0}, {:.0}): ducts {} and {}", c.at.x, c.at.y, c.a.hex(), c.b.hex()));
                }
                Ok(Step::Continue)
            }
            (Ask::Under, Input::Keyword(k)) if k == "Clear" => {
                self.ask = Ask::Clear;
                Ok(Step::Continue)
            }
            (Ask::Under, Input::Pick(hs)) => {
                match hs.iter().copied().find(|h| s.doc().is_ok_and(|d| is_duct(d, *h))) {
                    Some(h) => {
                        self.under = Some(h);
                        self.ask = Ask::Over;
                    }
                    None => s.echo("Select a duct."),
                }
                Ok(Step::Continue)
            }
            (Ask::Over, Input::Pick(hs)) => {
                let (Some(under), Some(over)) = (self.under, hs.iter().copied().find(|h| s.doc().is_ok_and(|d| is_duct(d, *h)))) else {
                    s.echo("Select a duct.");
                    return Ok(Step::Continue);
                };
                match s.doc_mut()?.pass_duct_under(under, over, CLEARANCE_MM) {
                    Ok(g) => {
                        s.echo(format!("Drawn broken over {:.0} mm.", g[1] - g[0]));
                        self.over = Some(over);
                        self.ask = Ask::Jog;
                        Ok(Step::Continue)
                    }
                    Err(e) => {
                        s.echo(e.to_string());
                        Ok(Step::Continue)
                    }
                }
            }
            (Ask::Jog, Input::Keyword(k) | Input::Text(k)) => {
                let (Some(under), Some(over)) = (self.under, self.over) else { return Ok(Step::Cancel) };
                let down = match k.trim().to_ascii_lowercase().as_str() {
                    "under" | "u" => true,
                    "over" | "o" => false,
                    _ => return Ok(Step::Done),
                };
                let (duct, other) = if down { (under, over) } else { (over, under) };
                match s.doc_mut()?.jog_duct_around(duct, other, down, None, CLEARANCE_MM) {
                    Ok(j) => {
                        s.echo(format!("Four 45° elbows, {:.0} mm {}.", j.shift_mm, if down { "down" } else { "up" }));
                        Ok(Step::Done)
                    }
                    Err(e) => {
                        s.echo(e.to_string());
                        Ok(Step::Continue)
                    }
                }
            }
            (Ask::Jog, Input::Enter) => Ok(Step::Done),
            (Ask::Clear, Input::Pick(hs)) => {
                let n = clear(s, &hs.iter().copied().filter(|h| s.doc().is_ok_and(|d| is_duct(d, *h))).collect::<Vec<_>>())?;
                s.echo(format!("{n} duct(s) drawn whole."));
                Ok(Step::Done)
            }
            (_, Input::Enter | Input::Cancel) => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, _c: Vec2) -> Vec<cadcraft_doc::EntityKind> {
        Vec::new()
    }
}
