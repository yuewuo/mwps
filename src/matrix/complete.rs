use super::interface::*;
use super::row::*;
use super::visualize::*;
use crate::util::*;
use derivative::Derivative;
use weak_table::PtrWeakHashSet;
use weak_table::PtrWeakKeyHashMap;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak};

/// complete matrix considers a predefined set of edges and won't consider any other edges
#[derive(Clone, Derivative)]
#[derivative(Default(new = "true"))]
pub struct CompleteMatrix {
    /// the vertices already maintained by this parity check
    vertices: PtrWeakHashSet<VertexWeak>,
    /// the edges maintained by this parity check, mapping to the local indices
    edges: PtrWeakKeyHashMap<EdgeWeak, VarIndex>,
    /// variable index map to edge index
    variables: Vec<EdgeWeak>,
    constraints: Vec<ParityRow>,
}

impl MatrixBasic for CompleteMatrix {
    fn add_variable(&mut self, edge_weak: EdgeWeak) -> Option<VarIndex> {
        if self.edges.contains_key(&edge_weak.upgrade_force()) {
            // variable already exists
            return None;
        }
        if !self.constraints.is_empty() {
            panic!("complete matrix doesn't allow dynamic edges, please insert all edges at the beginning")
        }
        let var_index = self.variables.len();
        self.edges.insert(edge_weak.upgrade_force(), var_index);
        self.variables.push(edge_weak);
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
        self.vertices.insert(vertex_weak.upgrade_force());
        let mut row = ParityRow::new_length(self.variables.len());
        for edge_index in incident_edges.iter() {
            if self.exists_edge(edge_index.clone()) {
                let var_index = self.edges[&edge_index.upgrade_force()];
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

    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeWeak {
        self.variables[var_index].clone()
    }

    fn edge_to_var_index(&self, edge_weak: EdgeWeak) -> Option<VarIndex> {
        self.edges.get(&edge_weak.upgrade_force()).cloned()
    }

    fn get_vertices(&self) -> PtrWeakHashSet<VertexWeak> {
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
pub mod tests {
    use crate::matrix::Echelon;
    use crate::dual_module_pq::{EdgePtr, Edge, VertexPtr, Vertex};
    use crate::pointers::*;
    use super::*;
    use num_traits::Zero;

    #[test]
    fn complete_matrix_1() {
        // cargo test --features=colorful complete_matrix_1 -- --nocapture
        let mut matrix = CompleteMatrix::new();


        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                })
            })
            .collect();

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
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();


        for edge_ptr in edges.iter() {
            matrix.add_variable(edge_ptr.downgrade());
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

        use std::collections::HashSet;
        let matrix_vertices: HashSet<_> = matrix.get_vertices().into_iter().map(|v| v.upgradable_read().vertex_index).collect();
        assert_eq!(matrix_vertices, [0, 1, 2].into());
        assert_eq!(matrix.get_view_edges().into_iter().map(|e| e.upgrade_force().read_recursive().edge_index).collect::<Vec<usize>>(), [1, 4, 12, 345]);
    }

    #[test]
    fn complete_matrix_should_not_add_repeated_constraint() {
        // cargo test --features=colorful complete_matrix_should_not_add_repeated_constraint -- --nocapture
        let mut matrix = CompleteMatrix::new();


        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                })
            })
            .collect();

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
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();


        for edge_ptr in edges.iter() {
            matrix.add_variable(edge_ptr.downgrade());
        }
        assert_eq!(matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], false), None);
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
    fn complete_matrix_row_operations() {
        // cargo test --features=colorful complete_matrix_row_operations -- --nocapture
        let mut matrix = CompleteMatrix::new();


        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                })
            })
            .collect();

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
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();



        for edge_ptr in edges.iter() {
            matrix.add_variable(edge_ptr.downgrade());
        }
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
    fn complete_matrix_manual_echelon() {
        // cargo test --features=colorful complete_matrix_manual_echelon -- --nocapture
        let mut matrix = CompleteMatrix::new();


        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                })
            })
            .collect();

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
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();


        for edge_ptr in edges.iter() {
            matrix.add_variable(edge_ptr.downgrade());
        }

        for &edge_index in [3, 2, 1, 0].iter() {
            matrix.add_variable(edges[edge_index].downgrade());
        }

        matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
        matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[3].downgrade()], false);
        matrix.add_constraint(vertices[2].downgrade(), &[edges[0].downgrade(), edges[3].downgrade()], true);
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


        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                })
            })
            .collect();

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
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();
        
        let edges_more: Vec<EdgePtr> = vec![11, 12, 23].into_iter()
            .map(|edge_index| {
                EdgePtr::new_value(Edge {
                    edge_index: edge_index,
                    weight: Rational::zero(),
                    dual_nodes: vec![],
                    vertices: vec![],
                    last_updated_time: Rational::zero(),
                    growth_at_last_updated_time: Rational::zero(),
                    grow_rate: Rational::zero(),
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();


        for edge_ptr in edges.iter() {
            matrix.add_variable(edge_ptr.downgrade());
        }
        matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade(), edges_more[0].downgrade(), edges_more[1].downgrade()], true);
        matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[3].downgrade(), edges_more[2].downgrade(), edges_more[1].downgrade()], false);
        matrix.add_constraint(vertices[2].downgrade(), &[edges[0].downgrade(), edges[3].downgrade(), edges_more[0].downgrade()], true);
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

        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                })
            })
            .collect();

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
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();

        for edge_ptr in edges.iter() {
            matrix.add_variable(edge_ptr.downgrade());
        }
        matrix.add_constraint(vertices[0].downgrade(), &[edges[0].downgrade(), edges[1].downgrade(), edges[2].downgrade()], true);
        matrix.add_constraint(vertices[1].downgrade(), &[edges[1].downgrade(), edges[3].downgrade()], false);
        matrix.add_constraint(vertices[2].downgrade(), &[edges[0].downgrade(), edges[3].downgrade()], true);

        let another_edge =  EdgePtr::new_value(Edge {
            edge_index: 2,
            weight: Rational::zero(),
            dual_nodes: vec![],
            vertices: vec![],
            last_updated_time: Rational::zero(),
            growth_at_last_updated_time: Rational::zero(),
            grow_rate: Rational::zero(),
            #[cfg(feature = "incr_lp")]
            cluster_weights: hashbrown::HashMap::new(),
        });


        matrix.add_variable(another_edge.downgrade());
    }
}
