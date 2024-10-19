use super::interface::*;
use super::row::*;
use super::visualize::*;
use crate::util::*;
use derivative::Derivative;
use std::collections::{BTreeMap, BTreeSet};
use crate::pointers::FastClearUnsafePtr;

#[cfg(all(feature = "pointer", feature = "non-pq"))]
use crate::dual_module_serial::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(all(feature = "pointer", not(feature = "non-pq")))]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature="unsafe_pointer")]
use crate::pointers::UnsafePtr;

/// complete matrix considers a predefined set of edges and won't consider any other edges
#[cfg(feature="pointer")]
#[derive(Clone, Derivative)]
#[derivative(Default(new = "true"))]
pub struct CompleteMatrix {
    /// the vertices already maintained by this parity check
    vertices: BTreeSet<VertexWeak>,
    /// the edges maintained by this parity check, mapping to the local indices
    edges: BTreeMap<EdgeWeak, VarIndex>,
    /// variable index map to edge index
    variables: Vec<EdgeWeak>,
    constraints: Vec<ParityRow>,
}

#[cfg(not(feature = "pointer"))]
#[derive(Clone, Derivative)]
#[derivative(Default(new = "true"))]
pub struct CompleteMatrix {
    /// the vertices already maintained by this parity check
    vertices: BTreeSet<VertexIndex>,
    /// the edges maintained by this parity check, mapping to the local indices
    edges: BTreeMap<EdgeIndex, VarIndex>,
    /// variable index map to edge index
    variables: Vec<EdgeIndex>,
    constraints: Vec<ParityRow>,
}


#[cfg(feature = "pointer")]
impl MatrixBasic for CompleteMatrix {
    fn add_variable(&mut self, edge_weak: EdgeWeak) -> Option<VarIndex> {
        if self.edges.contains_key(&edge_weak) {
            // variable already exists
            return None;
        }
        if !self.constraints.is_empty() {
            panic!("complete matrix doesn't allow dynamic edges, please insert all edges at the beginning")
        }
        let var_index = self.variables.len();
        self.edges.insert(edge_weak.clone(), var_index);
        self.variables.push(edge_weak);
        Some(var_index)
    }

    fn add_constraint(
        &mut self,
        vertex_ptr: VertexPtr,
        // incident_edges: &[EdgeWeak],
        // parity: bool,
    ) -> Option<Vec<VarIndex>> {
        if self.vertices.contains(&vertex_ptr.downgrade()) {
            // no need to add repeat constraint
            return None;
        }
        self.vertices.insert(vertex_ptr.downgrade());
        let mut row = ParityRow::new_length(self.variables.len());
        let vertex = vertex_ptr.read_recursive_force();
        // let incident_edges = &vertex.edges;
        for edge_index in vertex.edges.iter() {
            if self.exists_edge(edge_index.clone()) {
                let var_index = self.edges[&edge_index];
                row.set_left(var_index, true);
            }
        }
        // let parity = vertex.is_defect;
        row.set_right(vertex.is_defect);
        self.constraints.push(row);
        // never add new edges
        None
    }

    /// row operations
    fn xor_row(&mut self, target: RowIndex, source: RowIndex) {
        ParityRow::xor_two_rows(&mut self.constraints, target, source)
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

    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeWeak {
        self.variables[var_index].clone()
    }

    fn edge_to_var_index(&self, edge_weak: EdgeWeak) -> Option<VarIndex> {
        self.edges.get(&edge_weak).cloned()
    }

    fn get_vertices(&self) -> BTreeSet<VertexWeak> {
        self.vertices.clone()
    }
}


#[cfg(not(feature = "pointer"))]
impl MatrixBasic for CompleteMatrix {
    fn add_variable(&mut self, edge_index: EdgeIndex) -> Option<VarIndex> {
        if self.edges.contains_key(&edge_index) {
            // variable already exists
            return None;
        }
        if !self.constraints.is_empty() {
            panic!("complete matrix doesn't allow dynamic edges, please insert all edges at the beginning")
        }
        let var_index = self.variables.len();
        self.edges.insert(edge_index, var_index);
        self.variables.push(edge_index);
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
        self.vertices.insert(vertex_index);
        let mut row = ParityRow::new_length(self.variables.len());
        for &edge_index in incident_edges.iter() {
            if self.exists_edge(edge_index) {
                let var_index = self.edges[&edge_index];
                row.set_left(var_index, true);
            }
        }
        row.set_right(parity);
        self.constraints.push(row);
        // never add new edges
        None
    }

