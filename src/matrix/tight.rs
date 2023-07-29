use super::matrix_interface::*;
use crate::util::*;
use derivative::Derivative;
use std::collections::HashSet;

#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
pub struct Tight<M> {
    pub base: M,
    /// the set of tight edges: should be a relatively small set
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub tight_edges: HashSet<EdgeIndex>,
    /// tight matrix gives a view of only tight edges, with sorted indices
    #[derivative(Default(value = "true"))]
    is_var_indices_outdated: bool,
    /// the internal store of var indices
    var_indices: KnownSafeRefCell<Vec<VarIndex>>,
}

impl<M> MatrixTight for Tight<M> {
    fn update_edge_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.is_var_indices_outdated = true;
        if is_tight {
            self.tight_edges.insert(edge_index);
        } else {
            self.tight_edges.remove(&edge_index);
        }
    }

    fn is_tight(&self, edge_index: usize) -> bool {
        self.tight_edges.contains(&edge_index)
    }
}

impl<M: MatrixBasic> MatrixBasic for Tight<M> {
    /// add an edge to the basic matrix, return the `var_index` if newly created
    fn add_variable(&mut self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.base.add_variable(edge_index)
    }

    /// add constraint will implicitly call `add_variable` if the edge is not added and return the indices of them
    fn add_constraint(
        &mut self,
        vertex_index: VertexIndex,
        incident_edges: &[EdgeIndex],
        parity: bool,
    ) -> Option<Vec<VarIndex>> {
        self.base.add_constraint(vertex_index, incident_edges, parity)
    }

    fn xor_row(&mut self, target: RowIndex, source: RowIndex) {
        self.base.xor_row(target, source)
    }
    fn swap_row(&mut self, a: RowIndex, b: RowIndex) {
        self.base.swap_row(a, b)
    }
    fn get_lhs(&self, row: RowIndex, var_index: VarIndex) -> bool {
        self.base.get_lhs(row, var_index)
    }
    fn get_rhs(&self, row: RowIndex) -> bool {
        self.base.get_rhs(row)
    }
}

impl<M: MatrixBasic + MatrixView> Tight<M> {
    fn force_update_var_indices(&self) {
        let mut var_indices = self.var_indices.borrow_mut();
        var_indices.clear();
        for column in 0..self.base.columns() {
            let var_index = self.base.column_to_var_index(column);
            let edge_index = self.base.var_to_edge_index(var_index);
            if self.is_tight(edge_index) {
                var_indices.push(var_index);
            }
        }
    }

    #[inline]
    fn use_var_indices(&self) {
        if self.is_var_indices_outdated {
            self.force_update_var_indices()
        }
    }
}

impl<M: MatrixBasic + MatrixView> MatrixView for Tight<M> {
    fn columns(&self) -> usize {
        self.use_var_indices();
        self.var_indices.borrow().len()
    }

    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex {
        debug_assert!(self.is_var_indices_outdated); // performance critical
        self.var_indices.borrow()[column]
    }

    fn rows(&self) -> usize {
        self.base.rows()
    }

    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeIndex {
        self.base.var_to_edge_index(var_index)
    }
}
