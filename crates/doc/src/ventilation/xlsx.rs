//! A minimal Excel workbook (.xlsx): sheets of text and number cells, a bold style for headings,
//! column widths. An .xlsx file is a zip of XML parts; the zip is written here with its parts
//! stored, not compressed, which every reader accepts, so nothing outside the standard library is
//! needed. Used for the quantities and the calculation note.

use super::invalid;
use crate::Result;

/// Most rows a sheet keeps (Excel's own limit is 1 048 576).
const MAX_ROWS: usize = 100_000;
/// Most sheets a workbook has.
const MAX_SHEETS: usize = 32;
/// Widest a column is made, in characters.
const MAX_WIDTH: usize = 60;

/// One cell.
#[derive(Clone, Debug, PartialEq)]
pub enum SheetCell {
    Empty,
    Text(String),
    /// A heading: bold text.
    Head(String),
    Num(f64),
}

impl SheetCell {
    pub fn text(t: impl Into<String>) -> SheetCell {
        SheetCell::Text(t.into())
    }
    pub fn head(t: impl Into<String>) -> SheetCell {
        SheetCell::Head(t.into())
    }
    /// A number, or an empty cell for none (and for a value that is not finite).
    pub fn opt(v: Option<f64>) -> SheetCell {
        v.filter(|x| x.is_finite()).map_or(SheetCell::Empty, SheetCell::Num)
    }
}

struct Sheet {
    name: String,
    rows: Vec<Vec<SheetCell>>,
}

/// A workbook being built.
#[derive(Default)]
pub struct Workbook {
    sheets: Vec<Sheet>,
}

fn escape(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    for c in t.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Characters XML 1.0 does not allow are dropped.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            '\u{fffe}' | '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

/// A sheet name Excel accepts: at most 31 characters, none of `[]:*?/\`, not empty.
fn sheet_name(t: &str, taken: &[Sheet]) -> String {
    let mut name: String = t.chars().filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\') && !c.is_control()).take(31).collect();
    if name.trim().is_empty() {
        name = "Sheet".into();
    }
    let base = name.clone();
    let mut n = 2;
    while taken.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
        let suffix = format!(" {n}");
        name = format!("{}{suffix}", base.chars().take(31 - suffix.len()).collect::<String>());
        n += 1;
    }
    name
}

