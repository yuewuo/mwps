//! Parity Matrix
//!
//! A parity matrix containing all variables and constraints in a cluster, but can have multiple "views" of the same matrix focusing on part of the variables
//! (forcing other variables to be zero)
//!
//! The matrix can also be plotted with specific order of rows and columns for better visualization purpose
//!

use crate::dual_module::*;
use crate::hyper_decoding_graph::*;
use crate::parity_matrix_visualize::*;
use crate::prettytable::*;
use crate::util::*;
use derivative::Derivative;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

pub type BitArrayUnit = usize;
pub const BIT_UNIT_LENGTH: usize = std::mem::size_of::<BitArrayUnit>() * 8;
pub type DualVariableTag = usize;

pub struct HairView<'a> {
    /// hair edges
    pub hairs: &'a [EdgeIndex],
    /// starting index of hair edges
    hair_index: usize,
    /// the corresponding echelon form
    pub echelon: EchelonView<'a>,
}

impl<'a> HairView<'a> {
    pub fn new(matrix: &'a mut ParityMatrix, hairs: &'a [EdgeIndex]) -> Self {
        debug_assert!(!hairs.is_empty(), "hair edges should not be empty");
        let (edges, hair_index) = matrix.hair_edges_to_reorder(hairs);
        let echelon = EchelonView::new_reordered(matrix, edges);
        Self {
            hairs,
            hair_index,
            echelon,
        }
    }
}

impl<'a> MatrixView for HairView<'a> {
    fn display_table_title(&self) -> Row {
        self.echelon.display_table_title()
    }
    fn display_table_body(&self) -> Vec<Row> {
        self.echelon.display_table_body()
    }
    fn display_table(&self) -> Table {
        let table = self.echelon.display_table();
        // only display the hair part
        let edge_index = self.echelon.edges[self.hair_index];
        let var_index = *self.echelon.matrix.edges.get(&edge_index).unwrap();
        println!("{}", var_index);
        let (is_dependent, dependent_row) = self.echelon.matrix.echelon_column_info[var_index];
        assert!(is_dependent, "the first hair edge must be dependent in echelon form");
        // create new table
        // let mut rows = vec![];
        for row in &table {
            println!("{row:?}");
        }

        // for (row_index, row) in table.into_iter().enumerate() {
        //     rows.push(row);
        //     println!("{row:?}");
        // }
        // delete the previous rows before `dependent_row`

        // delete columns before `self.hair_index`
        println!("{is_dependent}, {dependent_row}");
        table
    }
}

pub struct EchelonView<'a> {
    /// the corresponding matrix, immutable until the EchelonView is dropped
    matrix: &'a mut ParityMatrix,
    /// reordered edges in this view
    pub edges: Vec<EdgeIndex>,
}

impl<'a> EchelonView<'a> {
    /// create an echelon view of a matrix
    pub fn new(matrix: &'a mut ParityMatrix) -> Self {
        let edges: Vec<_> = matrix.variables.iter().map(|(edge_index, _)| *edge_index).collect();
        Self::new_reordered(matrix, edges)
    }

    pub fn new_reordered(matrix: &'a mut ParityMatrix, edges: Vec<EdgeIndex>) -> Self {
        matrix.row_echelon_form_reordered(&edges);
        matrix.is_echelon_form = true;
        Self { matrix, edges }
    }

