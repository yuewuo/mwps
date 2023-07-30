use crate::decoding_hypergraph::*;
use crate::dual_module::*;
use crate::prettytable::*;
use crate::util::*;
use derivative::Derivative;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

pub trait MatrixView {
    fn display_table_body(&self) -> Vec<Row>;
    fn display_table_title(&self) -> Row;

    fn display_table(&self) -> Table {
        let mut table = nice_look_table();
        table.set_titles(self.display_table_title());
        for row in self.display_table_body().into_iter() {
            table.add_row(row);
        }
        table
    }

    fn additional_json(&self, _abbrev: bool) -> serde_json::Value {
        json!({}) // by default no additional json data
    }

    fn printstd(&self) {
        #[cfg(feature = "colorful")]
        self.display_table().printstd();
        #[cfg(not(feature = "colorful"))]
        println!("{}", self.display_table());
    }

    fn printstd_str(&self) -> String {
        self.display_table().to_string()
    }

    fn to_visualize_json(&self, abbrev: bool) -> serde_json::Value {
        let table = self.display_table();
        let mut table_str = vec![];
        for row in &table {
            let mut row_str = vec![];
            for cell in row {
                row_str.push(cell.get_content());
            }
            table_str.push(row_str);
        }
        json!({
            "table": table_str,
            "add": self.additional_json(abbrev),
        })
    }
}

pub fn nice_look_table() -> Table {
    let mut table = Table::new();
    let table_format = table.get_format();
    table_format.padding(0, 0);
    table_format.column_separator('\u{250A}');
    table_format.borders('\u{250A}');
    use format::LinePosition::*;
    let separators = [
        (Intern, ['\u{2500}', '\u{253C}', '\u{251C}', '\u{2524}']),
        (Top, ['\u{2500}', '\u{252C}', '\u{250C}', '\u{2510}']),
        (Bottom, ['\u{2500}', '\u{2534}', '\u{2514}', '\u{2518}']),
        (Title, ['\u{2550}', '\u{256A}', '\u{255E}', '\u{2561}']),
    ];
    for (position, s) in separators {
        table_format.separators(&[position], format::LineSeparator::new(s[0], s[1], s[2], s[3]))
    }
    table
}
