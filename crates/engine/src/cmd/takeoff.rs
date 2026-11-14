//! HVACTAKEOFF: the quantities of one level, each drawing of a building being a floor: ducts by
//! direction, section and altitude (length and surface), fittings, accessories, terminals, units,
//! risers, equipment and, by space, what it holds. As JSON, and as a `;`-separated file a spreadsheet opens.

use serde_json::{Value, json};

use cadcraft_doc::{CalcNote, NoteNetwork, NoteSegment, ahu_of, riser_of};
use cadcraft_hvac::{Governing, Section};

use super::{CommandSpec, bad, bool_or, str_param};

use crate::{EngineError, Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("hvactakeoff", "HVAC Quantities", run)
            .menu(&["Draw", "HVAC", "Quantities"])
            .params(
                "{level? (the name of the floor, written on the result), path? (a file to write: .xlsx for an Excel workbook with the calculation note, \\
                 else .csv with `;` between cells), decimal?: comma|point (the decimal mark of a .csv, default comma), lang?: fr|en (the labels of an .xlsx, default en), \\
                 note? (add the calculation note: default true for an .xlsx), + the sizing criteria of ductsize} → {level, ducts, fittings, accessories, terminals, units, \\
                 risers, equipment, spaces, totals, note?, path?}",
            ),
    ]
}

fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "hvactakeoff";
    let level = str_param(p, "level").unwrap_or("");
    if level.chars().count() > 64 || level.chars().any(char::is_control) {
        return Err(bad(CMD, "`level` has at most 64 characters, without control characters"));
    }
    let comma = match str_param(p, "decimal").map(|t| t.trim().to_ascii_lowercase()).as_deref() {
        None | Some("comma") => true,
        Some("point") => false,
        Some(_) => return Err(bad(CMD, "`decimal` is comma or point")),
    };
    let t = s.doc()?.takeoff(level).map_err(|e| other(e.to_string()))?;
    let mut out = serde_json::to_value(&t).map_err(|e| other(e.to_string()))?;
    let path = str_param(p, "path").map(str::trim).filter(|x| !x.is_empty());
    let xlsx = path.is_some_and(|x| x.to_ascii_lowercase().ends_with(".xlsx"));
    let lang = str_param(p, "lang").unwrap_or("en");
    if lang.len() > 16 || lang.chars().any(char::is_control) {
        return Err(bad(CMD, "`lang` is a language code such as fr or en"));
    }
    let note = if bool_or(p, "note", xlsx) { Some(calc_note(s, p)?) } else { None };
    if let Some(n) = &note {
        out["note"] = serde_json::to_value(n).map_err(|e| other(e.to_string()))?;
    }
    if let Some(path) = path {
        if path.len() > 1024 || path.contains('\0') {
            return Err(bad(CMD, "`path` is not a usable file name"));
        }
        let path = std::path::Path::new(path);
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| other(format!("Cannot write {}: {e}", path.display())))?;
        }
        let bytes = if xlsx {
            t.to_xlsx(note.as_ref(), lang).map_err(|e| other(e.to_string()))?
        } else {
            // A byte order mark, so a spreadsheet reads the accents as UTF-8.
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(t.to_csv(comma).as_bytes());
            bytes
        };
        std::fs::write(path, bytes).map_err(|e| other(format!("Cannot write {}: {e}", path.display())))?;
        out["path"] = json!(path.display().to_string());
    }
    Ok(out)
}

fn r(v: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (v * k).round() / k
}

/// The calculation of each network as sizing computes it now (the same criteria and the same reducers),
/// without changing the drawing.
fn calc_note(s: &Session, p: &Value) -> Result<CalcNote> {
    let criteria = super::duct::criteria(p)?;
    let d = s.doc()?;
    let mut note = CalcNote::default();
    for net in d.duct_networks().map_err(|e| other(e.to_string()))? {
        if net.network.segments.is_empty() {
            continue;
        }
        let report = super::duct::size_with_reducers(&net.network, &criteria).map_err(|e| other(e.to_string()))?;
        let origin =
            d.entity(net.ahu).and_then(|e| ahu_of(e).map(|a| a.name).or_else(|| riser_of(e).map(|r| r.name))).unwrap_or_else(|| net.ahu.hex());
        let segments = report
            .segments
            .iter()
            .enumerate()
            .map(|(i, g)| NoteSegment {
                duct: net.ducts.get(i).map_or(String::new(), |h| h.hex()),
                section: match g.section {
                    Section::Rectangular => format!("{:.0}×{:.0}", g.width_mm, g.height_mm),
                    Section::Round => format!("Ø{:.0}", g.diameter_mm),
                },
                length_m: r(net.network.segments.get(i).map_or(0.0, |x| x.length_m), 2),
                flow_m3h: r(g.flow_m3h, 0),
                velocity_ms: r(g.velocity, 2),
                gradient_pa_m: r(g.gradient_pa_m, 3),
                friction_pa: r(g.friction_pa, 1),
                fittings_pa: r(g.fittings_pa, 1),
                terminal_pa: r(g.terminal_pa, 1),
                total_pa: r(g.total_pa, 1),
                governing: match g.governing {
                    Governing::Velocity => "velocity",
                    Governing::Friction => "friction",
                    Governing::Imposed => "imposed",
                    Governing::Largest => "largest",
                }
                .into(),
                critical: report.critical_path.contains(&i),
            })
            .collect();
        note.networks.push(NoteNetwork {
            origin,
            air: net.air,
            total_flow_m3h: r(report.total_flow_m3h, 0),
            required_pa: r(report.required_pressure_pa, 1),
            segments,
            warnings: report.warnings.clone(),
        });
    }
    Ok(note)
}
