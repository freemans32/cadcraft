//! The sub-network of a run (a pipe, a duct or a cable) as the edit commands take it: `subnetwork` names the runs given ("" takes them out of
//! one), `subnetworkColour` gives it a colour of its own ("#rrggbb") or gives it back the network's (null).

use serde_json::Value;

use super::bad;
use crate::Result;

/// What a call asks of the sub-network: the name for the runs given, and the colour for the whole sub-network.
pub(super) struct SubnetEdit {
    pub name: Option<String>,
    pub colour: Option<Option<String>>,
}

impl SubnetEdit {
    pub fn read(cmd: &str, p: &Value) -> Result<SubnetEdit> {
        let name = match p.get("subnetwork") {
            None | Some(Value::Null) => None,
            Some(Value::String(t)) => Some(t.trim().to_string()),
            Some(_) => return Err(bad(cmd, "`subnetwork` must be text")),
        };
        let colour = match p.get("subnetworkColour") {
            None => None,
            Some(Value::Null) => Some(None),
            Some(Value::String(t)) if cadcraft_doc::parse_hex_colour(t.trim()).is_some() => Some(Some(t.trim().to_ascii_lowercase())),
            Some(_) => return Err(bad(cmd, "`subnetworkColour` must be a colour written #rrggbb, or null")),
        };
        Ok(SubnetEdit { name, colour })
    }

    /// Names a run (its sub-network and colour fields): a run named into a sub-network that has a colour (`known`, the colour its runs have) takes it.
    pub fn name_run(&self, subnetwork: &mut String, colour: &mut Option<String>, known: impl FnOnce(&str) -> Option<String>) {
        if let Some(n) = &self.name {
            *colour = if n.is_empty() { None } else { known(n).or(colour.take()) };
            subnetwork.clone_from(n);
        }
    }
}

/// The colour the runs of sub-network `name` already have, among `(sub-network, colour)` of the runs of the same kind and network.
pub(super) fn known_colour<'a>(runs: impl Iterator<Item = (&'a str, Option<&'a str>)>, name: &str) -> Option<String> {
    runs.filter(|(n, _)| n.eq_ignore_ascii_case(name)).find_map(|(_, c)| c.map(str::to_string))
}
