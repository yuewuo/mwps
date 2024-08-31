use super::interface::*;
use super::visualize::*;
use crate::util::*;
use derivative::Derivative;
use weak_table::PtrWeakHashSet;
use std::collections::BTreeSet;

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};

#[derive(Clone, Derivative)]
#[derivative(Default(new = "true"))]
pub struct Tail<M: MatrixView> {
    base: M,
    /// the set of edges that should be placed at the end, if any
    tail_edges: BTreeSet<EdgePtr>,
    /// var indices are outdated on any changes to the underlying matrix
    #[derivative(Default(value = "true"))]
    is_var_indices_outdated: bool,
    /// the internal store of var indices
    var_indices: Vec<VarIndex>,
    /// internal cache for reducing memory allocation
    tail_var_indices: Vec<VarIndex>,
}

impl<M: MatrixView> Tail<M> {
    pub fn get_base(&self) -> &M {
        &self.base
    }
}

impl<M: MatrixView> MatrixTail for Tail<M> {
    fn get_tail_edges(&self) -> &BTreeSet<EdgePtr> {
        &self.tail_edges
    }
    fn get_tail_edges_mut(&mut self) -> &mut BTreeSet<EdgePtr> {
        self.is_var_indices_outdated = true;
        &mut self.tail_edges
    }
}

impl<M: MatrixTight> MatrixTight for Tail<M> {
    fn update_edge_tightness(&mut self, edge_weak: EdgeWeak, is_tight: bool) {
        self.is_var_indices_outdated = true;
        self.base.update_edge_tightness(edge_weak, is_tight)
    }
    fn is_tight(&self, edge_weak: EdgeWeak) -> bool {
        self.base.is_tight(edge_weak)
    }
}

impl<M: MatrixView> MatrixBasic for Tail<M> {
    fn add_variable(&mut self, edge_weak: EdgeWeak) -> Option<VarIndex> {
        self.is_var_indices_outdated = true;
        self.base.add_variable(edge_weak)
    }

    fn add_constraint(
        &mut self,
        vertex_weak: VertexWeak,
        incident_edges: &[EdgeWeak],
        parity: bool,
    ) -> Option<Vec<VarIndex>> {
        self.base.add_constraint(vertex_weak, incident_edges, parity)
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
    fn get_vertices(&self) -> BTreeSet<VertexPtr> {
        self.get_base().get_vertices()
    }
}

impl<M: MatrixView> Tail<M> {
    fn force_update_var_indices(&mut self) {
        self.var_indices.clear();
        self.tail_var_indices.clear();
        for column in 0..self.base.columns() {
            let var_index = self.base.column_to_var_index(column);
            let edge_weak = self.base.var_to_edge_index(var_index);
            if self.tail_edges.contains(&edge_weak.upgrade_force()) {
                self.tail_var_indices.push(var_index);
            } else {
                self.var_indices.push(var_index);
            }
        }
        self.var_indices.append(&mut self.tail_var_indices)
    }

    fn var_indices_lazy_update(&mut self) {
        if self.is_var_indices_outdated {
            self.force_update_var_indices();
            self.is_var_indices_outdated = false;
        }
    }
}

impl<M: MatrixView> MatrixView for Tail<M> {
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

impl<M: MatrixView> VizTrait for Tail<M> {
    fn viz_table(&mut self) -> VizTable {
        VizTable::from(self)
    }
}

#[cfg(test)]
pub mod tests {
    use super::super::basic::*;
    use super::super::tight::*;
    use super::*;

    use crate::dual_module_pq::{EdgePtr, Edge, VertexPtr, Vertex};
    use crate::pointers::*;
    use num_traits::Zero;

    type TailMatrix = Tail<Tight<BasicMatrix>>;

    #[test]
    fn tail_matrix_1() {
        // cargo test --features=colorful tail_matrix_1 -- --nocapture
        let mut matrix = TailMatrix::new();

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
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();


        matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
        matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[3].downgrade()], false);
        matrix.add_constraint(vertices[2].downgrade(), &[edges[0].downgrade(), edges[3].downgrade()], true);
        assert_eq!(matrix.edge_to_var_index(edges[1].downgrade()), Some(1));
        matrix.printstd();
        assert_eq!(
            matrix.clone().printstd_str(),
            "\
┌─┬───┐
┊ ┊ = ┊
╞═╪═══╡
┊0┊ 1 ┊
├─┼───┤
┊1┊   ┊
├─┼───┤
┊2┊ 1 ┊
└─┴───┘
"
        );
        for edge_index in edges.iter() {
            matrix.update_edge_tightness(edge_index.downgrade(), true);
        }
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
        matrix.set_tail_edges([edges[0].downgrade(), edges[2].downgrade()].into_iter());
        matrix.printstd();
        assert_eq!(
            matrix.clone().printstd_str(),
            "\
┌─┬─┬─┬─┬─┬───┐
┊ ┊4┊9┊1┊6┊ = ┊
╞═╪═╪═╪═╪═╪═══╡
┊0┊1┊ ┊1┊1┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊1┊1┊1┊ ┊ ┊   ┊
├─┼─┼─┼─┼─┼───┤
┊2┊ ┊1┊1┊ ┊ 1 ┊
└─┴─┴─┴─┴─┴───┘
"
        );
        assert_eq!(matrix.get_tail_edges_vec().iter().map(|e| e.upgrade_force().read_recursive().edge_index).collect::<Vec<_>>(), [1, 6]);
        assert_eq!(matrix.edge_to_var_index(edges[1].downgrade()), Some(1));
    }

//     #[test]
//     #[should_panic]
//     fn tail_matrix_cannot_call_dirty_column() {
//         // cargo test tail_matrix_cannot_call_dirty_column -- --nocapture
//         let mut matrix = TailMatrix::new();
//         matrix.add_constraint(0, &[1, 4, 6], true);
//         matrix.update_edge_tightness(1, true);
//         // even though there is indeed such a column, we forbid such dangerous calls
//         // always call `columns()` before accessing any column
//         matrix.column_to_var_index(0);
//     }

//     #[test]
//     fn tail_matrix_basic_trait() {
//         // cargo test --features=colorful tail_matrix_basic_trait -- --nocapture
//         let mut matrix = TailMatrix::new();
//         matrix.add_variable(3); // untight edges will not show
//         matrix.add_constraint(0, &[1, 4, 6], true);
//         matrix.add_constraint(1, &[4, 9], false);
//         matrix.add_constraint(2, &[1, 9], true);
//         matrix.swap_row(2, 1);
//         matrix.xor_row(0, 1);
//         for edge_index in [1, 4, 6, 9] {
//             matrix.update_edge_tightness(edge_index, true);
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
//         assert!(matrix.is_tight(1));
//         assert_eq!(matrix.edge_to_var_index(4), Some(2));
//     }
}