    pub fn get_matrix(&'a self) -> &'a ParityMatrix {
        self.matrix
    }

    pub fn get_tight_edges(&self) -> BTreeSet<EdgeIndex> {
        self.matrix.get_tight_edges()
    }

    pub fn get_vertices(&self) -> BTreeSet<VertexIndex> {
        self.matrix.vertices.clone()
    }

    pub fn satisfiable(&self) -> bool {
        self.matrix.echelon_satisfiable // guaranteed in echelon form
    }

    /// using only necessary edges to build a joint solution of all non-zero dual variables,
    ///     requiring all non-zero dual variables to get empty array when calling `get_implicit_shrink_edges`
    pub fn get_joint_solution(&mut self) -> Option<Subgraph> {
        if !self.satisfiable() {
            return None; // no joint solution is possible once all the implicit shrinks have been executed
        }
        // self.print();
        let mut joint_solution = vec![];
        for row_index in 0..self.matrix.echelon_effective_rows {
            if self.matrix.constraints[row_index].get_right() {
                let var_index = self.matrix.echelon_row_info[row_index];
                let (edge_index, _) = self.matrix.variables[var_index];
                joint_solution.push(edge_index);
            }
        }
        Some(Subgraph::new(joint_solution))
    }

    /// try every independent variables and try to minimize the overall primal objective function
    #[allow(clippy::unnecessary_cast)]
    pub fn get_joint_solution_local_minimum(&mut self, hypergraph: &SolverInitializer) -> Option<Subgraph> {
        if !self.satisfiable() {
            return None; // no joint solution is possible once all the implicit shrinks have been executed
        }
        let mut joint_solution = BTreeSet::new();
        for row_index in 0..self.matrix.echelon_effective_rows {
            if self.matrix.constraints[row_index].get_right() {
                let var_index = self.matrix.echelon_row_info[row_index];
                let (edge_index, _) = self.matrix.variables[var_index];
                joint_solution.insert(edge_index);
            }
        }
        let mut independent_variables = vec![];
        for var_index in 0..self.matrix.variables.len() {
            if !self.matrix.is_tight(var_index) {
                continue; // ignore this edge
            }
            let (is_dependent, _) = self.matrix.echelon_column_info[var_index];
            if !is_dependent {
                independent_variables.push(var_index);
            }
        }
        let mut primal_objective_value = 0;
        for &edge_index in joint_solution.iter() {
            primal_objective_value += hypergraph.weighted_edges[edge_index as usize].weight;
        }
        let mut pending_flip_edge_indices = vec![];
        let mut is_local_minimum = false;
        while !is_local_minimum {
            is_local_minimum = true;
            // try every independent variable and find a local minimum
            for &var_index in independent_variables.iter() {
                pending_flip_edge_indices.clear();
                let (edge_index, _) = self.matrix.variables[var_index];
                let mut primal_delta = (hypergraph.weighted_edges[edge_index as usize].weight as isize)
                    * (if joint_solution.contains(&edge_index) { -1 } else { 1 });
                pending_flip_edge_indices.push(edge_index);
                for row in 0..self.matrix.echelon_effective_rows {
                    if self.matrix.constraints[row].get_left(var_index) {
                        let flip_var_index = self.matrix.echelon_row_info[row];
                        debug_assert!(flip_var_index < var_index);
                        let (flip_edge_index, _) = self.matrix.variables[flip_var_index];
                        primal_delta += (hypergraph.weighted_edges[flip_edge_index as usize].weight as isize)
                            * (if joint_solution.contains(&flip_edge_index) { -1 } else { 1 });
                        pending_flip_edge_indices.push(flip_edge_index);
                    }
                }
                if primal_delta < 0 {
                    primal_objective_value = (primal_objective_value as isize + primal_delta) as usize;
                    for &edge_index in pending_flip_edge_indices.iter() {
                        if joint_solution.contains(&edge_index) {
                            joint_solution.remove(&edge_index);
                        } else {
                            joint_solution.insert(edge_index);
                        }
                    }
                    is_local_minimum = false;
                    break; // loop over again
                }
            }
        }
        Some(Subgraph::new(joint_solution.into_iter().collect()))
    }
}

impl<'a> MatrixView for EchelonView<'a> {
    fn display_table_title(&self) -> Row {
        self.matrix.display_table_title()
    }
    fn display_table_body(&self) -> Vec<Row> {
        self.matrix.display_table_body()
    }
}

impl<'a> Drop for EchelonView<'a> {
    fn drop(&mut self) {
        // out of the echelon view
        self.matrix.is_echelon_form = false
    }
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
    pub variables: Vec<(EdgeIndex, bool)>,
    /// the constraints
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub constraints: Vec<ParityRow>,
    /// information about the matrix when it's formatted into the Echelon form:
    /// (is_dependent, if dependent the only "1" position row)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub echelon_column_info: Vec<(bool, usize)>,
    /// the number of effective rows
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub echelon_effective_rows: usize,
    /// whether it's a satisfiable matrix, only valid when `is_echelon_form` is true
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub echelon_satisfiable: bool,
    /// the leading "1" position column
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub echelon_row_info: Vec<usize>,
    /// whether it's in an echelon form (generally set by `EchelonView` and used by print function)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    is_echelon_form: bool,
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

/// a plugin is only allowed to modify the parity matrix in a constrained manner
pub struct ParityMatrixProtected<'a> {
    /// the parity matrix instance
    matrix: &'a mut ParityMatrix,
}

impl<'a> ParityMatrixProtected<'a> {
    pub fn new(matrix: &'a mut ParityMatrix) -> Self {
        Self { matrix }
    }