    /// row operations
    fn xor_row(&mut self, target: RowIndex, source: RowIndex) {
        ParityRow::xor_two_rows(&mut self.constraints, target, source)
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

    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeIndex {
        self.variables[var_index]
    }

    fn edge_to_var_index(&self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.edges.get(&edge_index).cloned()
    }

    fn get_vertices(&self) -> BTreeSet<VertexIndex> {
        self.vertices.clone()
    }
}

impl MatrixView for CompleteMatrix {
    fn columns(&mut self) -> usize {
        self.variables.len()
    }

    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex {
        column
    }

    fn rows(&mut self) -> usize {
        self.constraints.len()
    }
}

impl VizTrait for CompleteMatrix {
    fn viz_table(&mut self) -> VizTable {
        VizTable::from(self)
    }
}

#[cfg(test)]
#[cfg(not(feature="pointer"))]
pub mod tests {
    use crate::matrix::Echelon;

    use super::*;

    #[test]
    fn complete_matrix_1() {
        // cargo test --features=colorful complete_matrix_1 -- --nocapture
        let mut matrix = CompleteMatrix::new();
        for edge_index in [1, 4, 12, 345] {
            matrix.add_variable(edge_index);
        }
        matrix.printstd();
        assert_eq!(
            matrix.printstd_str(),
            "\
┌┬─┬─┬─┬─┬───┐
┊┊1┊4┊1┊3┊ = ┊
┊┊ ┊ ┊2┊4┊   ┊
┊┊ ┊ ┊ ┊5┊   ┊
╞╪═╪═╪═╪═╪═══╡
└┴─┴─┴─┴─┴───┘
"
        );
        matrix.add_constraint(0, &[1, 4, 12], true);
        matrix.add_constraint(1, &[4, 345], false);
        matrix.add_constraint(2, &[1, 345], true);
        matrix.printstd();
        assert_eq!(
            matrix.clone().printstd_str(),
            "\
┌─┬─┬─┬─┬─┬───┐
┊ ┊1┊4┊1┊3┊ = ┊
┊ ┊ ┊ ┊2┊4┊   ┊
┊ ┊ ┊ ┊ ┊5┊   ┊
╞═╪═╪═╪═╪═╪═══╡
┊0┊1┊1┊1┊ ┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊1┊ ┊1┊ ┊1┊   ┊
├─┼─┼─┼─┼─┼───┤
┊2┊1┊ ┊ ┊1┊ 1 ┊
└─┴─┴─┴─┴─┴───┘
"
        );
        assert_eq!(matrix.get_vertices(), [0, 1, 2].into());
        assert_eq!(matrix.get_view_edges(), [1, 4, 12, 345]);
    }

    #[test]
    fn complete_matrix_should_not_add_repeated_constraint() {
        // cargo test --features=colorful complete_matrix_should_not_add_repeated_constraint -- --nocapture
        let mut matrix = CompleteMatrix::new();
        for edge_index in [1, 4, 8] {
            matrix.add_variable(edge_index);
        }
        assert_eq!(matrix.add_constraint(0, &[1, 4, 8], false), None);
        assert_eq!(matrix.add_constraint(1, &[4, 8], true), None);
        assert_eq!(matrix.add_constraint(0, &[4], true), None); // repeated
        matrix.printstd();
        assert_eq!(
            matrix.clone().printstd_str(),
            "\
┌─┬─┬─┬─┬───┐
┊ ┊1┊4┊8┊ = ┊
╞═╪═╪═╪═╪═══╡
┊0┊1┊1┊1┊   ┊
├─┼─┼─┼─┼───┤
┊1┊ ┊1┊1┊ 1 ┊
└─┴─┴─┴─┴───┘
"
        );
    }

