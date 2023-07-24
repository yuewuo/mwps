//! Parity Matrix Table
//!
//! Converting a matrix into a printable table.
//!
//! I created my own Table struct as a simple wrapper on prettytable::Table
//! because it doesn't provide any public method to retrieve the title row.
//! Some of my functionalities require a flexible operation on the title row.
//!

use super::*;
use prettytable::format::TableFormat;
use prettytable::*;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

#[derive(Clone, Debug)]
pub struct VizTable {
    pub title: Row,
    pub rows: Vec<Row>,
}

impl VizTable {
    pub fn new(basic_matrix: &BasicMatrix, var_indices: &[usize]) -> Self {
        Self {
            title: Self::build_title(basic_matrix, var_indices),
            rows: Self::build_rows(basic_matrix, var_indices),
        }
    }

    pub fn force_single_column(long_str: &str) -> String {
        long_str
            .chars()
            .enumerate()
            .flat_map(|(idx, c)| if idx == 0 { vec![c] } else { vec!['\n', c] })
            .collect()
    }

    pub fn build_title(basic_matrix: &BasicMatrix, var_indices: &[usize]) -> Row {
        let mut title_row = Row::empty();
        title_row.add_cell(Cell::new(""));
        for &var_index in var_indices.iter() {
            let edge_index = basic_matrix.variables[var_index].edge_index;
            let edge_index_str = Self::force_single_column(edge_index.to_string().as_str());
            title_row.add_cell(Cell::new(edge_index_str.as_str()).style_spec("brFr"));
        }
        title_row.add_cell(Cell::new(" = "));
        title_row
    }

    pub fn build_rows(basic_matrix: &BasicMatrix, var_indices: &[usize]) -> Vec<Row> {
        let mut rows: Vec<Row> = vec![];
        for (row_index, row) in basic_matrix.constraints.iter().enumerate() {
            let mut table_row = Row::empty();
            table_row.add_cell(Cell::new(row_index.to_string().as_str()).style_spec("brFb"));
            for &var_index in var_indices.iter() {
                table_row.add_cell(Cell::new(if row.get_left(var_index) { "1" } else { " " }));
            }
            table_row.add_cell(Cell::new(if row.get_right() { " 1 " } else { "   " }));
            rows.push(table_row);
        }
        rows
    }
}

lazy_static! {
    pub static ref DEFAULT_FORMAT: TableFormat = {
        let mut format = TableFormat::new();
        format.padding(0, 0);
        format.column_separator('\u{250A}');
        format.borders('\u{250A}');
        use format::LinePosition::*;
        let separators = [
            (Intern, ['\u{2500}', '\u{253C}', '\u{251C}', '\u{2524}']),
            (Top, ['\u{2500}', '\u{252C}', '\u{250C}', '\u{2510}']),
            (Bottom, ['\u{2500}', '\u{2534}', '\u{2514}', '\u{2518}']),
            (Title, ['\u{2550}', '\u{256A}', '\u{255E}', '\u{2561}']),
        ];
        for (position, s) in separators {
            format.separators(&[position], format::LineSeparator::new(s[0], s[1], s[2], s[3]))
        }
        format
    };
}
