//! Serial Primal Module like Union-Find decoder
//! 
//! This implementation is an approximate MWPS solver, which is essentially what hypergraph UF decoder does.
//! Delfosse, Nicolas, Vivien Londe, and Michael E. Beverland. "Toward a Union-Find decoder for quantum LDPC codes." IEEE Transactions on Information Theory 68.5 (2022): 3187-3199.
//!
//! there might be some minor difference with Delfosse's paper, but the idea is the same
//! 

use crate::util::*;
use crate::derivative::Derivative;
use crate::primal_module::*;
use crate::visualize::*;
use crate::dual_module::*;
use crate::pointers::*;
use crate::serde::{Serialize, Deserialize};
use crate::union_find::*;
use std::collections::BTreeSet;
use crate::num_traits::Zero;


#[derive(Derivative)]
#[derivative(Debug)]
pub struct PrimalModuleUnionFind {
    /// union find data structure
    union_find: UnionFind,
}

type UnionFind = UnionFindGeneric<PrimalModuleUnionFindNode>;

/// define your own union-find node data structure like this
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PrimalModuleUnionFindNode {
    /// all the internal edges
    pub internal_edges: BTreeSet<EdgeIndex>,
    /// the corresponding node index with these internal edges
    pub node_index: NodeIndex,
}

/// example trait implementation
impl UnionNodeTrait for PrimalModuleUnionFindNode {
    #[inline]
    fn union(left: &Self, right: &Self) -> (bool, Self) {
        let mut internal_edges = BTreeSet::new();
        internal_edges.extend(left.internal_edges.iter().cloned());
        internal_edges.extend(right.internal_edges.iter().cloned());
        let result = Self {
            internal_edges: internal_edges,
            node_index: NodeIndex::MAX,  // waiting for assignment
        };
        // if left size is larger, choose left (weighted union)
        (true, result)
    }
    #[inline]
    fn clear(&mut self) {
        panic!("clear a node is meaningless here, call `remove_all` instead");
    }
    #[inline]
    fn default() -> Self {
        Self {
            internal_edges: BTreeSet::new(),
            node_index: NodeIndex::MAX,  // waiting for assignment
        }
    }
}

impl PrimalModuleImpl for PrimalModuleUnionFind {

    fn new_empty(_initializer: &SolverInitializer) -> Self {
        Self {
            union_find: UnionFind::new(0),
        }
    }

    fn clear(&mut self) {
        self.union_find.remove_all();
    }

    fn load_defect_dual_node(&mut self, dual_node_ptr: &DualNodePtr) {
        let node = dual_node_ptr.read_recursive();
        assert_eq!(node.index, self.union_find.size(), "must load defect nodes in order");
        self.union_find.insert(PrimalModuleUnionFindNode {
            internal_edges: BTreeSet::new(),
            node_index: node.index,
        });
    }

    fn resolve<D: DualModuleImpl>(&mut self, mut group_max_update_length: GroupMaxUpdateLength, interface: &DualModuleInterfacePtr, dual_module: &mut D) {
        debug_assert!(!group_max_update_length.is_unbounded() && group_max_update_length.get_valid_growth().is_none());
        let mut active_clusters = BTreeSet::<NodeIndex>::new();
        while let Some(conflict) = group_max_update_length.pop() {
            // println!("conflict: {conflict:?}");
            match conflict {
                MaxUpdateLength::Conflicting(edge_index) => {
                    // union all the dual nodes in the edge index and create new dual node by adding this edge to `internal_edges`
                    let dual_nodes = dual_module.get_edge_nodes(edge_index);
                    debug_assert!(dual_nodes.len() > 0, "should not conflict if no dual nodes are contributing");
                    let cluster_index = dual_nodes[0].read_recursive().index;
                    for node_ptr in dual_nodes.iter() {
                        let mut node = node_ptr.write();
                        active_clusters.remove(&self.union_find.find(node.index));
                        node.grow_rate = Rational::zero();
                        self.union_find.union(cluster_index, node.index);
                    }
                    self.union_find.get_mut(cluster_index).internal_edges.insert(edge_index);
                    active_clusters.insert(self.union_find.find(cluster_index));
                }, _ => { unreachable!() }
            }
        }
        for &cluster_index in active_clusters.iter() {
            if dual_module.is_valid_cluster(&self.union_find.get(cluster_index).internal_edges) {
                // do nothing
            } else {
                let new_cluster_node_index = self.union_find.size();
                self.union_find.insert(PrimalModuleUnionFindNode {
                    internal_edges: BTreeSet::new(),
                    node_index: new_cluster_node_index,
                });
                self.union_find.union(cluster_index, new_cluster_node_index);
                interface.create_cluster_node(self.union_find.get(cluster_index).internal_edges.clone(), dual_module);
            }
        }
    }

    fn subgraph<D: DualModuleImpl>(&mut self, _interface: &DualModuleInterfacePtr, dual_module: &mut D) -> Subgraph {
        let mut valid_clusters = BTreeSet::new();
        let mut subgraph = Subgraph::new_empty();
        for i in 0..self.union_find.size() {
            let root_index = self.union_find.find(i);
            if !valid_clusters.contains(&root_index) {
                valid_clusters.insert(root_index);
                let cluster_subgraph = dual_module.find_valid_subgraph(&self.union_find.get(root_index).internal_edges).expect("must be valid cluster");
                subgraph.extend(cluster_subgraph.iter());
            }
        }
        subgraph
    }

}