    pub fn get_matrix(&'a self) -> &'a ParityMatrix {
        self.matrix
    }

    pub fn echelon_view(&'a mut self) -> EchelonView<'a> {
        EchelonView::new(self.matrix)
    }

    pub fn echelon_view_reordered(&'a mut self, edges: Vec<EdgeIndex>) -> EchelonView<'a> {
        EchelonView::new_reordered(self.matrix, edges)
    }
}

/// optimize for small clusters where there are no more than 63 edges
#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct ParityRow {
    /// the first BIT_UNIT_LENGTH-1 edges are stored here, and the last bit is used the right hand bit value
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    first: BitArrayUnit,
    /// the other edges
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    others: Vec<BitArrayUnit>,
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl ParityMatrix {
    #[cfg_attr(feature = "python_binding", new)]
    pub fn new_py() -> Self {
        Self::new()
    }

    /// when you're sure no phantom edges will be dynamically included, then this matrix is faster; otherwise it might panic
    #[cfg_attr(feature = "python_binding", staticmethod)]
    pub fn new_no_phantom() -> Self {
        let mut matrix = Self::new();
        matrix.keep_phantom_edges = false;
        matrix
    }

    pub fn add_variable(&mut self, edge_index: EdgeIndex) {
        // must remove from phantom edge no matter whether the edge is already in `self.edge` or not
        self.phantom_edges.remove(&edge_index); // mark as explicitly added edge
        if self.edges.contains_key(&edge_index) {
            return; // variable already exists
        }
        self.edges.insert(edge_index, self.variables.len());
        self.variables.push((edge_index, false));
        let variable_count = self.variables.len();
        if variable_count % BIT_UNIT_LENGTH == 0 {
            let others_len = variable_count / BIT_UNIT_LENGTH;
            for row in self.constraints.iter_mut() {
                debug_assert_eq!(row.others.len() + 1, others_len);
                row.others.push(0);
            }
        }
        self.echelon_column_info.push((false, 0));
    }

    pub fn add_variable_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.add_variable(edge_index);
        self.update_edge_tightness(edge_index, is_tight);
    }

    pub fn add_tight_variable(&mut self, edge_index: EdgeIndex) {
        self.add_variable_tightness(edge_index, true)
    }

    pub fn update_edge_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        let var_index = *self.edges.get(&edge_index).expect("edge must be a variable");
        self.variables[var_index].1 = is_tight;
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "update_edges_tightness")]
    pub fn update_edges_tightness_py(&mut self, edges: Vec<EdgeIndex>, is_tight: bool) {
        self.update_edges_tightness(&edges, is_tight)
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

    pub fn get_edge_indices(&self) -> Vec<EdgeIndex> {
        self.variables.iter().map(|(edge_index, _)| *edge_index).collect()
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "printstd_reordered")]
    pub fn printstd_reordered_py(&self, edges: Vec<EdgeIndex>) {
        self.printstd_reordered(&edges)
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "to_visualize_json")]
    pub fn to_visualize_json_py(&self) -> PyObject {
        json_to_pyobject(self.to_visualize_json(true))
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "row_echelon_form_reordered")]
    pub fn row_echelon_form_reordered_py(&mut self, edges: Vec<EdgeIndex>) {
        self.row_echelon_form_reordered(&edges)
    }

    pub fn check_is_satisfiable(&mut self) -> bool {
        EchelonView::new(self).satisfiable()
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "hair_edges_to_reorder")]
    pub fn hair_edges_to_reorder_py(&self, hair_edges: Vec<EdgeIndex>) -> (Vec<EdgeIndex>, usize) {
        self.hair_edges_to_reorder(&hair_edges)
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

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "add_implicit_shrink")]
    pub fn add_implicit_shrink_py(&mut self, shrink_edges: Vec<EdgeIndex>) {
        self.add_implicit_shrink(&shrink_edges)
    }

    pub fn clear_implicit_shrink(&mut self) {
        self.implicit_shrunk_edges.clear();
    }

    #[allow(clippy::unnecessary_cast)]
    fn is_tight(&self, var_index: usize) -> bool {
        let (edge_index, is_tight) = self.variables[var_index as usize];
        is_tight && !self.implicit_shrunk_edges.contains(&edge_index) && !self.phantom_edges.contains(&edge_index)
    }

    pub fn get_joint_solution(&mut self) -> Option<Subgraph> {
        EchelonView::new(self).get_joint_solution()
    }

    pub fn get_joint_solution_local_minimum(&mut self, hypergraph: &SolverInitializer) -> Option<Subgraph> {
        EchelonView::new(self).get_joint_solution_local_minimum(hypergraph)
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "add_parity_checks")]
    pub fn add_parity_checks_py(&mut self, odd_parity_checks: Vec<Vec<EdgeIndex>>, even_parity_checks: Vec<Vec<EdgeIndex>>) {
        self.add_parity_checks(&odd_parity_checks, &even_parity_checks)
    }

    #[cfg(feature = "python_binding")]
    fn __repr__(&self) -> String {
        // TODO: check IPython environment and use better display
        let edges = self.get_edge_indices();
        let table = self.display_table_reordered(&edges);
        format!("{table}")
    }
}

