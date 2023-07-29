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
use derivative::Derivative;
use std::collections::BTreeSet;

pub type VarIndex = usize;
pub type RowIndex = usize;
pub type ColumnIndex = usize;

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

    /// get edge index from the var_index
    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeIndex;

    fn edge_to_var_index(&self, edge_index: EdgeIndex) -> Option<VarIndex>;

    fn exists_edge(&self, edge_index: EdgeIndex) -> bool {
        self.edge_to_var_index(edge_index).is_some()
    }
}

pub trait MatrixView: MatrixBasic {
    /// the number of columns: to get the `var_index` of each column,
    /// use `column_to_var_index()`; here the mutable reference enables
    /// lazy update of the internal data structure
    fn columns(&mut self) -> usize;

    /// get the `var_index` in the basic matrix
    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex;

    fn column_to_edge_index(&self, column: ColumnIndex) -> EdgeIndex {
        let var_index = self.column_to_var_index(column);
        self.var_to_edge_index(var_index)
    }

    /// the number of rows: rows always have indices 0..rows
    fn rows(&mut self) -> usize;

    fn get_view_edges(&mut self) -> Vec<EdgeIndex> {
        (0..self.columns())
            .map(|column: usize| {
                let var_index = self.column_to_var_index(column);
                self.var_to_edge_index(var_index)
            })
            .collect()
    }

    fn var_to_column_index(&mut self, var_index: VarIndex) -> Option<ColumnIndex> {
        (0..self.columns()).find(|&column| self.column_to_var_index(column) == var_index)
    }

    fn edge_to_column_index(&mut self, edge_index: EdgeIndex) -> Option<ColumnIndex> {
        let var_index = self.edge_to_var_index(edge_index)?;
        self.var_to_column_index(var_index)
    }
}

pub trait MatrixTight: MatrixBasic {
    fn update_edge_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool);
    fn is_tight(&self, edge_index: usize) -> bool;

    fn add_variable_with_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.add_variable(edge_index);
        self.update_edge_tightness(edge_index, is_tight);
    }

    fn add_tight_variable(&mut self, edge_index: EdgeIndex) {
        self.add_variable_with_tightness(edge_index, true)
    }
}

pub trait MatrixTail {
    fn get_tail_edges(&self) -> &BTreeSet<EdgeIndex>;
    fn get_tail_edges_mut(&mut self) -> &mut BTreeSet<EdgeIndex>;

    fn set_tail_edges<'a, Iter>(&mut self, iter: Iter)
    where
        Iter: Iterator<Item = &'a EdgeIndex>,
    {
        let tail_edges = self.get_tail_edges_mut();
        tail_edges.clear();
        for &edge_index in iter {
            tail_edges.insert(edge_index);
        }
    }

    fn get_tail_edges_vec(&self) -> Vec<EdgeIndex> {
        let mut edges: Vec<EdgeIndex> = self.get_tail_edges().iter().cloned().collect();
        edges.sort();
        edges
    }
}

pub trait MatrixEchelon {
    fn get_echelon_info(&mut self) -> &EchelonInfo;
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct EchelonInfo {
    /// whether it's a satisfiable matrix, only valid when `is_echelon_form` is true
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub satisfiable: bool,
    /// (is_dependent, if dependent the only "1" position row)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub columns: Vec<ColumnInfo>,
    /// the number of effective rows
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub effective_rows: usize,
    /// the leading "1" position column
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub rows: Vec<RowInfo>,
}

#[derive(Clone, Copy, Derivative, PartialEq, Eq)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct ColumnInfo {
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub row: RowIndex,
}

impl ColumnInfo {
    pub fn not_dependent() -> Self {
        Self { row: RowIndex::MAX }
    }
    pub fn set(&mut self, row: RowIndex) {
        debug_assert!(row != RowIndex::MAX);
        self.row = row;
    }
    pub fn is_dependent(&self) -> bool {
        self.row != RowIndex::MAX
    }
    pub fn set_not_dependent(&mut self) {
        self.row = RowIndex::MAX;
    }
}

impl std::fmt::Debug for ColumnInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if !self.is_dependent() {
            write!(f, "Row(*)")
        } else {
            write!(f, "Row({})", self.row)
        }
    }
}

#[derive(Clone, Copy, Derivative, PartialEq, Eq)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct RowInfo {
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub column: ColumnIndex,
}

impl RowInfo {
    pub fn no_leading() -> Self {
        Self {
            column: ColumnIndex::MAX,
        }
    }
    pub fn set(&mut self, column: ColumnIndex) {
        debug_assert!(column != ColumnIndex::MAX);
        self.column = column;
    }
    pub fn has_leading(&self) -> bool {
        self.column != ColumnIndex::MAX
    }
    pub fn set_no_leading(&mut self) {
        self.column = ColumnIndex::MAX;
    }
}

impl std::fmt::Debug for RowInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if !self.has_leading() {
            write!(f, "Col(*)")
        } else {
            write!(f, "Col({})", self.column)
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::super::basic_matrix::*;
    use super::super::tight::*;
    use super::*;
    use crate::matrix::VizTrait;

    type TightMatrix = Tight<BasicMatrix>;

    #[test]
    fn matrix_interface_simple() {
        // cargo test --features=colorful matrix_interface_simple -- --nocapture
        let mut matrix = TightMatrix::new();
        matrix.add_tight_variable(233);
        matrix.add_tight_variable(14);
        matrix.add_variable(68);
        matrix.add_tight_variable(75);
        matrix.printstd();
        assert_eq!(matrix.get_view_edges(), [233, 14, 75]);
        assert_eq!(matrix.var_to_column_index(0), Some(0));
        assert_eq!(matrix.var_to_column_index(1), Some(1));
        assert_eq!(matrix.var_to_column_index(2), None);
        assert_eq!(matrix.var_to_column_index(3), Some(2));
        assert_eq!(matrix.edge_to_column_index(233), Some(0));
        assert_eq!(matrix.edge_to_column_index(14), Some(1));
        assert_eq!(matrix.edge_to_column_index(68), None);
        assert_eq!(matrix.edge_to_column_index(75), Some(2));
        assert_eq!(matrix.edge_to_column_index(666), None);
    }

    #[test]
    fn matrix_interface_echelon_info() {
        // cargo test matrix_interface_echelon_info -- --nocapture
        let mut column_info = ColumnInfo::new();
        column_info.set(13);
        assert_eq!(format!("{column_info:?}"), "Row(13)");
        column_info.set_not_dependent();
        assert_eq!(format!("{column_info:?}"), "Row(*)");
        assert_eq!(format!("{:?}", column_info.clone()), "Row(*)");
        let mut row_info = RowInfo::new();
        row_info.set(13);
        assert_eq!(format!("{row_info:?}"), "Col(13)");
        row_info.set_no_leading();
        assert_eq!(format!("{row_info:?}"), "Col(*)");
        assert_eq!(format!("{:?}", row_info.clone()), "Col(*)");
        let echelon_info = EchelonInfo::new();
        println!("echelon_info: {echelon_info:?}");
    }
}
