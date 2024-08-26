use super::interface::*;
use super::row::*;
use super::visualize::*;
use crate::util::*;
use derivative::Derivative;
use weak_table::PtrWeakHashSet;
use weak_table::PtrWeakKeyHashMap;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};

#[derive(Clone, Derivative)]
#[derivative(Default(new = "true"))]
pub struct BasicMatrix {
    /// the vertices already maintained by this parity check
    pub vertices: BTreeSet<VertexPtr>,
    /// the edges maintained by this parity check, mapping to the local indices
    pub edges: BTreeMap<EdgePtr, VarIndex>,
    /// variable index map to edge index
    pub variables: Vec<EdgeWeak>,
    pub constraints: Vec<ParityRow>,
}

impl MatrixBasic for BasicMatrix {
    fn add_variable(&mut self, edge_weak: EdgeWeak) -> Option<VarIndex> {
        if self.edges.contains_key(&edge_weak.upgrade_force()) {
            // variable already exists
            return None;
        }
        let var_index = self.variables.len();
        self.edges.insert(edge_weak.upgrade_force(), var_index);
        self.variables.push(edge_weak.clone());
        ParityRow::add_one_variable(&mut self.constraints, self.variables.len());
        Some(var_index)
    }

    fn add_constraint(
        &mut self,
        vertex_weak: VertexWeak,
        incident_edges: &[EdgeWeak],
        parity: bool,
    ) -> Option<Vec<VarIndex>> {
        if self.vertices.contains(&vertex_weak.upgrade_force()) {
            // no need to add repeat constraint
            return None;
        }
        let mut var_indices = None;
        self.vertices.insert(vertex_weak.upgrade_force());
        for edge_weak in incident_edges.iter() {
            if let Some(var_index) = self.add_variable(edge_weak.clone()) {
                // this is a newly added edge
                var_indices.get_or_insert_with(Vec::new).push(var_index);
            }
        }
        let mut row = ParityRow::new_length(self.variables.len());
        for edge_weak in incident_edges.iter() {
            let var_index = self.edges[&edge_weak.upgrade_force()];
            row.set_left(var_index, true);
        }
        row.set_right(parity);
        self.constraints.push(row);
        var_indices
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
        self.edges.get(&edge_weak.upgrade_force()).cloned()
    }

    fn get_vertices(&self) -> BTreeSet<VertexPtr> {
        self.vertices.clone()
    }
}

impl MatrixView for BasicMatrix {
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

impl VizTrait for BasicMatrix {
    fn viz_table(&mut self) -> VizTable {
        VizTable::from(self)
    }
}

#[cfg(test)]
pub mod tests {
    use hashbrown::HashSet;
    use num_traits::Zero;

    use super::*;
    use crate::dual_module_pq::{EdgePtr, Edge, VertexPtr, Vertex};
    use crate::pointers::*;


