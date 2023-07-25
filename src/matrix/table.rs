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

#[derive(Clone)]
pub struct VizTable {
    pub title: Row,
    pub rows: Vec<Row>,
}

impl VizTable {
    pub fn new(parity_matrix: &ParityMatrix, var_indices: &[usize]) -> Self {
        Self {
            title: Self::build_title(parity_matrix, var_indices),
            rows: Self::build_rows(parity_matrix, var_indices),
        }
    }

    pub fn force_single_column(long_str: &str) -> String {
        long_str
            .chars()
            .enumerate()
            .flat_map(|(idx, c)| if idx == 0 { vec![c] } else { vec!['\n', c] })
            .collect()
    }

    pub fn build_title(parity_matrix: &ParityMatrix, var_indices: &[usize]) -> Row {
        let mut title_row = Row::empty();
        title_row.add_cell(Cell::new(""));
        for &var_index in var_indices.iter() {
            let edge_index = parity_matrix.variables[var_index].edge_index;
            let edge_index_str = Self::force_single_column(edge_index.to_string().as_str());
            title_row.add_cell(Cell::new(edge_index_str.as_str()).style_spec("brFr"));
        }
        title_row.add_cell(Cell::new(" = "));
        title_row
    }

    pub fn build_rows(parity_matrix: &ParityMatrix, var_indices: &[usize]) -> Vec<Row> {
        let mut rows: Vec<Row> = vec![];
        for (row_index, row) in parity_matrix.constraints.iter().enumerate() {
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
    pub static ref DEFAULT_TABLE_FORMAT: TableFormat = {
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

impl From<VizTable> for Table {
    fn from(viz_table: VizTable) -> Table {
        let mut table = Table::new();
        table.set_format(*DEFAULT_TABLE_FORMAT);
        table.set_titles(viz_table.title.clone());
        for row in viz_table.rows.iter() {
            table.add_row(row.clone());
        }
        table
    }
}

impl From<VizTable> for serde_json::Value {
    fn from(viz_table: VizTable) -> serde_json::Value {
        let mut table_json = vec![];
        let mut title_json = vec![];
        for cell in viz_table.title.iter() {
            title_json.push(cell.get_content());
        }
        table_json.push(title_json);
        for row in viz_table.rows.iter() {
            let mut row_json = vec![];
            for cell in row {
                row_json.push(cell.get_content());
            }
            table_json.push(row_json);
        }
        json!(table_json)
    }
}

pub trait VizTrait {
    fn viz_table(&self) -> VizTable;
    fn printstd_str(&self) -> String {
        Table::from(self.viz_table()).to_string()
    }
    fn printstd(&self) {
        #[cfg(feature = "colorful")]
        Table::from(self.viz_table()).printstd();
        #[cfg(not(feature = "colorful"))]
        println!("{}", Table::from(self.viz_table()));
    }
}

impl VizTrait for VizTable {
    fn viz_table(&self) -> VizTable {
        self.clone()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn parity_matrix_table_1() {
        // cargo test --features=colorful parity_matrix_table_1 -- --nocapture
        let mut parity_matrix = ParityMatrix::new();
        for edge_index in 0..4 {
            parity_matrix.add_tight_variable(edge_index * 11);
        }
        let parity_checks = vec![
            (vec![0, 11], true),
            (vec![33], true),
            (vec![11, 12], false),
            (vec![11, 22, 33], false),
        ];
        parity_matrix.add_parity_checks(&parity_checks);
        parity_matrix.printstd();
        let expected_result = "\
┌─┬─┬─┬─┬─┬───┐
┊ ┊0┊1┊2┊3┊ = ┊
┊ ┊ ┊1┊2┊3┊   ┊
╞═╪═╪═╪═╪═╪═══╡
┊0┊1┊1┊ ┊ ┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊1┊ ┊ ┊ ┊1┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊2┊ ┊1┊ ┊ ┊   ┊
├─┼─┼─┼─┼─┼───┤
┊3┊ ┊1┊1┊1┊   ┊
└─┴─┴─┴─┴─┴───┘
";
        assert_eq!(parity_matrix.printstd_str(), expected_result);
        let viz_table: VizTable = parity_matrix.viz_table();
        assert_eq!(viz_table.printstd_str(), expected_result);
        let cloned = viz_table.clone();
        assert_eq!(cloned.printstd_str(), expected_result);
        let json_value: serde_json::Value = viz_table.into();
        println!("{json_value}");
        assert_eq!(
            json_value,
            json!([
                ["", "0", "1\n1", "2\n2", "3\n3", " = "],
                ["0", "1", "1", " ", " ", " 1 "],
                ["1", " ", " ", " ", "1", " 1 "],
                ["2", " ", "1", " ", " ", "   "],
                ["3", " ", "1", "1", "1", "   "]
            ])
        );
    }
}