impl ParityMatrix {
    pub fn update_edges_tightness(&mut self, edges: &[EdgeIndex], is_tight: bool) {
        for edge_index in edges.iter() {
            let var_index = *self.edges.get(edge_index).expect("edge must be a variable");
            self.variables[var_index].1 = is_tight;
        }
    }

    /// update the parity matrix with tight edges in the dual module
    pub fn update_with_dual_module(&mut self, dual_module: &impl DualModuleImpl) {
        for (edge_index, is_tight) in self.variables.iter_mut() {
            *is_tight = dual_module.is_edge_tight(*edge_index);
        }
    }

    /// add a row to the parity matrix from a given vertex, automatically add phantom edges corresponding to this parity check
    pub fn add_parity_check_with_decoding_graph(&mut self, vertex_index: VertexIndex, decoding_graph: &DecodingHyperGraph) {
        if self.vertices.contains(&vertex_index) {
            return; // no need to add repeat constraint
        }
        let incident_edges = decoding_graph.get_vertex_neighbors(vertex_index);
        let parity = decoding_graph.is_vertex_defect(vertex_index);
        self.add_constraint(vertex_index, incident_edges, parity);
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
        self.echelon_row_info.push(0);
        self.echelon_effective_rows = self.constraints.len(); // by default all constraints are taking effect
    }

    pub fn display_table_reordered(&self, edges: &[EdgeIndex]) -> Table {
        let var_indices = self.edge_to_tight_var_indices(edges);
        let mut table = nice_look_table();
        table.set_titles(self.display_table_title_reordered(&var_indices));
        for row in self.display_table_body_reordered(&var_indices).into_iter() {
            table.add_row(row);
        }
        table
    }

    /// print edges (maybe a subset of edges)
    pub fn printstd_reordered(&self, edges: &[EdgeIndex]) {
        self.display_table_reordered(edges).printstd();
    }

    pub fn edge_to_tight_var_indices(&self, edges: &[EdgeIndex]) -> Vec<usize> {
        let mut var_indices = Vec::with_capacity(edges.len());
        for &edge_index in edges.iter() {
            let var_index = *self.edges.get(&edge_index).expect("edge must be a variable");
            if self.is_tight(var_index) {
                var_indices.push(var_index);
            }
        }
        var_indices
    }

    /// use the new EchelonView to access this function
    fn row_echelon_form_reordered(&mut self, edges: &[EdgeIndex]) {
        self.echelon_satisfiable = false;
        if self.constraints.is_empty() {
            // no parity requirement
            self.echelon_satisfiable = true;
            return;
        }
        let height = self.constraints.len();
        let var_indices = self.edge_to_tight_var_indices(edges);
        if var_indices.is_empty() {
            // no variable to satisfy any requirement
            self.echelon_satisfiable = !self.constraints.iter().any(|x| x.get_right()); // if any RHS=1, it cannot be satisfied
            return;
        }
        let width = var_indices.len();
        let mut lead = 0;
        for r in 0..height {
            if lead >= width {
                // no more variables
                self.echelon_satisfiable = r == height || (r..height).all(|row| !self.constraints[row].get_right());
                if self.echelon_satisfiable {
                    self.echelon_effective_rows = r;
                } else {
                    // find a row with rhs=1 and swap with r row
                    self.echelon_effective_rows = r + 1;
                    if !self.constraints[r].get_right() {
                        // make sure display is reasonable: RHS=1 and all LHS=0
                        for row in r + 1..height {
                            if self.constraints[row].get_right() {
                                let (slice_1, slice_2) = self.constraints.split_at_mut(r + 1);
                                std::mem::swap(&mut slice_1[r], &mut slice_2[row - r - 1]);
                                break;
                            }
                        }
                    }
                }
                return;
            }
            let mut i = r;
            while !self.constraints[i].get_left(var_indices[lead]) {
                // find first non-zero lead
                i += 1;
                if i == height {
                    i = r;
                    // couldn't find a leading 1 in this column, indicating this variable is an independent variable
                    self.echelon_column_info[var_indices[lead]] = (false, r);
                    lead += 1; // consider the next lead
                    if lead == width {
                        self.echelon_satisfiable = r == height || (r..height).all(|row| !self.constraints[row].get_right());
                        if self.echelon_satisfiable {
                            self.echelon_effective_rows = r;
                        } else {
                            // find a row with rhs=1 and swap with r row
                            self.echelon_effective_rows = r + 1;
                            if !self.constraints[r].get_right() {
                                // make sure display is reasonable: RHS=1 and all LHS=0
                                for row in r + 1..height {
                                    if self.constraints[row].get_right() {
                                        let (slice_1, slice_2) = self.constraints.split_at_mut(r + 1);
                                        std::mem::swap(&mut slice_1[r], &mut slice_2[row - r - 1]);
                                        break;
                                    }
                                }
                            }
                        }
                        return;
                    }
                }
            }
            if i != r {
                // implies r < i
                let (slice_1, slice_2) = self.constraints.split_at_mut(i);
                std::mem::swap(&mut slice_1[r], &mut slice_2[0]);
            }
            for j in 0..height {
                if j != r && self.constraints[j].get_left(var_indices[lead]) {
                    self.xor_row(j, r);
                }
            }
            self.echelon_row_info[r] = var_indices[lead];
            self.echelon_column_info[var_indices[lead]] = (true, r);
            self.echelon_effective_rows = r + 1;
            lead += 1;
        }
        while lead < width {
            self.echelon_column_info[var_indices[lead]] = (false, height - 1);
            lead += 1;
        }
        self.echelon_satisfiable = true;
    }