    #[test]
    fn basic_matrix_1() {
        // cargo test --features=colorful basic_matrix_1 -- --nocapture
        let mut matrix = BasicMatrix::new();
        matrix.printstd();
        assert_eq!(
            matrix.printstd_str(),
            "\
┌┬───┐
┊┊ = ┊
╞╪═══╡
└┴───┘
"
        );

        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                    is_mirror: false,
                    fusion_done: false,
                    mirrored_vertices: vec![],
                })
            })
            .collect();

        let global_time = ArcRwLock::new_value(Rational::zero());

        // create edges
        let edges: Vec<EdgePtr> = vec![1, 4, 12, 345].into_iter()
            .map(|edge_index| {
                EdgePtr::new_value(Edge {
                    edge_index: edge_index,
                    weight: Rational::zero(),
                    dual_nodes: vec![],
                    vertices: vec![],
                    last_updated_time: Rational::zero(),
                    growth_at_last_updated_time: Rational::zero(),
                    grow_rate: Rational::zero(),
                    unit_index: None,
                    connected_to_boundary_vertex: false,
                    global_time: global_time.clone(),
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();

        matrix.add_variable(edges[0].downgrade());
        matrix.add_variable(edges[1].downgrade());
        matrix.add_variable(edges[2].downgrade());
        matrix.add_variable(edges[3].downgrade());
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
        matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
        matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[3].downgrade()], false);
        matrix.add_constraint(vertices[2].downgrade(), &[edges[0].downgrade(), edges[3].downgrade()], true);
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
        let matrix_vertices: HashSet<_> = matrix.get_vertices().into_iter().map(|v| v.upgradable_read().vertex_index).collect();
        assert_eq!(matrix_vertices, [0, 1, 2].into());
        assert_eq!(matrix.get_view_edges().into_iter().map(|e| e.upgrade_force().read_recursive().edge_index).collect::<Vec<usize>>(), [1, 4, 12, 345]);
    }

    #[test]
    fn basic_matrix_should_not_add_repeated_constraint() {
        // cargo test --features=colorful basic_matrix_should_not_add_repeated_constraint -- --nocapture
        let mut matrix = BasicMatrix::new();

        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                    is_mirror: false,
                    fusion_done: false,
                    mirrored_vertices: vec![],
                })
            })
            .collect();

        let global_time = ArcRwLock::new_value(Rational::zero());

        // create edges
        let edges: Vec<EdgePtr> = vec![1, 4, 8].into_iter()
            .map(|edge_index| {
                EdgePtr::new_value(Edge {
                    edge_index: edge_index,
                    weight: Rational::zero(),
                    dual_nodes: vec![],
                    vertices: vec![],
                    last_updated_time: Rational::zero(),
                    growth_at_last_updated_time: Rational::zero(),
                    grow_rate: Rational::zero(),
                    unit_index: None,
                    connected_to_boundary_vertex: false,
                    global_time: global_time.clone(),
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();


        assert_eq!(matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], false), Some(vec![0, 1, 2]));
        assert_eq!(matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[2].downgrade()], true), None);
        assert_eq!(matrix.add_constraint(vertices[0].downgrade(), &[edges[1].downgrade()], true), None); // repeated
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
    fn basic_matrix_row_operations() {
        // cargo test --features=colorful basic_matrix_row_operations -- --nocapture

        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                    is_mirror: false,
                    fusion_done: false,
                    mirrored_vertices: vec![],
                })
            })
            .collect();

        let global_time = ArcRwLock::new_value(Rational::zero());

        // create edges
        let edges: Vec<EdgePtr> = vec![1, 4, 6, 9].into_iter()
            .map(|edge_index| {
                EdgePtr::new_value(Edge {
                    edge_index: edge_index,
                    weight: Rational::zero(),
                    dual_nodes: vec![],
                    vertices: vec![],
                    last_updated_time: Rational::zero(),
                    growth_at_last_updated_time: Rational::zero(),
                    grow_rate: Rational::zero(),
                    unit_index: None,
                    connected_to_boundary_vertex: false,
                    global_time: global_time.clone(),
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();




        let mut matrix = BasicMatrix::new();
        matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
        matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[3].downgrade()], false);
        matrix.add_constraint(vertices[2].downgrade(), &[edges[0].downgrade(), edges[3].downgrade()], true);
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
    fn basic_matrix_manual_echelon() {
        // cargo test --features=colorful basic_matrix_manual_echelon -- --nocapture


        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                    is_mirror: false,
                    fusion_done: false,
                    mirrored_vertices: vec![],
                })
            })
            .collect();

        let global_time = ArcRwLock::new_value(Rational::zero());

        // create edges
        let edges: Vec<EdgePtr> = vec![1, 4, 6, 9].into_iter()
            .map(|edge_index| {
                EdgePtr::new_value(Edge {
                    edge_index: edge_index,
                    weight: Rational::zero(),
                    dual_nodes: vec![],
                    vertices: vec![],
                    last_updated_time: Rational::zero(),
                    growth_at_last_updated_time: Rational::zero(),
                    grow_rate: Rational::zero(),
                    unit_index: None,
                    connected_to_boundary_vertex: false,
                    global_time: global_time.clone(),
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();



        let mut matrix = BasicMatrix::new();
        matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
        matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[3].downgrade()], false);
        matrix.add_constraint(vertices[2].downgrade(), &[edges[0].downgrade(), edges[3].downgrade()], true);

        matrix.printstd();
        
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
}
