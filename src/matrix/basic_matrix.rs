use super::matrix_interface::*;
use super::row::*;
use crate::util::*;
use derivative::Derivative;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
pub struct BasicMatrix {
    /// the vertices already maintained by this parity check
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub vertices: HashSet<VertexIndex>,
    /// the edges maintained by this parity check, mapping to the local indices
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub edges: HashMap<EdgeIndex, VarIndex>,
    /// variable index map to edge index
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub variables: Vec<EdgeIndex>,
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub constraints: Vec<ParityRow>,
}

impl MatrixBasic for BasicMatrix {
    fn add_variable(&mut self, edge_index: EdgeIndex) -> Option<VarIndex> {
        if self.edges.contains_key(&edge_index) {
            // variable already exists
            return None;
        }
        let var_index = self.variables.len();
        self.edges.insert(edge_index, var_index);
        self.variables.push(edge_index);
        ParityRow::add_one_variable(&mut self.constraints, self.variables.len());
        Some(var_index)
    }

    fn add_constraint(
        &mut self,
        vertex_index: VertexIndex,
        incident_edges: &[EdgeIndex],
        parity: bool,
    ) -> Option<Vec<VarIndex>> {
        if self.vertices.contains(&vertex_index) {
            // no need to add repeat constraint
            return None;
        }
        let mut var_indices = None;
        self.vertices.insert(vertex_index);
        for &edge_index in incident_edges.iter() {
            if let Some(var_index) = self.add_variable(edge_index) {
                // this is a newly added edge
                var_indices.get_or_insert_with(Vec::new).push(var_index);
            }
        }
        let mut row = ParityRow::new_length(self.variables.len());
        for &edge_index in incident_edges.iter() {
            let var_index = self.edges[&edge_index];
            row.set_left(var_index, true);
        }
        row.set_right(parity);
        self.constraints.push(row);
        var_indices
    }

    /// row operations
    fn xor_row(&mut self, target: RowIndex, source: RowIndex) {
        if target < source {
            let (slice_1, slice_2) = self.constraints.split_at_mut(source);
            let source = &slice_2[0];
            let target = &mut slice_1[target];
            target.add(source);
        } else {
            let (slice_1, slice_2) = self.constraints.split_at_mut(target);
            let source = &slice_1[source];
            let target = &mut slice_2[0];
            target.add(source);
        }
    }

    fn swap_row(&mut self, a: RowIndex, b: RowIndex) {
        self.constraints.swap(a, b);
    }

    fn get_lhs(&self, row: RowIndex, var_index: VarIndex) -> bool {
        self.constraints[row].get_left(var_index)
    }

    fn get_rhs(&self, row: RowIndex) -> bool {
        self.constraints[row].get_right()
    }
}

impl MatrixView for BasicMatrix {
    fn columns(&self) -> usize {
        self.variables.len()
    }

    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex {
        column
    }

    fn rows(&self) -> usize {
        self.constraints.len()
    }

    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeIndex {
        self.edges[&var_index]
    }

    fn get_view_lhs(&self, row: RowIndex, column: ColumnIndex) -> bool {
        self.get_lhs(row, column)
    }
}
