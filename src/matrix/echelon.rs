use super::matrix_interface::*;
use super::viz_table::*;
use crate::matrix::echelon;
use crate::util::*;
use core::panic;
use derivative::Derivative;
use prettytable::*;
use std::collections::HashSet;

#[derive(Clone, Derivative)]
#[derivative(Default(new = "true"))]
pub struct Echelon<M> {
    base: M,
    /// echelon form is invalidated on any changes to the underlying matrix
    #[derivative(Default(value = "true"))]
    is_info_outdated: bool,
    /// information about the matrix when it's formatted into the Echelon form;
    info: EchelonInfo,
}

impl<M> Echelon<M> {
    pub fn get_base(&self) -> &M {
        &self.base
    }
}

impl<M: MatrixTail> MatrixTail for Echelon<M> {
    fn get_tail_edges(&self) -> &HashSet<EdgeIndex> {
        self.base.get_tail_edges()
    }
    fn get_tail_edges_mut(&mut self) -> &mut HashSet<EdgeIndex> {
        self.is_info_outdated = true;
        self.base.get_tail_edges_mut()
    }
}

impl<M: MatrixTight> MatrixTight for Echelon<M> {
    fn update_edge_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.is_info_outdated = true;
        self.base.update_edge_tightness(edge_index, is_tight)
    }
    fn is_tight(&self, edge_index: usize) -> bool {
        self.base.is_tight(edge_index)
    }
}

impl<M: MatrixBasic> MatrixBasic for Echelon<M> {
    fn add_variable(&mut self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.is_info_outdated = true;
        self.base.add_variable(edge_index)
    }

    fn add_constraint(
        &mut self,
        vertex_index: VertexIndex,
        incident_edges: &[EdgeIndex],
        parity: bool,
    ) -> Option<Vec<VarIndex>> {
        self.is_info_outdated = true;
        self.base.add_constraint(vertex_index, incident_edges, parity)
    }

    fn xor_row(&mut self, target: RowIndex, source: RowIndex) {
        panic!("xor operation on an echelon matrix is forbidden");
    }
    fn swap_row(&mut self, a: RowIndex, b: RowIndex) {
        panic!("swap operation on an echelon matrix is forbidden");
    }
    fn get_lhs(&self, row: RowIndex, var_index: VarIndex) -> bool {
        self.get_base().get_lhs(row, var_index)
    }
    fn get_rhs(&self, row: RowIndex) -> bool {
        self.get_base().get_rhs(row)
    }
    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeIndex {
        self.get_base().var_to_edge_index(var_index)
    }
    fn edge_to_var_index(&self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.get_base().edge_to_var_index(edge_index)
    }
}

impl<M: MatrixView> Echelon<M> {
    fn force_update_echelon_info(&mut self) {
        let width = self.base.columns();
        let height = self.base.rows();
        if width != self.info.columns.len() {
            self.info.columns.resize_with(width, Default::default);
        }
        if height != self.info.rows.len() {
            self.info.rows.resize_with(height, Default::default);
        }
        println!("width: {width}, height: {height}");
        if height == 0 {
            // no parity requirement
            self.info.satisfiable = true;
            self.info.effective_rows = 0;
            return;
        }
        if width == 0 {
            // no variable to satisfy any requirement
            // if any RHS=1, it cannot be satisfied
            for row in 0..height {
                if self.base.get_rhs(row) {
                    self.info.satisfiable = false;
                    self.base.swap_row(0, row); // make it the first row
                    self.info.effective_rows = 1;
                    return;
                }
            }
            self.info.satisfiable = true;
            self.info.effective_rows = 0;
            return;
        }
        // prepare info
        self.info.satisfiable = false;
        let mut lead = 0;
        for r in 0..height {
            if lead >= width {
                // no more variables
                self.info.satisfiable = (r..height).all(|row| !self.base.get_rhs(row));
                if self.info.satisfiable {
                    self.info.effective_rows = r;
                    return;
                }
                // find an unsatisfiable row with rhs=1 and make it the row[r]
                if !self.base.get_rhs(r) {
                    for row in r + 1..height {
                        if self.base.get_rhs(row) {
                            self.base.swap_row(r, row);
                            self.info.rows[r].set_no_leading();
                            break;
                        }
                    }
                }
                debug_assert!(self.base.get_rhs(r));
                debug_assert!(!self.info.satisfiable);
                self.info.effective_rows = r + 1;
                return;
            }
            let mut i = r;
            // find first non-zero lead and make it the row[r]
            while !self.base.get_lhs(i, self.base.column_to_var_index(lead)) {
                i += 1;
                if i == height {
                    i = r;
                    // couldn't find a leading 1 in this column, indicating this variable is an independent variable
                    self.info.columns[self.base.column_to_var_index(lead)].set_not_dependent();
                    lead += 1; // consider the next lead
                    if lead >= width {
                        self.info.satisfiable = (r..height).all(|row| !self.base.get_rhs(row));
                        if self.info.satisfiable {
                            self.info.effective_rows = r;
                            return;
                        }
                        // find a row with rhs=1 and swap with r row
                        if !self.base.get_rhs(r) {
                            // make sure display is reasonable: RHS=1 and all LHS=0
                            for row in r + 1..height {
                                if self.base.get_rhs(row) {
                                    self.base.swap_row(r, row);
                                    self.info.rows[r].set_no_leading();
                                    break;
                                }
                            }
                        }
                        debug_assert!(self.base.get_rhs(r));
                        debug_assert!(!self.info.satisfiable);
                        self.info.effective_rows = r + 1;
                        return;
                    }
                }
            }
            if i != r {
                // implies r < i
                self.base.swap_row(r, i);
            }
            for j in 0..height {
                if j != r && self.base.get_lhs(j, self.base.column_to_var_index(lead)) {
                    self.base.xor_row(j, r);
                }
            }
            self.info.rows[r].set(lead);
            self.info.columns[lead].set(r);
            self.info.effective_rows = r + 1;
            lead += 1;
        }
        while lead < width {
            self.info.columns[lead].set_not_dependent();
            lead += 1;
        }
        self.info.satisfiable = true;
    }