/*
Implementing visualization functions
*/

impl MWPSVisualizer for PrimalModuleUnionFind {
    fn snapshot(&self, _abbrev: bool) -> serde_json::Value {
        json!({

        })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use super::super::example_codes::*;
    use super::super::dual_module_serial::*;
    use crate::more_asserts::*;
    use crate::num_traits::{FromPrimitive, ToPrimitive};

    pub fn primal_module_union_find_basic_standard_syndrome_optional_viz(mut code: impl ExampleCode, visualize_filename: Option<String>, defect_vertices: Vec<VertexIndex>, final_dual: Weight)
            -> (DualModuleInterfacePtr, PrimalModuleUnionFind, DualModuleSerial) {
        println!("{defect_vertices:?}");
        let mut visualizer = match visualize_filename.as_ref() {
            Some(visualize_filename) => {
                let visualizer = Visualizer::new(Some(visualize_data_folder() + visualize_filename.as_str()), code.get_positions(), true).unwrap();
                print_visualize_link(visualize_filename.clone());
                Some(visualizer)
            }, None => None
        };
        // create dual module
        let initializer = code.get_initializer();
        let mut dual_module = DualModuleSerial::new_empty(&initializer);
        // create primal module
        let mut primal_module = PrimalModuleUnionFind::new_empty(&initializer);
        // try to work on a simple syndrome
        code.set_defect_vertices(&defect_vertices);
        let interface_ptr = DualModuleInterfacePtr::new_empty();
        primal_module.solve_visualizer(&interface_ptr, &code.get_syndrome(), &mut dual_module, visualizer.as_mut());
        let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module, &initializer);
        if let Some(visualizer) = visualizer.as_mut() {
            visualizer.snapshot_combined("perfect matching and subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph, &weight_range]).unwrap();
        }
        assert!(initializer.matches_subgraph_syndrome(&subgraph, &defect_vertices), "the result subgraph is invalid");
        assert_le!(Rational::from_usize(final_dual).unwrap(), weight_range.upper, "unmatched sum dual variables");
        assert_ge!(Rational::from_usize(final_dual).unwrap(), weight_range.lower, "unexpected final dual variable sum");
        println!("weight range: [{}, {}]", weight_range.lower.to_i64().unwrap(), weight_range.upper.to_i64().unwrap());
        (interface_ptr, primal_module, dual_module)
    }

    pub fn primal_module_union_find_basic_standard_syndrome(code: impl ExampleCode, visualize_filename: String, defect_vertices: Vec<VertexIndex>, final_dual: Weight)
            -> (DualModuleInterfacePtr, PrimalModuleUnionFind, DualModuleSerial) {
        primal_module_union_find_basic_standard_syndrome_optional_viz(code, Some(visualize_filename), defect_vertices, final_dual)
    }

    /// test a simple case
    #[test]
    fn primal_module_union_find_basic_1() {  // cargo test primal_module_union_find_basic_1 -- --nocapture
        let visualize_filename = format!("primal_module_union_find_basic_1.json");
        let defect_vertices = vec![23, 24, 29, 30];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_union_find_basic_standard_syndrome(code, visualize_filename, defect_vertices, 1);
    }

    #[test]
    fn primal_module_union_find_basic_2() {  // cargo test primal_module_union_find_basic_2 -- --nocapture
        let visualize_filename = format!("primal_module_union_find_basic_2.json");
        let defect_vertices = vec![16, 17, 23, 25, 29, 30];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_union_find_basic_standard_syndrome(code, visualize_filename, defect_vertices, 2);
    }

    #[test]
    fn primal_module_union_find_basic_3() {  // cargo test primal_module_union_find_basic_3 -- --nocapture
        let visualize_filename = format!("primal_module_union_find_basic_3.json");
        let defect_vertices = vec![14, 15, 16, 17, 22, 25, 28, 31, 36, 37, 38, 39];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_union_find_basic_standard_syndrome(code, visualize_filename, defect_vertices, 5);
    }

    #[test]
    fn primal_module_union_find_basic_4() {  // cargo test primal_module_union_find_basic_4 -- --nocapture
        let visualize_filename = format!("primal_module_union_find_basic_4.json");
        let defect_vertices = vec![3, 12];
        let code = CodeCapacityColorCode::new(7, 0.01, 1);
        primal_module_union_find_basic_standard_syndrome(code, visualize_filename, defect_vertices, 2);
    }

    #[test]
    fn primal_module_union_find_basic_5() {  // cargo test primal_module_union_find_basic_5 -- --nocapture
        let visualize_filename = format!("primal_module_union_find_basic_5.json");
        let defect_vertices = vec![3, 5, 10, 12];
        let code = CodeCapacityColorCode::new(7, 0.01, 1);
        primal_module_union_find_basic_standard_syndrome(code, visualize_filename, defect_vertices, 4);
    }

    #[test]
    fn primal_module_union_find_basic_6() {  // cargo test primal_module_union_find_basic_6 -- --nocapture
        let visualize_filename = format!("primal_module_union_find_basic_6.json");
        let defect_vertices = vec![22];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.05, 1);
        primal_module_union_find_basic_standard_syndrome(code, visualize_filename, defect_vertices, 4);
    }

}