    /// create the reorder edges and also the starting index of hair edges
    pub fn hair_edges_to_reorder(&self, hair_edges: &[EdgeIndex]) -> (Vec<EdgeIndex>, usize) {
        let mut hair_edges_set = BTreeSet::new();
        for hair_edge in hair_edges.iter() {
            assert!(!hair_edges_set.contains(hair_edge), "duplicate hair edge");
            hair_edges_set.insert(*hair_edge);
        }
        let mut edges = Vec::with_capacity(self.variables.len());
        for (&edge_index, &var_index) in self.edges.iter() {
            if self.is_tight(var_index) && !hair_edges.contains(&edge_index) {
                edges.push(edge_index);
            }
        }
        let start_index = edges.len();
        for &edge_index in hair_edges.iter() {
            let var_index = *self.edges.get(&edge_index).expect("edge must be a variable");
            if self.is_tight(var_index) {
                edges.push(edge_index);
            }
        }
        (edges, start_index)
    }

    /// deprecated
    /// return a set of edges that can shrink when needed, i.e. they can be view as not-tight edges
    ///     , None if this is already invalid cluster: indicating it's time to execute the previous implicit edges1
    pub fn get_implicit_shrink_edges(&mut self, hair_edges: &[EdgeIndex]) -> Option<Vec<EdgeIndex>> {
        debug_assert!(!hair_edges.is_empty(), "hair edges should not be empty");
        let (edges, hair_index) = self.hair_edges_to_reorder(hair_edges);
        let echelon = EchelonView::new_reordered(self, edges);
        echelon.printstd();
        if !echelon.satisfiable() {
            return None;
        }
        let mut first_dependent_1_hair_row_index = usize::MAX;
        for hair_edge_index in echelon.edges.iter().skip(hair_index) {
            let hair_var_index = *echelon.matrix.edges.get(hair_edge_index).unwrap();
            let (is_dependent, row_index) = echelon.matrix.echelon_column_info[hair_var_index];
            if is_dependent && echelon.matrix.constraints[row_index].get_right() {
                first_dependent_1_hair_row_index = row_index;
                break;
            }
        }
        assert!(
            first_dependent_1_hair_row_index != usize::MAX,
            "lemma 1: there exists at least one dependent variable in the hair edges with RHS=1"
        );
        // proof: if all hair edges are independent variable or dependent variable with RHS=0, there exists a solution with all hair edges non-selecting
        //     , violating the assumption of this is the hair of an invalid cluster
        // construct a list of shrink edges that are zero in at least one of the RHS=1 constraint rows,
        let mut implicit_shrink_edges = vec![];
        let row = &echelon.matrix.constraints[first_dependent_1_hair_row_index];
        for hair_edge_index in echelon.edges.iter().skip(hair_index) {
            let hair_var_index = *echelon.matrix.edges.get(hair_edge_index).unwrap();
            if !row.get_left(hair_var_index) {
                implicit_shrink_edges.push(*hair_edge_index);
            }
        }
        Some(implicit_shrink_edges)
    }

