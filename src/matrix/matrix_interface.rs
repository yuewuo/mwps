//! Matrix Definition
//!
//! A matrix has a fixed data layout which consists of multiple rows
//!
//! The only operations to change the basic matrix itself are
//! - add variable (column)
//! - add constraint (row)
//! - xor/swap rows
//!
//! Apart from the matrix itself, we can have a view of the matrix:
//! a view is defined as a list of columns `var_indices: Vec<usize>`
//! and a number of rows `rows_count` representing rows `0..rows_count`.
//!
//! A `var_index` is always referring to the original matrix, to improve performance
//! as well as to avoid confusion.
//!
//! Each variable (column) corresponds to an edge in the hypergraph, so we label the
//! columns in the basic matrix. When it comes to operating the matrix, we'll always
//! use the `var_index` to avoid duplicated translation (at least one translation is necessary).
//!

use crate::util::*;

pub type VarIndex = usize;
pub type RowIndex = usize;
pub type ColumnIndex = usize;

/// deprecated
pub trait MatrixImpl {
    fn add_variable(&mut self, edge_index: EdgeIndex);
    fn update_edge_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool);
    fn is_tight(&self, var_index: usize) -> bool;

    fn add_variable_with_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.add_variable(edge_index);
        self.update_edge_tightness(edge_index, is_tight);
    }

    fn add_tight_variable(&mut self, edge_index: EdgeIndex) {
        self.add_variable_with_tightness(edge_index, true)
    }
}

pub trait MatrixBasic {
    /// add an edge to the basic matrix, return the `var_index` if newly created
    fn add_variable(&mut self, edge_index: EdgeIndex) -> Option<VarIndex>;

    /// add constraint will implicitly call `add_variable` if the edge is not added and return the indices of them
    fn add_constraint(
        &mut self,
        vertex_index: VertexIndex,
        incident_edges: &[EdgeIndex],
        parity: bool,
    ) -> Option<Vec<VarIndex>>;

    /// row operations
    fn xor_row(&mut self, target: RowIndex, source: RowIndex);
    fn swap_row(&mut self, a: RowIndex, b: RowIndex);

    /// view the raw matrix
    fn get_lhs(&self, row: RowIndex, var_index: VarIndex) -> bool;
    fn get_rhs(&self, row: RowIndex) -> bool;
}

pub trait MatrixView {
    /// the number of columns: to get the `var_index` of each column,
    /// use `var_of()`
    fn columns(&self) -> usize;

    /// get the `var_index` in the basic matrix
    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex;

    /// the number of rows: rows always have indices 0..rows
    fn rows(&self) -> usize;

    /// get edge index from the var_index
    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeIndex;

    fn get_view_edges(&self) -> Vec<EdgeIndex> {
        (0..self.columns())
            .map(|column: usize| {
                let var_index = self.column_to_var_index(column);
                self.var_to_edge_index(var_index)
            })
            .collect()
    }
}

pub trait MatrixTight {
    /// update the edges' tightness
    fn update_edge_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool);

    /// this function is generally slow given that
    fn is_tight(&self, edge_index: usize) -> bool;
}
