use super::*;
use crate::util::*;
use derivative::Derivative;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// the parity matrix that is necessary to satisfy parity requirement
#[derive(Clone, Debug)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct Variable {
    pub edge_index: EdgeIndex,
    pub is_tight: bool,
}

/// the parity matrix that is necessary to satisfy parity requirement
#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct ParityMatrix {
    /// the vertices already maintained by this parity check
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub vertices: BTreeSet<VertexIndex>,
    /// the edges maintained by this parity check, mapping to the local indices
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub edges: BTreeMap<EdgeIndex, usize>,
    /// variable index map to edge index and whether the edge is fully grown
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub variables: Vec<Variable>,
    /// the constraints
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub constraints: Vec<ParityRow>,
    /// edges that are affected by any implicit shrink event
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub implicit_shrunk_edges: BTreeSet<EdgeIndex>,
    /// edges that are not visible to outside, e.g. implicitly added to keep the constraints complete;
    /// these edges must be explicitly added to remove from phantom edges
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub phantom_edges: BTreeSet<EdgeIndex>,
    /// whether to keep phantom edges or not, default to True; needed when dynamically changing tight edges
    #[derivative(Default(value = "true"))]
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub keep_phantom_edges: bool,
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl ParityMatrix {
    pub fn add_variable(&mut self, edge_index: EdgeIndex) {
        // must remove from phantom edge no matter whether the edge is already in `self.edge` or not
        self.phantom_edges.remove(&edge_index); // mark as explicitly added edge
        if self.edges.contains_key(&edge_index) {
            return; // variable already exists
        }
        self.edges.insert(edge_index, self.variables.len());
        self.variables.push(Variable {
            edge_index,
            is_tight: false,
        });
        ParityRow::add_one_variable(&mut self.constraints, self.variables.len());
    }

    pub fn update_edge_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        let var_index = self.edge_to_var_index(edge_index);
        self.variables[var_index].is_tight = is_tight;
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "update_edges_tightness")]
    pub fn update_edges_tightness_py(&mut self, edges: Vec<EdgeIndex>, is_tight: bool) {
        self.update_edges_tightness(&edges, is_tight)
    }

    #[allow(clippy::unnecessary_cast)]
    fn is_tight(&self, var_index: usize) -> bool {
        let Variable { edge_index, is_tight } = self.variables[var_index as usize];
        is_tight && !self.implicit_shrunk_edges.contains(&edge_index) && !self.phantom_edges.contains(&edge_index)
    }

    pub fn get_tight_edges(&self) -> BTreeSet<EdgeIndex> {
        let mut tight_edges = BTreeSet::new();
        for (&edge_index, &var_index) in self.edges.iter() {
            if self.is_tight(var_index) {
                tight_edges.insert(edge_index);
            }
        }
        tight_edges
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "add_constraint")]
    pub fn add_constraint_py(&mut self, vertex_index: VertexIndex, incident_edges: Vec<EdgeIndex>, parity: bool) {
        self.add_constraint(vertex_index, &incident_edges, parity)
    }

    pub fn clear_implicit_shrink(&mut self) {
        self.implicit_shrunk_edges.clear();
    }
}

// simple helper functions
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl ParityMatrix {
    pub fn add_variable_with_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.add_variable(edge_index);
        self.update_edge_tightness(edge_index, is_tight);
    }

    pub fn add_tight_variable(&mut self, edge_index: EdgeIndex) {
        self.add_variable_with_tightness(edge_index, true)
    }

    pub fn get_edge_indices(&self) -> Vec<EdgeIndex> {
        self.variables.iter().map(|variable| variable.edge_index).collect()
    }

    pub fn xor_row(&mut self, target_row: usize, source_row: usize) {
        if target_row < source_row {
            let (slice_1, slice_2) = self.constraints.split_at_mut(source_row);
            let source = &slice_2[0];
            let target = &mut slice_1[target_row];
            target.add(source);
        } else {
            let (slice_1, slice_2) = self.constraints.split_at_mut(target_row);
            let source = &slice_1[source_row];
            let target = &mut slice_2[0];
            target.add(source);
        }
    }
}

// simple internal functions
impl ParityMatrix {
    #[inline]
    pub fn edge_to_var_index(&self, edge_index: EdgeIndex) -> usize {
        *self.edges.get(&edge_index).expect("edge must be a variable")
    }

    pub fn edge_to_tight_var_indices(&self, edges: &[EdgeIndex]) -> Vec<usize> {
        let mut var_indices = Vec::with_capacity(edges.len());
        for &edge_index in edges.iter() {
            let var_index = self.edge_to_var_index(edge_index);
            if self.is_tight(var_index) {
                var_indices.push(var_index);
            }
        }
        var_indices
    }

    /// a helper function to quickly add a few constraints, mainly used in tests
    pub fn add_parity_checks(&mut self, odd_parity_checks: &[Vec<EdgeIndex>], even_parity_checks: &[Vec<EdgeIndex>]) {
        let bias_1 = self.vertices.last().map(|idx| idx + 1).unwrap_or(0);
        for (vertex_index, incident_edges) in odd_parity_checks.iter().enumerate() {
            self.add_constraint(vertex_index as VertexIndex + bias_1, incident_edges, true);
        }
        let bias_2 = bias_1 + odd_parity_checks.len() as VertexIndex;
        for (vertex_index, incident_edges) in even_parity_checks.iter().enumerate() {
            self.add_constraint(vertex_index as VertexIndex + bias_2, incident_edges, false);
        }
    }
}

impl ParityMatrix {
    pub fn update_edges_tightness(&mut self, edges: &[EdgeIndex], is_tight: bool) {
        for &edge_index in edges.iter() {
            let var_index = self.edge_to_var_index(edge_index);
            self.variables[var_index].is_tight = is_tight;
        }
    }

    /// add a parity constraint coming from a vertex
    pub fn add_constraint(&mut self, vertex_index: VertexIndex, incident_edges: &[EdgeIndex], parity: bool) {
        if self.vertices.contains(&vertex_index) {
            return; // no need to add repeat constraint
        }
        self.vertices.insert(vertex_index);
        for &edge_index in incident_edges.iter() {
            if !self.edges.contains_key(&edge_index) && self.keep_phantom_edges {
                // add variable but mark as phantom edge
                self.add_variable(edge_index);
                self.phantom_edges.insert(edge_index);
            }
        }
        let mut row = ParityRow::new_length(self.variables.len());
        for &edge_index in incident_edges.iter() {
            if let Some(&var_index) = self.edges.get(&edge_index) {
                row.set_left(var_index, true);
            } else {
                assert!(!self.keep_phantom_edges, "unknown edge");
            }
        }
        row.set_right(parity);
        self.constraints.push(row);
    }
}

impl VizTrait for ParityMatrix {
    fn viz_table(&self) -> VizTable {
        let edges = self.get_edge_indices();
        let var_indices = self.edge_to_tight_var_indices(&edges);
        VizTable::new(self, &var_indices)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn parity_matrix_basic_matrix_1() {
        // cargo test --features=colorful parity_matrix_basic_matrix_1 -- --nocapture
        let mut basic_matrix: ParityMatrix = ParityMatrix::new();
        basic_matrix.printstd();
        assert_eq!(
            basic_matrix.printstd_str(),
            "\
┌┬───┐
┊┊ = ┊
╞╪═══╡
└┴───┘
"
        );
    }
}
