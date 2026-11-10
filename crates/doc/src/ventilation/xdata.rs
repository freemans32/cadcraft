//! Parameters stored as JSON in an object's extended data: `1001` application name, `1000`
//! format name, then the JSON in `1000` strings short enough for any file version.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::RawTag;

/// Bytes per xdata string, each non-ASCII character counted as the 7-byte `\U+XXXX` escape an
/// older DXF file writes it as: under the 255-byte limit in any file version.
const CHUNK_COST: usize = 240;
/// Most bytes of one object's parameters: within the 16 KB other programs allow per object.
pub(super) const MAX_XDATA_BYTES: usize = 12_000;

fn tag(code: i32, value: &str) -> RawTag {
    RawTag { code, value: value.into() }
}

fn char_cost(c: char) -> usize {
    if c.is_ascii() { 1 } else { 7 }
}

/// The JSON cut into strings. A JSON escape is never split across two strings, and an escaped
/// backslash is written `\`, so a name holding `\U+0041` can't be read back by a DXF
/// reader as the character it spells.
fn json_strings(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut cost = 0usize;
    let mut chars = json.chars();
    while let Some(c) = chars.next() {
        let mut piece = c.to_string();
        if c == '\\'
            && let Some(n) = chars.next()
        {
            if n == '\\' {
                piece = "\\u005c".into();
            } else {
                piece.push(n);
            }
        }
        let pc: usize = piece.chars().map(char_cost).sum();
        if cost + pc > CHUNK_COST && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            cost = 0;
        }
        cur.push_str(&piece);
        cost += pc;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub(super) fn to_xdata<T: Serialize>(app: &str, format: &str, value: &T) -> crate::Result<Vec<RawTag>> {
    let json = serde_json::to_string(value).map_err(|e| super::invalid(format!("The object can't be saved: {e}")))?;
    let strings = json_strings(&json);
    let cost: usize = strings.iter().map(|s| s.chars().map(char_cost).sum::<usize>()).sum();
    if cost > MAX_XDATA_BYTES {
        return Err(super::invalid("The object has too much data to save: use fewer points or shorter names."));
    }
    let mut tags = vec![tag(1001, app), tag(1000, format)];
    tags.extend(strings.into_iter().map(|value| RawTag { code: 1000, value }));
    Ok(tags)
}

pub(super) fn from_xdata<T: DeserializeOwned>(app: &str, format: &str, xdata: &[RawTag]) -> Option<T> {
    let start = xdata.iter().position(|t| t.code == 1001 && t.value.trim().eq_ignore_ascii_case(app))?;
    let ours = xdata.get(start + 1..)?;
    let ours = ours.get(..ours.iter().position(|t| t.code == 1001).unwrap_or(ours.len()))?;
    let mut strings = ours.iter().filter(|t| t.code == 1000);
    if strings.next()?.value != format {
        return None;
    }
    let mut json = String::new();
    let mut cost = 0usize;
    for t in strings {
        cost = cost.saturating_add(t.value.chars().map(char_cost).sum());
        if cost > MAX_XDATA_BYTES {
            return None;
        }
        json.push_str(&t.value);
    }
    serde_json::from_str(&json).ok()
}

/// `xdata` with `app`'s data replaced by `ours` (other applications' data kept).
pub(super) fn replace(app: &str, xdata: &[RawTag], ours: Vec<RawTag>) -> Vec<RawTag> {
    let mut out = Vec::with_capacity(xdata.len() + ours.len());
    let mut mine = false;
    for t in xdata {
        if t.code == 1001 {
            mine = t.value.trim().eq_ignore_ascii_case(app);
        }
        if !mine {
            out.push(t.clone());
        }
    }
    out.extend(ours);
    out
}
