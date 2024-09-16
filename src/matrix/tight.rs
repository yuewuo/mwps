use super::interface::*;
use super::visualize::*;
use crate::util::*;
use derivative::Derivative;
use std::collections::BTreeSet;

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};


#[derive(Clone, Derivative)]
#[derivative(Default(new = "true"))]
pub struct Tight<M: MatrixView> {
    base: M,
    /// the set of tight edges: should be a relatively small set
    tight_edges: BTreeSet<EdgeWeak>,
    /// tight matrix gives a view of only tight edges, with sorted indices
    #[derivative(Default(value = "true"))]
    is_var_indices_outdated: bool,
    /// the internal store of var indices
    var_indices: Vec<VarIndex>,
}

impl<M: MatrixView> Tight<M> {
    pub fn get_base(&self) -> &M {
        &self.base
    }
}

impl<M: MatrixView> MatrixTight for Tight<M> {
    fn update_edge_tightness(&mut self, edge_weak: EdgeWeak, is_tight: bool) {
        debug_assert!(self.exists_edge(edge_weak.clone()));
        self.is_var_indices_outdated = true;
        if is_tight {
            self.tight_edges.insert(edge_weak.clone());
        } else {
            self.tight_edges.remove(&edge_weak);
        }
    }

    fn is_tight(&self, edge_weak: EdgeWeak) -> bool {
        debug_assert!(self.exists_edge(edge_weak.clone()));
        self.tight_edges.contains(&edge_weak)
    }
}

impl<M: MatrixView> MatrixBasic for Tight<M> {
    fn add_variable(&mut self, edge_weak: EdgeWeak) -> Option<VarIndex> {
        self.base.add_variable(edge_weak)
    }

    fn add_constraint(
        &mut self,
        vertex_ptr: VertexPtr,
        // incident_edges: &[EdgeWeak],
        // parity: bool,
    ) -> Option<Vec<VarIndex>> {
        self.base.add_constraint(vertex_ptr)
    }

    fn xor_row(&mut self, target: RowIndex, source: RowIndex) {
        self.base.xor_row(target, source)
    }
    fn swap_row(&mut self, a: RowIndex, b: RowIndex) {
        self.base.swap_row(a, b)
    }
    fn get_lhs(&self, row: RowIndex, var_index: VarIndex) -> bool {
        self.get_base().get_lhs(row, var_index)
    }
    fn get_rhs(&self, row: RowIndex) -> bool {
        self.get_base().get_rhs(row)
    }
    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeWeak {
        self.get_base().var_to_edge_index(var_index)
    }
    fn edge_to_var_index(&self, edge_weak: EdgeWeak) -> Option<VarIndex> {
        self.get_base().edge_to_var_index(edge_weak)
    }
    fn get_vertices(&self) -> BTreeSet<VertexWeak> {
        self.get_base().get_vertices()
    }
}

impl<M: MatrixView> Tight<M> {
    fn force_update_var_indices(&mut self) {
        self.var_indices.clear();
        for column in 0..self.base.columns() {
            let var_index = self.base.column_to_var_index(column);
            let edge_index = self.base.var_to_edge_index(var_index);
            if self.is_tight(edge_index) {
                self.var_indices.push(var_index);
            }
        }
    }

    fn var_indices_lazy_update(&mut self) {
        if self.is_var_indices_outdated {
            self.force_update_var_indices();
            self.is_var_indices_outdated = false;
        }
    }
}

impl<M: MatrixView> MatrixView for Tight<M> {
    fn columns(&mut self) -> usize {
        self.var_indices_lazy_update();
        self.var_indices.len()
    }

    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex {
        debug_assert!(!self.is_var_indices_outdated, "call `columns` first");
        self.var_indices[column]
    }

    fn rows(&mut self) -> usize {
        self.base.rows()
    }
}

impl<M: MatrixView> VizTrait for Tight<M> {
    fn viz_table(&mut self) -> VizTable {
        VizTable::from(self)
    }
}