    /// these edges can shrink when needed, and record the possible shrink operation by `shrink_tag`
    pub fn add_implicit_shrink(&mut self, shrink_edges: &[EdgeIndex]) {
        for &edge_index in shrink_edges.iter() {
            self.implicit_shrunk_edges.insert(edge_index);
        }
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
    fn display_table_title_reordered(&self, var_indices: &[usize]) -> Row {
        let mut title_row = Row::empty();
        title_row.add_cell(
            Cell::new(if self.is_echelon_form { "Ec" } else { "" }).style_spec(if self.is_echelon_form {
                "brFg"
            } else {
                ""
            }),
        );
        for &var_index in var_indices.iter() {
            let (edge_index, _) = self.variables[var_index];
            // make sure edge index is a single column, to save space and be consistent
            let edge_index_str = format!("{edge_index}");
            let single_column_str: String = edge_index_str
                .chars()
                .enumerate()
                .flat_map(|(idx, c)| if idx == 0 { vec![c] } else { vec!['\n', c] })
                .collect();
            title_row.add_cell(Cell::new(single_column_str.as_str()).style_spec("brFr"));
        }
        title_row.add_cell(Cell::new(" = "));
        if self.is_echelon_form {
            title_row.add_cell(Cell::new("\u{25BC}"));
        }
        title_row
    }

    fn display_table_body_reordered(&self, var_indices: &[usize]) -> Vec<Row> {
        let mut rows = vec![];
        for (row_index, row) in self.constraints.iter().enumerate() {
            if self.is_echelon_form && row_index >= self.echelon_effective_rows {
                break;
            }
            let mut table_row = Row::empty();
            table_row.add_cell(Cell::new(format!("{row_index}.").as_str()).style_spec("brFb"));
            for &var_index in var_indices.iter() {
                table_row.add_cell(Cell::new(if row.get_left(var_index) { "1" } else { " " }));
            }
            table_row.add_cell(Cell::new(if row.get_right() { " 1 " } else { "   " }));
            if self.is_echelon_form && row_index < self.echelon_effective_rows {
                table_row.add_cell(
                    Cell::new(format!("{}", self.variables[self.echelon_row_info[row_index]].0).as_str()).style_spec("irFr"),
                );
            }
            rows.push(table_row);
        }
        if self.is_echelon_form {
            let mut table_row = Row::empty();
            table_row.add_cell(Cell::new(" \u{25B6}"));
            for &var_index in var_indices.iter() {
                let (is_dependent, dependent_row) = self.echelon_column_info[var_index];
                let dependent_row_name: String = if is_dependent {
                    format!("{dependent_row}")
                        .chars()
                        .enumerate()
                        .flat_map(|(idx, c)| if idx == 0 { vec![c] } else { vec!['\n', c] })
                        .collect()
                } else {
                    "*".to_string()
                };
                table_row.add_cell(Cell::new(dependent_row_name.as_str()).style_spec("irFb"));
            }
            table_row.add_cell(Cell::new("\u{25C0}  "));
            table_row.add_cell(Cell::new("\u{25B2}"));
            rows.push(table_row);
        }
        rows
    }
}

impl MatrixView for ParityMatrix {
    fn display_table_title(&self) -> Row {
        let edges = self.get_edge_indices();
        let var_indices = self.edge_to_tight_var_indices(&edges);
        self.display_table_title_reordered(&var_indices)
    }
    fn display_table_body(&self) -> Vec<Row> {
        let edges = self.get_edge_indices();
        let var_indices = self.edge_to_tight_var_indices(&edges);
        self.display_table_body_reordered(&var_indices)
    }
}

impl ParityRow {
    pub fn new_length(variable_count: usize) -> Self {
        let mut row = ParityRow::new();
        let others_len = variable_count / BIT_UNIT_LENGTH;
        if others_len > 0 {
            row.others = vec![0; others_len];
        }
        row
    }

    pub fn set_left(&mut self, var_index: usize, value: bool) {
        if var_index < BIT_UNIT_LENGTH - 1 {
            // common case
            if value {
                self.first |= 0x01 << var_index;
            } else {
                self.first &= !(0x01 << var_index);
            }
        } else {
            let bias_index = var_index - (BIT_UNIT_LENGTH - 1);
            let others_idx = bias_index / BIT_UNIT_LENGTH;
            let bit_idx = bias_index % BIT_UNIT_LENGTH;
            if value {
                self.others[others_idx] |= 0x01 << bit_idx;
            } else {
                self.others[others_idx] &= (!0x01) << bit_idx;
            }
        }
    }