/// The column letters of 0-based column `i`: A, B, … Z, AA, …
fn column(mut i: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    out.reverse();
    String::from_utf8_lossy(&out).into_owned()
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn u16le(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn u32le(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// A zip of `files` (name, bytes), the parts stored.
fn zip(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let too_big = || invalid("The workbook is too large.");
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let offset = u32::try_from(out.len()).map_err(|_| too_big())?;
        let size = u32::try_from(data.len()).map_err(|_| too_big())?;
        let name_len = u16::try_from(name.len()).map_err(|_| too_big())?;
        let crc = crc32(data);
        // Local header.
        u32le(&mut out, 0x0403_4b50);
        u16le(&mut out, 20);
        u16le(&mut out, 0);
        u16le(&mut out, 0); // stored
        u16le(&mut out, 0); // time
        u16le(&mut out, 0x0021); // date: 1980-01-01
        u32le(&mut out, crc);
        u32le(&mut out, size);
        u32le(&mut out, size);
        u16le(&mut out, name_len);
        u16le(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        // Central directory entry.
        u32le(&mut central, 0x0201_4b50);
        u16le(&mut central, 20);
        u16le(&mut central, 20);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0x0021);
        u32le(&mut central, crc);
        u32le(&mut central, size);
        u32le(&mut central, size);
        u16le(&mut central, name_len);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u32le(&mut central, 0);
        u32le(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let start = u32::try_from(out.len()).map_err(|_| too_big())?;
    let size = u32::try_from(central.len()).map_err(|_| too_big())?;
    let count = u16::try_from(files.len()).map_err(|_| too_big())?;
    out.extend_from_slice(&central);
    u32le(&mut out, 0x0605_4b50);
    u16le(&mut out, 0);
    u16le(&mut out, 0);
    u16le(&mut out, count);
    u16le(&mut out, count);
    u32le(&mut out, size);
    u32le(&mut out, start);
    u16le(&mut out, 0);
    Ok(out)
}

impl Workbook {
    pub fn new() -> Workbook {
        Workbook::default()
    }

    /// Adds a sheet; its name is made valid and unique. Rows beyond the limit are dropped.
    pub fn sheet(&mut self, name: &str, rows: Vec<Vec<SheetCell>>) -> Result<()> {
        if self.sheets.len() >= MAX_SHEETS {
            return Err(invalid("A workbook has at most 32 sheets."));
        }
        let name = sheet_name(name, &self.sheets);
        let mut rows = rows;
        rows.truncate(MAX_ROWS);
        self.sheets.push(Sheet { name, rows });
        Ok(())
    }

    fn sheet_xml(sheet: &Sheet) -> String {
        let cols = sheet.rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut width = vec![8usize; cols];
        for row in &sheet.rows {
            for (i, c) in row.iter().enumerate() {
                let n = match c {
                    SheetCell::Text(t) | SheetCell::Head(t) => t.chars().count(),
                    SheetCell::Num(v) => format!("{v}").len(),
                    SheetCell::Empty => 0,
                };
                if let Some(w) = width.get_mut(i) {
                    *w = (*w).max(n + 2).min(MAX_WIDTH);
                }
            }
        }
        let mut x = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
        );
        if cols > 0 {
            x.push_str("<cols>");
            for (i, w) in width.iter().enumerate() {
                x.push_str(&format!("<col min=\"{0}\" max=\"{0}\" width=\"{w}\" customWidth=\"1\"/>", i + 1));
            }
            x.push_str("</cols>");
        }
        x.push_str("<sheetData>");
        for (r, row) in sheet.rows.iter().enumerate() {
            x.push_str(&format!("<row r=\"{}\">", r + 1));
            for (c, cell) in row.iter().enumerate() {
                let at = format!("{}{}", column(c), r + 1);
                match cell {
                    SheetCell::Empty => {}
                    SheetCell::Text(t) => {
                        x.push_str(&format!("<c r=\"{at}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>", escape(t)))
                    }
                    SheetCell::Head(t) => {
                        x.push_str(&format!("<c r=\"{at}\" s=\"1\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>", escape(t)))
                    }
                    SheetCell::Num(v) if v.is_finite() => x.push_str(&format!("<c r=\"{at}\" s=\"2\"><v>{v}</v></c>")),
                    SheetCell::Num(_) => {}
                }
            }
            x.push_str("</row>");
        }
        x.push_str("</sheetData></worksheet>");
        x
    }

    /// The file's bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.sheets.is_empty() {
            return Err(invalid("A workbook needs a sheet."));
        }
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
        let mut content = format!(
            "{xml}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>"
        );
        let mut book = format!(
            "{xml}<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>"
        );
        let mut rels = format!("{xml}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for (i, s) in self.sheets.iter().enumerate() {
            let n = i + 1;
            content.push_str(&format!("<Override PartName=\"/xl/worksheets/sheet{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"));
            book.push_str(&format!("<sheet name=\"{}\" sheetId=\"{n}\" r:id=\"rId{n}\"/>", escape(&s.name)));
            rels.push_str(&format!("<Relationship Id=\"rId{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{n}.xml\"/>"));
            files.push((format!("xl/worksheets/sheet{n}.xml"), Self::sheet_xml(s).into_bytes()));
        }
        content.push_str("</Types>");
        book.push_str("</sheets></workbook>");
        let styles = format!(
            "{xml}<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills><borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"3\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/></cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>"
        );
        rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>",
            self.sheets.len() + 1
        ));
        let root = format!(
            "{xml}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>"
        );
        let mut all = vec![
            ("[Content_Types].xml".to_string(), content.into_bytes()),
            ("_rels/.rels".to_string(), root.into_bytes()),
            ("xl/workbook.xml".to_string(), book.into_bytes()),
            ("xl/_rels/workbook.xml.rels".to_string(), rels.into_bytes()),
            ("xl/styles.xml".to_string(), styles.into_bytes()),
        ];
        all.extend(files);
        zip(&all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_letters_run_past_z() {
        assert_eq!([0, 25, 26, 27, 701, 702].map(column), ["A", "Z", "AA", "AB", "ZZ", "AAA"]);
    }

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn sheet_names_are_made_valid_and_unique() {
        let mut w = Workbook::new();
        w.sheet("Gaines/Réseaux: [1]", vec![]).unwrap();
        w.sheet("Gaines/Réseaux: [1]", vec![]).unwrap();
        w.sheet(&"x".repeat(60), vec![]).unwrap();
        w.sheet("", vec![]).unwrap();
        let names: Vec<_> = w.sheets.iter().map(|s| s.name.clone()).collect();
        assert_eq!(names[0], "GainesRéseaux 1");
        assert_eq!(names[1], "GainesRéseaux 1 2");
        assert_eq!(names[2].chars().count(), 31);
        assert_eq!(names[3], "Sheet");
        assert!(names.iter().all(|n| n.chars().count() <= 31));
    }

    #[test]
    fn a_workbook_is_a_zip_with_its_parts_and_escapes_text() {
        let mut w = Workbook::new();
        w.sheet(
            "A",
            vec![
                vec![SheetCell::head("Name"), SheetCell::head("Qty")],
                vec![SheetCell::text("R&D <x> \"q\"\u{1}"), SheetCell::Num(2.5), SheetCell::Num(f64::NAN), SheetCell::Empty],
            ],
        )
        .unwrap();
        let b = w.to_bytes().unwrap();
        assert_eq!(&b[..4], b"PK\x03\x04");
        let text = String::from_utf8_lossy(&b);
        for part in ["[Content_Types].xml", "_rels/.rels", "xl/workbook.xml", "xl/styles.xml", "xl/worksheets/sheet1.xml"] {
            assert!(text.contains(part), "{part}");
        }
        assert!(text.contains("R&amp;D &lt;x&gt; &quot;q&quot;"), "text is escaped, control characters dropped");
        assert!(text.contains("<v>2.5</v>") && !text.contains("NaN"));
        // The end of central directory record closes the file.
        assert_eq!(&b[b.len() - 22..b.len() - 18], b"PK\x05\x06");
        assert!(Workbook::new().to_bytes().is_err());
    }
}