// #[cfg(test)]
// pub mod tests {
//     use super::super::basic::*;
//     use super::*;

//     use crate::dual_module_pq::{EdgePtr, Edge, VertexPtr, Vertex};
//     use crate::pointers::*;
//     use num_traits::Zero;

//     type TightMatrix = Tight<BasicMatrix>;

//     #[test]
//     fn tight_matrix_1() {
//         // cargo test --features=colorful tight_matrix_1 -- --nocapture
//         let mut matrix = TightMatrix::new();

//         // create vertices 
//         let vertices: Vec<VertexPtr> = (0..3)
//         .map(|vertex_index| {
//             VertexPtr::new_value(Vertex {
//                 vertex_index,
//                 is_defect: false,
//                 edges: vec![],
//                 mirrored_vertices: vec![],
//             })
//         })
//         .collect();

//         // create edges
//         let edges: Vec<EdgePtr> = vec![1, 4, 6, 9].into_iter()
//             .map(|edge_index| {
//                 EdgePtr::new_value(Edge {
//                     edge_index: edge_index,
//                     weight: Rational::zero(),
//                     dual_nodes: vec![],
//                     vertices: vec![],
//                     last_updated_time: Rational::zero(),
//                     growth_at_last_updated_time: Rational::zero(),
//                     grow_rate: Rational::zero(),
//                     unit_index: None,
//                     connected_to_boundary_vertex: false,
//                     #[cfg(feature = "incr_lp")]
//                     cluster_weights: hashbrown::HashMap::new(),
//                 })
//             }).collect();

//         matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
//         matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[3].downgrade()], false);
//         matrix.add_constraint(vertices[2].downgrade(), &[edges[0].downgrade(), edges[3].downgrade()], true);
//         matrix.printstd();
//         // this is because by default all edges are not tight
//         assert_eq!(
//             matrix.clone().printstd_str(),
//             "\
// ┌─┬───┐
// ┊ ┊ = ┊
// ╞═╪═══╡
// ┊0┊ 1 ┊
// ├─┼───┤
// ┊1┊   ┊
// ├─┼───┤
// ┊2┊ 1 ┊
// └─┴───┘
// "
//         );
//         matrix.update_edge_tightness(edges[1].downgrade(), true);
//         matrix.update_edge_tightness(edges[3].downgrade(), true);
//         matrix.printstd();
//         assert_eq!(
//             matrix.clone().printstd_str(),
//             "\
// ┌─┬─┬─┬───┐
// ┊ ┊4┊9┊ = ┊
// ╞═╪═╪═╪═══╡
// ┊0┊1┊ ┊ 1 ┊
// ├─┼─┼─┼───┤
// ┊1┊1┊1┊   ┊
// ├─┼─┼─┼───┤
// ┊2┊ ┊1┊ 1 ┊
// └─┴─┴─┴───┘
// "
//         );
//         matrix.update_edge_tightness(edges[3].downgrade(), false);
//         matrix.printstd();
//         assert_eq!(
//             matrix.clone().printstd_str(),
//             "\
// ┌─┬─┬───┐
// ┊ ┊4┊ = ┊
// ╞═╪═╪═══╡
// ┊0┊1┊ 1 ┊
// ├─┼─┼───┤
// ┊1┊1┊   ┊
// ├─┼─┼───┤
// ┊2┊ ┊ 1 ┊
// └─┴─┴───┘
// "
//         );
//     }

//     #[test]
//     #[should_panic]
//     fn tight_matrix_cannot_set_nonexistent_edge() {
//         // cargo test tight_matrix_cannot_set_nonexistent_edge -- --nocapture
//         let mut matrix = TightMatrix::new();

//         // create vertices 
//         let vertices: Vec<VertexPtr> = (0..3)
//         .map(|vertex_index| {
//             VertexPtr::new_value(Vertex {
//                 vertex_index,
//                 is_defect: false,
//                 edges: vec![],
//                 mirrored_vertices: vec![],
//             })
//         })
//         .collect();