    pub fn get_left(&self, var_index: usize) -> bool {
        if var_index < BIT_UNIT_LENGTH - 1 {
            // common case
            self.first & (0x01 << var_index) != 0
        } else {
            let bias_index = var_index - (BIT_UNIT_LENGTH - 1);
            let others_idx = bias_index / BIT_UNIT_LENGTH;
            let bit_idx = bias_index % BIT_UNIT_LENGTH;
            self.others[others_idx] & (0x01 << bit_idx) != 0
        }
    }

    pub fn set_right(&mut self, value: bool) {
        if value {
            self.first |= 0x01 << (BIT_UNIT_LENGTH - 1);
        } else {
            self.first &= !(0x01 << (BIT_UNIT_LENGTH - 1));
        }
    }

    pub fn get_right(&self) -> bool {
        self.first & (0x01 << (BIT_UNIT_LENGTH - 1)) != 0
    }

    pub fn add(&mut self, other: &Self) {
        debug_assert_eq!(self.others.len(), other.others.len(), "size must be the same");
        self.first ^= other.first;
        for i in 0..self.others.len() {
            self.others[i] ^= other.others[i];
        }
    }

    pub fn is_all_zero(&self) -> bool {
        if self.first != 0 {
            return false;
        }
        for &other in self.others.iter() {
            if other != 0 {
                return false;
            }
        }
        true
    }
}

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<ParityMatrix>()?;
    m.add_class::<ParityRow>()?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn parity_matrix_basic_1() {
        // cargo test --features=colorful parity_matrix_basic_1 -- --nocapture
        let mut matrix = ParityMatrix::new();
        for edge_index in 0..7 {
            matrix.add_tight_variable(edge_index);
        }
        matrix.add_constraint(0, &[0, 1], true);
        matrix.add_constraint(1, &[0, 2], false);
        matrix.add_constraint(2, &[2, 3, 5], false);
        matrix.add_constraint(3, &[1, 3, 4], false);
        matrix.add_constraint(4, &[4, 6], false);
        matrix.add_constraint(5, &[5, 6], true);
        matrix.printstd();
        let echelon = EchelonView::new(&mut matrix);
        echelon.printstd();
        drop(echelon);
        // focus on the middle dual, by letting them to be independent variables as much as possible
        let edges = vec![0, 1, 5, 6, 2, 3, 4];
        let echelon = EchelonView::new_reordered(&mut matrix, edges);
        echelon.printstd();
        drop(echelon);
        // focus on the first dual, by letting them to be independent variables as much as possible
        let edges = vec![2, 3, 4, 5, 6, 0, 1];
        let echelon = EchelonView::new_reordered(&mut matrix, edges);
        echelon.printstd();
        drop(echelon);
        // try a different order
        let edges = vec![2, 3, 4, 1, 0, 6, 5];
        let echelon = EchelonView::new_reordered(&mut matrix, edges);
        echelon.printstd();
        drop(echelon);
    }

