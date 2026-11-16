//! The CSV and TSV viewer: the `csv` crate reads the file once, streaming, into an index of where each
//! record starts; only the rows on screen are parsed and built (`virtual_rows`, with a version so a
//! hundred thousand rows commit in time independent of their number); the header stays above the
//! scrolling rows and the footer says how many there are.
//!
//! Why not the kit's `Table`: it is built from `&App` (`Table::build(s, kit)`), and a registered
//! viewer is a function of its input and its own state only, so a plugin's viewer can stand beside
//! this one. The rows, header and metrics here are the kit's `TableStyle`, drawn the same way.

use std::borrow::Cow;
use std::rc::Rc;

use rune::{El, GrowthSpec, ScrollSpec, col, el, row, text};
use rune_kit::TableStyle;

use super::{ViewerInput, ViewerState};
use crate::app::App;
use crate::theme::theme;

/// Columns are at least this wide, in points, and at most [`COLUMN_MAX`].
const COLUMN_MIN: f32 = 72.0;
const COLUMN_MAX: f32 = 260.0;
/// Records read to size the columns.
const SAMPLE: usize = 200;

/// A CSV file indexed: its bytes and where each record starts. Record 0 is the header.
pub struct CsvData {
    bytes: Vec<u8>,
    delimiter: u8,
    /// Where each record starts in `bytes`.
    offsets: Vec<u64>,
    pub header: Vec<String>,
    /// How wide each column is, in points.
    pub widths: Vec<f32>,
}

impl CsvData {
    pub fn new(bytes: &[u8], delimiter: u8) -> CsvData {
        let mut reader = csv::ReaderBuilder::new().has_headers(false).flexible(true).delimiter(delimiter).from_reader(bytes);
        let mut offsets: Vec<u64> = Vec::new();
        let mut record = csv::ByteRecord::new();
        let mut header: Vec<String> = Vec::new();
        let mut longest: Vec<usize> = Vec::new();
        loop {
            let at = reader.position().byte();
            match reader.read_byte_record(&mut record) {
                Ok(true) => {
                    let n = offsets.len();
                    offsets.push(at);
                    if n < SAMPLE {
                        if n == 0 {
                            header = record.iter().map(|f| String::from_utf8_lossy(f).into_owned()).collect();
                        }
                        if longest.len() < record.len() {
                            longest.resize(record.len(), 0);
                        }
                        for (i, f) in record.iter().enumerate() {
                            longest[i] = longest[i].max(String::from_utf8_lossy(f).chars().count());
                        }
                    }
                }
                Ok(false) => break,
                // A malformed stretch ends the index there: what was read stays usable.
                Err(_) => break,
            }
        }
        let widths = longest.iter().map(|n| ((*n as f32 + 3.0) * 7.2 + 24.0).clamp(COLUMN_MIN, COLUMN_MAX)).collect();
        CsvData { bytes: bytes.to_vec(), delimiter, offsets, header, widths }
    }

