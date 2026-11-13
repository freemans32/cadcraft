//! HVACTAKEOFF: the quantities of one level, each drawing of a building being a floor: ducts by
//! direction, section and altitude (length and surface), fittings, accessories, terminals, units,
//! risers, equipment and, by space, what it holds. As JSON, and as a `;`-separated file a spreadsheet opens.

use serde_json::{Value, json};

use super::{CommandSpec, bad, str_param};

use crate::{EngineError, Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("hvactakeoff", "HVAC Quantities", run)
            .menu(&["Draw", "HVAC", "Quantities"])
            .params(
                "{level? (the name of the floor, written on the result), path? (a .csv file to write: `;` between cells), decimal?: comma|point (the \\
                 decimal mark of the file, default comma)} → {level, ducts, fittings, accessories, terminals, units, risers, equipment, spaces, totals, path?}",
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
    if let Some(path) = str_param(p, "path").map(str::trim).filter(|x| !x.is_empty()) {
        if path.len() > 1024 || path.contains('\0') {
            return Err(bad(CMD, "`path` is not a usable file name"));
        }
        let path = std::path::Path::new(path);
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| other(format!("Cannot write {}: {e}", path.display())))?;
        }
        // A byte order mark, so a spreadsheet reads the accents as UTF-8.
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(t.to_csv(comma).as_bytes());
        std::fs::write(path, bytes).map_err(|e| other(format!("Cannot write {}: {e}", path.display())))?;
        out["path"] = json!(path.display().to_string());
    }
    Ok(out)
}