    #[test]
    fn complete_matrix_row_operations() {
        // cargo test --features=colorful complete_matrix_row_operations -- --nocapture
        let mut matrix = CompleteMatrix::new();
        for edge_index in [1, 4, 6, 9] {
            matrix.add_variable(edge_index);
        }
        matrix.add_constraint(0, &[1, 4, 6], true);
        matrix.add_constraint(1, &[4, 9], false);
        matrix.add_constraint(2, &[1, 9], true);
        matrix.printstd();
        assert_eq!(
            matrix.clone().printstd_str(),
            "\
┌─┬─┬─┬─┬─┬───┐
┊ ┊1┊4┊6┊9┊ = ┊
╞═╪═╪═╪═╪═╪═══╡
┊0┊1┊1┊1┊ ┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊1┊ ┊1┊ ┊1┊   ┊
├─┼─┼─┼─┼─┼───┤
┊2┊1┊ ┊ ┊1┊ 1 ┊
└─┴─┴─┴─┴─┴───┘
"
        );
        matrix.swap_row(2, 1);
        matrix.printstd();
        assert_eq!(
            matrix.clone().printstd_str(),
            "\
┌─┬─┬─┬─┬─┬───┐
┊ ┊1┊4┊6┊9┊ = ┊
╞═╪═╪═╪═╪═╪═══╡
┊0┊1┊1┊1┊ ┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊1┊1┊ ┊ ┊1┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊2┊ ┊1┊ ┊1┊   ┊
└─┴─┴─┴─┴─┴───┘
"
        );
        matrix.xor_row(0, 1);
        matrix.printstd();
        assert_eq!(
            matrix.clone().printstd_str(),
            "\
┌─┬─┬─┬─┬─┬───┐
┊ ┊1┊4┊6┊9┊ = ┊
╞═╪═╪═╪═╪═╪═══╡
┊0┊ ┊1┊1┊1┊   ┊
├─┼─┼─┼─┼─┼───┤
┊1┊1┊ ┊ ┊1┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊2┊ ┊1┊ ┊1┊   ┊
└─┴─┴─┴─┴─┴───┘
"
        );
    }

    #[test]
    fn complete_matrix_manual_echelon() {
        // cargo test --features=colorful complete_matrix_manual_echelon -- --nocapture
        let mut matrix = CompleteMatrix::new();
        for edge_index in [1, 4, 6, 9, 9, 6, 4, 1] {
            matrix.add_variable(edge_index);
        }
        matrix.add_constraint(0, &[1, 4, 6], true);
        matrix.add_constraint(1, &[4, 9], false);
        matrix.add_constraint(2, &[1, 9], true);
        matrix.xor_row(2, 0);
        matrix.xor_row(0, 1);
        matrix.xor_row(2, 1);
        matrix.xor_row(0, 2);
        matrix.printstd();
        assert_eq!(
            matrix.clone().printstd_str(),
            "\
┌─┬─┬─┬─┬─┬───┐
┊ ┊1┊4┊6┊9┊ = ┊
╞═╪═╪═╪═╪═╪═══╡
┊0┊1┊ ┊ ┊1┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊1┊ ┊1┊ ┊1┊   ┊
├─┼─┼─┼─┼─┼───┤
┊2┊ ┊ ┊1┊ ┊   ┊
└─┴─┴─┴─┴─┴───┘
"
        );
    }

    #[test]
    fn complete_matrix_automatic_echelon() {
        // cargo test --features=colorful complete_matrix_automatic_echelon -- --nocapture
        let mut matrix = Echelon::<CompleteMatrix>::new();
        for edge_index in [1, 4, 6, 9] {
            matrix.add_variable(edge_index);
        }
        matrix.add_constraint(0, &[1, 4, 6, 11, 12], true);
        matrix.add_constraint(1, &[4, 9, 23, 12], false);
        matrix.add_constraint(2, &[1, 9, 11], true);
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
    }

    #[test]
    #[should_panic]
    fn complete_matrix_dynamic_variables_forbidden() {
        // cargo test complete_matrix_dynamic_variables_forbidden -- --nocapture
        let mut matrix = Echelon::<CompleteMatrix>::new();
        for edge_index in [1, 4, 6, 9] {
            matrix.add_variable(edge_index);
        }
        matrix.add_constraint(0, &[1, 4, 6], true);
        matrix.add_constraint(1, &[4, 9], false);
        matrix.add_constraint(2, &[1, 9], true);
        matrix.add_variable(2);
    }
}