//         // create edges
//         let edges: Vec<EdgePtr> = vec![1, 4, 6, 9].into_iter()
//             .map(|edge_index| {
//                 EdgePtr::new_value(Edge {
//                     edge_index: edge_index,
//                     weight: Rational::zero(),
//                     dual_nodes: vec![],
//                     vertices: vec![],
//                     last_updated_time: Rational::zero(),
//                     growth_at_last_updated_time: Rational::zero(),
//                     grow_rate: Rational::zero(),
//                     unit_index: None,
//                     connected_to_boundary_vertex: false,
//                     #[cfg(feature = "incr_lp")]
//                     cluster_weights: hashbrown::HashMap::new(),
//                 })
//             }).collect();

//         let another_edge = EdgePtr::new_value(Edge {
//             edge_index: 2,
//             weight: Rational::zero(),
//             dual_nodes: vec![],
//             vertices: vec![],
//             last_updated_time: Rational::zero(),
//             growth_at_last_updated_time: Rational::zero(),
//             grow_rate: Rational::zero(),
//             unit_index: None,
//             connected_to_boundary_vertex: false,
//             #[cfg(feature = "incr_lp")]
//             cluster_weights: hashbrown::HashMap::new(),
//         });
//         matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
//         matrix.update_edge_tightness(another_edge.downgrade(), true);
//     }

//     #[test]
//     #[should_panic]
//     fn tight_matrix_cannot_read_nonexistent_edge() {
//         // cargo test tight_matrix_cannot_read_nonexistent_edge -- --nocapture
//         let mut matrix = TightMatrix::new();

//         // create vertices 
//         let vertices: Vec<VertexPtr> = (0..3)
//         .map(|vertex_index| {
//             VertexPtr::new_value(Vertex {
//                 vertex_index,
//                 is_defect: false,
//                 edges: vec![],
//                 mirrored_vertices: vec![],
//             })
//         })
//         .collect();

//         // create edges
//         let edges: Vec<EdgePtr> = vec![1, 4, 6, 9].into_iter()
//             .map(|edge_index| {
//                 EdgePtr::new_value(Edge {
//                     edge_index: edge_index,
//                     weight: Rational::zero(),
//                     dual_nodes: vec![],
//                     vertices: vec![],
//                     last_updated_time: Rational::zero(),
//                     growth_at_last_updated_time: Rational::zero(),
//                     grow_rate: Rational::zero(),
//                     unit_index: None,
//                     connected_to_boundary_vertex: false,
//                     #[cfg(feature = "incr_lp")]
//                     cluster_weights: hashbrown::HashMap::new(),
//                 })
//             }).collect();

//         let another_edge = EdgePtr::new_value(Edge {
//             edge_index: 2,
//             weight: Rational::zero(),
//             dual_nodes: vec![],
//             vertices: vec![],
//             last_updated_time: Rational::zero(),
//             growth_at_last_updated_time: Rational::zero(),
//             grow_rate: Rational::zero(),
//             unit_index: None,
//             connected_to_boundary_vertex: false,
//             #[cfg(feature = "incr_lp")]
//             cluster_weights: hashbrown::HashMap::new(),
//         });
//         matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
//         matrix.is_tight(another_edge.downgrade());
//     }

//     #[test]
//     fn tight_matrix_basic_trait() {
//         // cargo test --features=colorful tight_matrix_basic_trait -- --nocapture
//         let mut matrix = TightMatrix::new();

//         // create vertices 
//         let vertices: Vec<VertexPtr> = (0..3)
//         .map(|vertex_index| {
//             VertexPtr::new_value(Vertex {
//                 vertex_index,
//                 is_defect: false,
//                 edges: vec![],
//                 mirrored_vertices: vec![],
//             })
//         })
//         .collect();