    #[test]
    fn parity_matrix_basic_2() {
        // cargo test --features=colorful parity_matrix_basic_2 -- --nocapture
        let mut matrix = ParityMatrix::new();
        for edge_index in 0..15 {
            matrix.add_tight_variable(edge_index);
        }
        let odd_parity_checks = vec![vec![0, 3, 8, 12], vec![6, 7]];
        let even_parity_checks = vec![
            vec![1, 2],
            vec![2, 3, 4],
            vec![4, 5, 6],
            vec![0, 1, 14],
            vec![5, 8, 9],
            vec![7, 9],
            vec![13, 14],
            vec![11, 12, 13],
            vec![10, 11],
        ];
        matrix.add_parity_checks(&odd_parity_checks, &even_parity_checks);
        matrix.printstd();
        let hair_edges_1 = vec![0, 3, 8, 12];
        let hair_edges_2 = vec![1, 2, 4, 5, 9, 10, 11, 13, 14];
        let hair_edges_3 = vec![6, 7];

        let hair_view = HairView::new(&mut matrix, &hair_edges_1);
        hair_view.printstd();
        drop(hair_view);
        println!("the first dual variable");
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges_1).unwrap();
        matrix.printstd();
        assert!(implicit_shrink_edges.is_empty(), "no need to add implicit shrinks");
        println!("the second dual variable");
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges_2).unwrap();
        assert_eq!(implicit_shrink_edges, vec![1, 2, 10, 11, 13, 14]);
        // we need to add hair edges not in the necessary hair set as implicit shrinks
        //     , because there is a way to shrink them while maintaining the summation of dual
        matrix.add_implicit_shrink(&implicit_shrink_edges);
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges_2).unwrap();
        assert!(implicit_shrink_edges.is_empty(), "no need to add more implicit shrinks");
        println!("the third dual variable");
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges_3).unwrap();
        assert!(implicit_shrink_edges.is_empty(), "no need to add more implicit shrinks");
        // one more round to check if any edges can shrink
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges_1).unwrap();
        assert_eq!(implicit_shrink_edges, vec![0, 12]);
        matrix.add_implicit_shrink(&implicit_shrink_edges);
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges_1).unwrap();
        assert!(implicit_shrink_edges.is_empty(), "no need to add more implicit shrinks");
        let joint_solution = matrix.get_joint_solution().unwrap();
        assert_eq!(joint_solution, Subgraph::new(vec![3, 4, 6]), "we got some joint solution");
    }

    /// an example where the first hair edge might be independent variable: because it has nothing to do with outside
    #[test]
    fn parity_matrix_basic_3() {
        // cargo test --features=colorful parity_matrix_basic_3 -- --nocapture
        let mut matrix = ParityMatrix::new();
        for edge_index in 0..4 {
            matrix.add_tight_variable(edge_index);
        }
        let odd_parity_checks = vec![vec![0, 1], vec![3]];
        let even_parity_checks = vec![vec![0, 2], vec![1, 2, 3]];
        matrix.add_parity_checks(&odd_parity_checks, &even_parity_checks);
        matrix.printstd();
        let echelon = EchelonView::new(&mut matrix);
        echelon.printstd();
        drop(echelon);
        let hair_edges = vec![2, 3];
        matrix.get_implicit_shrink_edges(&hair_edges);
    }

    /// Notability MWPS design page 56: designed contrary case where although every dual
    /// variable has single-hair solution, they don't have a joint single-hair solution
    #[test]
    fn parity_matrix_basic_4() {
        // cargo test --features=colorful parity_matrix_basic_4 -- --nocapture
        let mut matrix = ParityMatrix::new();
        for edge_index in 0..14 {
            matrix.add_tight_variable(edge_index);
        }
        let odd_parity_checks = vec![
            vec![0, 8, 12, 13],
            vec![1, 8, 9, 13],
            vec![2, 8, 9, 10],
            vec![3, 8, 9, 10, 11],
            vec![4, 9, 10, 11, 12],
            vec![5, 10, 11, 12, 13],
            vec![6, 11, 12, 13],
            vec![7, 8, 9, 10, 11, 12, 13],
        ];
        matrix.add_parity_checks(&odd_parity_checks, &[]);
        matrix.printstd();
        let echelon = EchelonView::new(&mut matrix);
        echelon.printstd();
        drop(echelon);
        let hair_edges = vec![7, 8, 9, 10, 11, 12, 13];
        matrix.get_implicit_shrink_edges(&hair_edges);
        let hair_edges = vec![0, 1, 2, 3, 4, 5, 6];
        matrix.get_implicit_shrink_edges(&hair_edges);
        // then we use the method to create another set of dual variables
        matrix.update_edges_tightness(&[0, 1, 3, 4, 5, 7], false);
        let hair_edges = vec![2, 8, 9, 10];
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges).unwrap();
        assert_eq!(implicit_shrink_edges, vec![8, 9]);
        matrix.add_implicit_shrink(&implicit_shrink_edges);
        assert!(matrix.get_joint_solution().is_none());
        // then we don't have any joint solution after implicitly shrinking those edges
        matrix.clear_implicit_shrink();
        matrix.update_edges_tightness(&[0, 1, 3, 4, 5, 7], true);
        let hair_edges_orange = vec![0, 1, 3, 4, 5, 7, 2, 10];
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges_orange).unwrap();
        assert_eq!(implicit_shrink_edges, vec![5, 7]);
        matrix.add_implicit_shrink(&implicit_shrink_edges);
        let hair_edges_orange = vec![0, 1, 3, 4, 2, 10];
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges_orange).unwrap();
        assert_eq!(implicit_shrink_edges, vec![0, 1, 3, 4]);
        matrix.add_implicit_shrink(&implicit_shrink_edges);
        let hair_edges_orange = vec![2, 10];
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges_orange).unwrap();
        assert!(implicit_shrink_edges.is_empty());
        let hair_edges_yellow = vec![8, 9];
        let implicit_shrink_edges = matrix.get_implicit_shrink_edges(&hair_edges_yellow).unwrap();
        assert_eq!(implicit_shrink_edges, vec![9]);
        matrix.add_implicit_shrink(&implicit_shrink_edges);
        assert!(matrix.get_joint_solution().is_none());
        // then we don't have any joint solution after implicitly shrinking those edges
    }
}