    /// Rows under the header.
    pub fn rows(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    pub fn columns(&self) -> usize {
        self.widths.len()
    }

    /// The fields of data row `i` (0 is the first under the header).
    pub fn row(&self, i: usize) -> Vec<String> {
        let (Some(&from), end) = (self.offsets.get(i + 1), self.offsets.get(i + 2)) else { return Vec::new() };
        let to = end.copied().unwrap_or(self.bytes.len() as u64).min(self.bytes.len() as u64);
        let mut reader = csv::ReaderBuilder::new().has_headers(false).flexible(true).delimiter(self.delimiter).from_reader(&self.bytes[from as usize..to as usize]);
        reader.records().next().and_then(Result::ok).map(|r| r.iter().map(str::to_owned).collect()).unwrap_or_default()
    }

    /// How much memory this holds, for a measurement.
    pub fn held(&self) -> usize {
        self.bytes.len() + self.offsets.len() * 8
    }
}

/// `100000` as `100,000`.
pub fn group_digits(n: usize) -> String {
    let digits = n.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    grouped
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", group_digits(n), if n == 1 { one } else { many })
}

pub fn build(input: &ViewerInput, state: &mut ViewerState) -> El {
    let t = theme(input.theme_dark);
    let style = TableStyle::from_kit(&t.kit);
    let delimiter = if input.kind.eq_ignore_ascii_case("tsv") { b'\t' } else { b',' };
    let bytes = input.bytes.unwrap_or(&[]);
    let data: Rc<CsvData> = state.data(|| Rc::new(CsvData::new(bytes, delimiter))).clone();
    if data.columns() == 0 {
        return col().key("viewer-table").w_full().grow(1.0).center().child(text("This file has no rows.").font(t.ty.meta).color(t.text.muted));
    }
    let total: f32 = data.widths.iter().sum();
    let key = state.key.clone();
    let rows = data.rows();

    let mut header = row().key("table-header").h(style.header_height).w(total).fill(t.surface.card).child(el().key("rule").pin(Some(0.0), None, Some(0.0), Some(0.0)).h(1.0).fill(style.rule).a11y_hidden());
    for (i, w) in data.widths.iter().enumerate() {
        let title = data.header.get(i).cloned().unwrap_or_default();
        header = header.child(row().key(format!("h{i}")).role("columnheader").label(title.clone()).w(*w).h_full().px(style.cell_pad).center_y().child(text(title).font(style.header_font).color(style.header_text).truncate().min_w(0.0)));
    }

    let body = {
        let data = data.clone();
        let (font, ink) = (style.font, style.text);
        let (row_h, pad, stripe) = (style.row_height, style.cell_pad, style.stripe);
        col()
            .key("table-rows")
            .w(total)
            .growth(GrowthSpec::default())
            .virtual_rows(
                rows.max(1),
                row_h,
                |_: &App, i| Cow::Owned(format!("r{i}")),
                move |_: &App, i| {
                    let fields = data.row(i);
                    let mut r = row().w_full().h(row_h).center_y();
                    if i % 2 == 1 {
                        r = r.fill(stripe);
                    }
                    for (c, w) in data.widths.iter().enumerate() {
                        r = r.child(text(fields.get(c).cloned().unwrap_or_default()).font(font).color(ink).truncate().w(*w).px(pad).min_w(0.0));
                    }
                    r
                },
            )
            .virtual_rows_version((key.clone(), rows))
            .on_visible_rows(move |s: &mut App, range| {
                s.panel.states.with(&key, |st| st.scroll = range.start);
                false
            })
    };

    let inner = col().key("table-inner").w(total).h_full().child(header).child(col().key("table-scroll").w(total).grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body));
    col()
        .key("viewer-table")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(col().key("table-x").w_full().grow(1.0).min_h(0.0).scroll_x().child(inner))
        .child(text(format!("{} · {}", plural(rows, "row", "rows"), plural(data.columns(), "column", "columns"))).key("table-footer").font(t.ty.id).color(t.text.faint).px(12.0).py(6.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_indexed_once_and_read_by_position() {
        let data = CsvData::new(b"id,name\n1,Ada\n2,\"Grace, H\"\n3,\"multi\nline\"\n4,Linus", b',');
        assert_eq!(data.header, ["id", "name"]);
        assert_eq!(data.rows(), 4);
        assert_eq!(data.row(0), ["1", "Ada"]);
        assert_eq!(data.row(1), ["2", "Grace, H"], "a quoted comma stays in its field");
        assert_eq!(data.row(2), ["3", "multi\nline"], "a quoted newline stays in its record");
        assert_eq!(data.row(3), ["4", "Linus"], "the last record needs no newline");
        assert!(data.row(4).is_empty());
        assert_eq!(CsvData::new(b"a\tb\n1\t2\n", b'\t').row(0), ["1", "2"]);
        assert_eq!(CsvData::new(b"", b',').columns(), 0);
    }

    #[test]
    fn a_hundred_thousand_rows_index_quickly_and_read_the_row_asked_for() {
        let mut csv = String::from("id,name,amount,when\n");
        for i in 0..100_000 {
            csv.push_str(&format!("{i},user-{i},{}.{:02},2026-11-16\n", i % 977, i % 100));
        }
        let started = std::time::Instant::now();
        let data = CsvData::new(csv.as_bytes(), b',');
        let took = started.elapsed();
        assert_eq!((data.rows(), data.columns()), (100_000, 4));
        assert_eq!(data.row(50_000)[1], "user-50000");
        assert_eq!(data.row(99_999)[0], "99999");
        assert!(took.as_secs_f32() < 5.0, "indexing took {took:?} (a debug build)");
        assert!(data.held() < csv.len() + 100_000 * 8 + 16, "the index is 8 bytes a record");
        assert_eq!(plural(100_000, "row", "rows"), "100,000 rows");
        assert_eq!(plural(1, "row", "rows"), "1 row");
    }

    #[test]
    fn columns_are_sized_from_their_widest_early_value_within_bounds() {
        let data = CsvData::new(b"a,a long header,c\nxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx,2,3\n", b',');
        assert_eq!(data.widths[0], COLUMN_MAX, "capped");
        assert_eq!(data.widths[2], COLUMN_MIN, "floored");
        assert!(data.widths[1] > COLUMN_MIN && data.widths[1] < COLUMN_MAX);
    }
}