//         // create edges
//         let edges: Vec<EdgePtr> = vec![1, 4, 6, 9].into_iter()
//             .map(|edge_index| {
//                 EdgePtr::new_value(Edge {
//                     edge_index: edge_index,
//                     weight: Rational::zero(),
//                     dual_nodes: vec![],
//                     vertices: vec![],
//                     last_updated_time: Rational::zero(),
//                     growth_at_last_updated_time: Rational::zero(),
//                     grow_rate: Rational::zero(),
//                     unit_index: None,
//                     connected_to_boundary_vertex: false,
//                     #[cfg(feature = "incr_lp")]
//                     cluster_weights: hashbrown::HashMap::new(),
//                 })
//             }).collect();

//         let another_edge = EdgePtr::new_value(Edge {
//             edge_index: 3,
//             weight: Rational::zero(),
//             dual_nodes: vec![],
//             vertices: vec![],
//             last_updated_time: Rational::zero(),
//             growth_at_last_updated_time: Rational::zero(),
//             grow_rate: Rational::zero(),
//             unit_index: None,
//             connected_to_boundary_vertex: false,
//             #[cfg(feature = "incr_lp")]
//             cluster_weights: hashbrown::HashMap::new(),
//         });

//         matrix.add_variable(another_edge.downgrade()); // untight edges will not show
//         matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
//         matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[3].downgrade()], false);
//         matrix.add_constraint(vertices[2].downgrade(), &[edges[0].downgrade(), edges[3].downgrade()], true);
//         matrix.swap_row(2, 1);
//         matrix.xor_row(0, 1);
//         for edge_index in edges.iter() {
//             matrix.update_edge_tightness(edge_index.downgrade(), true);
//         }
//         matrix.printstd();
//         assert_eq!(
//             matrix.clone().printstd_str(),
//             "\
// ┌─┬─┬─┬─┬─┬───┐
// ┊ ┊1┊4┊6┊9┊ = ┊
// ╞═╪═╪═╪═╪═╪═══╡
// ┊0┊ ┊1┊1┊1┊   ┊
// ├─┼─┼─┼─┼─┼───┤
// ┊1┊1┊ ┊ ┊1┊ 1 ┊
// ├─┼─┼─┼─┼─┼───┤
// ┊2┊ ┊1┊ ┊1┊   ┊
// └─┴─┴─┴─┴─┴───┘
// "
//         );
//     }

//     #[test]
//     fn tight_matrix_rebuild_var_indices() {
//         // cargo test --features=colorful tight_matrix_rebuild_var_indices -- --nocapture
//         let mut matrix = TightMatrix::new();

//         // create vertices 
//         let vertices: Vec<VertexPtr> = (0..3)
//         .map(|vertex_index| {
//             VertexPtr::new_value(Vertex {
//                 vertex_index,
//                 is_defect: false,
//                 edges: vec![],
//                 mirrored_vertices: vec![],
//             })
//         })
//         .collect();

//         // create edges
//         let edges: Vec<EdgePtr> = vec![1, 4, 6, 9].into_iter()
//             .map(|edge_index| {
//                 EdgePtr::new_value(Edge {
//                     edge_index: edge_index,
//                     weight: Rational::zero(),
//                     dual_nodes: vec![],
//                     vertices: vec![],
//                     last_updated_time: Rational::zero(),
//                     growth_at_last_updated_time: Rational::zero(),
//                     grow_rate: Rational::zero(),
//                     unit_index: None,
//                     connected_to_boundary_vertex: false,
//                     #[cfg(feature = "incr_lp")]
//                     cluster_weights: hashbrown::HashMap::new(),
//                 })
//             }).collect();