    fn echelon_info_lazy_update(&mut self) {
        if self.is_info_outdated {
            self.force_update_echelon_info();
            self.is_info_outdated = false;
        }
    }
}

impl<M: MatrixView> MatrixEchelon for Echelon<M> {
    fn get_echelon_info(&mut self) -> &EchelonInfo {
        self.echelon_info_lazy_update();
        &self.info
    }
}

impl<M: MatrixView> MatrixView for Echelon<M> {
    fn columns(&mut self) -> usize {
        self.echelon_info_lazy_update();
        self.base.columns()
    }

    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex {
        debug_assert!(!self.is_info_outdated, "call `columns` first");
        self.base.column_to_var_index(column)
    }

    fn rows(&mut self) -> usize {
        self.echelon_info_lazy_update();
        self.info.effective_rows
    }
}

impl<M: MatrixView> VizTrait for Echelon<M> {
    fn viz_table(&mut self) -> VizTable {
        // self will be mutably borrowed, so clone the necessary information
        let info = self.get_echelon_info().clone();
        let leading_edges: Vec<Option<EdgeIndex>> = info
            .rows
            .iter()
            .map(|row_info| {
                if row_info.has_leading() {
                    Some(self.column_to_edge_index(row_info.column))
                } else {
                    None
                }
            })
            .collect();
        let mut table = VizTable::from(self);
        // add echelon mark
        assert!(table.title.get_cell(0).unwrap().get_content().is_empty());
        table.title.set_cell(Cell::new("E").style_spec("brFg"), 0).unwrap();
        assert_eq!(table.title.len(), info.columns.len() + 2);
        assert_eq!(table.rows.len(), info.effective_rows);
        assert_eq!(leading_edges.len(), info.effective_rows);
        // add row information on the right
        table.title.add_cell(Cell::new("\u{25BC}"));
        for (row, edge_index) in leading_edges.iter().enumerate() {
            let cell = if let Some(edge_index) = edge_index {
                Cell::new(edge_index.to_string().as_str()).style_spec("irFr")
            } else {
                Cell::new("*").style_spec("irFm")
            };
            table.rows[row].add_cell(cell);
        }
        // add column information on the bottom
        let mut bottom_row = Row::empty();
        bottom_row.add_cell(Cell::new(" \u{25B6}"));
        for column_info in info.columns.iter() {
            let cell = if column_info.is_dependent() {
                Cell::new(column_info.row.to_string().as_str()).style_spec("irFb")
            } else {
                Cell::new("*").style_spec("irFm")
            };
            bottom_row.add_cell(cell);
        }
        bottom_row.add_cell(Cell::new("\u{25C0}  "));
        bottom_row.add_cell(Cell::new("\u{25B2}"));
        table.rows.push(bottom_row);
        table
    }
}

#[cfg(test)]
pub mod tests {
    use super::super::basic_matrix::*;
    use super::super::tail::*;
    use super::super::tight::*;
    use super::*;

    type EchelonMatrix = Echelon<Tail<Tight<BasicMatrix>>>;

    #[test]
    fn echelon_matrix_simple() {
        // cargo test --features=colorful echelon_matrix_simple -- --nocapture
        let mut matrix = EchelonMatrix::new();
        matrix.add_constraint(0, &[1, 4, 6], true);
        matrix.add_constraint(1, &[4, 9], false);
        matrix.add_constraint(2, &[1, 9], true);
        assert_eq!(matrix.edge_to_var_index(4), Some(1));
        for edge_index in [1, 4, 6, 9] {
            matrix.update_edge_tightness(edge_index, true);
        }
        matrix.printstd();
        assert_eq!(
            matrix.clone().printstd_str(),
            "\
┌──┬─┬─┬─┬─┬───┬─┐
┊ E┊1┊4┊6┊9┊ = ┊▼┊
╞══╪═╪═╪═╪═╪═══╪═╡
┊ 0┊1┊ ┊ ┊1┊ 1 ┊1┊
├──┼─┼─┼─┼─┼───┼─┤
┊ 1┊ ┊1┊ ┊1┊   ┊4┊
├──┼─┼─┼─┼─┼───┼─┤
┊ 2┊ ┊ ┊1┊ ┊   ┊6┊
├──┼─┼─┼─┼─┼───┼─┤
┊ ▶┊0┊1┊2┊*┊◀  ┊▲┊
└──┴─┴─┴─┴─┴───┴─┘
"
        );
        // matrix.
    }
}