//         let another_edge = EdgePtr::new_value(Edge {
//             edge_index: 3,
//             weight: Rational::zero(),
//             dual_nodes: vec![],
//             vertices: vec![],
//             last_updated_time: Rational::zero(),
//             growth_at_last_updated_time: Rational::zero(),
//             grow_rate: Rational::zero(),
//             unit_index: None,
//             connected_to_boundary_vertex: false,
//             #[cfg(feature = "incr_lp")]
//             cluster_weights: hashbrown::HashMap::new(),
//         });

//         matrix.add_variable(another_edge.downgrade()); // untight edges will not show
//         matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
//         assert_eq!(matrix.columns(), 0);
//         for edge_index in [0, 1, 2] {
//             matrix.update_edge_tightness(edges[edge_index].downgrade(), true);
//         }
//         assert_eq!(matrix.columns(), 3);
//         assert_eq!(matrix.columns(), 3); // should only update var_indices_once
//         matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[3].downgrade()], false);
//         matrix.add_constraint(vertices[2].downgrade(), &[edges[0].downgrade(), edges[3].downgrade()], true);
//         matrix.update_edge_tightness(edges[3].downgrade(), true);
//         matrix.update_edge_tightness(edges[1].downgrade(), false);
//         matrix.update_edge_tightness(edges[2].downgrade(), false);
//         assert_eq!(matrix.columns(), 2);
//         matrix.printstd();
//         assert_eq!(
//             matrix.clone().printstd_str(),
//             "\
// ┌─┬─┬─┬───┐
// ┊ ┊1┊9┊ = ┊
// ╞═╪═╪═╪═══╡
// ┊0┊1┊ ┊ 1 ┊
// ├─┼─┼─┼───┤
// ┊1┊ ┊1┊   ┊
// ├─┼─┼─┼───┤
// ┊2┊1┊1┊ 1 ┊
// └─┴─┴─┴───┘
// "
//         );
//     }

//     #[test]
//     #[should_panic]
//     fn tight_matrix_cannot_call_dirty_column() {
//         // cargo test tight_matrix_cannot_call_dirty_column -- --nocapture
//         let mut matrix = TightMatrix::new();

//         // create vertices 
//         let vertices: Vec<VertexPtr> = (0..3)
//         .map(|vertex_index| {
//             VertexPtr::new_value(Vertex {
//                 vertex_index,
//                 is_defect: false,
//                 edges: vec![],
//                 mirrored_vertices: vec![],
//             })
//         })
//         .collect();

//         let global_time = ArcRwLock::new_value(Rational::zero());

//         // create edges
//         let edges: Vec<EdgePtr> = vec![1, 4, 6, 9].into_iter()
//             .map(|edge_index| {
//                 EdgePtr::new_value(Edge {
//                     edge_index: edge_index,
//                     weight: Rational::zero(),
//                     dual_nodes: vec![],
//                     vertices: vec![],
//                     last_updated_time: Rational::zero(),
//                     growth_at_last_updated_time: Rational::zero(),
//                     grow_rate: Rational::zero(),
//                     unit_index: None,
//                     connected_to_boundary_vertex: false,
//                     #[cfg(feature = "incr_lp")]
//                     cluster_weights: hashbrown::HashMap::new(),
//                 })
//             }).collect();

//         // let another_edge = EdgePtr::new_value(Edge {
//         //     edge_index: 3,
//         //     weight: Rational::zero(),
//         //     dual_nodes: vec![],
//         //     vertices: vec![],
//         //     last_updated_time: Rational::zero(),
//         //     growth_at_last_updated_time: Rational::zero(),
//         //     grow_rate: Rational::zero(),
//         //     unit_index: None,
//         //     connected_to_boundary_vertex: false,
//         //     global_time: global_time.clone(),
//         //     #[cfg(feature = "incr_lp")]
//         //     cluster_weights: hashbrown::HashMap::new(),
//         // });
//         matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
//         matrix.update_edge_tightness(edges[0].downgrade(), true);
//         // even though there is indeed such a column, we forbid such dangerous calls
//         // always call `columns()` before accessing any column
//         matrix.column_to_var_index(0);
//     }
// }
