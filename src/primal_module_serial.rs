//! Serial Primal Module
//! 
//! This implementation targets to be an exact MWPS solver, although it's not yet sure whether it is actually one.
//! 

use crate::parity_matrix::*;
use crate::util::*;
use crate::primal_module::*;
use crate::visualize::*;
use crate::dual_module::*;
use crate::pointers::*;
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Debug;
use crate::num_traits::{Zero, One};
use crate::parking_lot::Mutex;
use std::sync::Arc;


pub struct PrimalModuleSerial {
    /// the original hypergraph, used to run local minimization
    pub initializer: Arc<SolverInitializer>,
    /// growing strategy, default to single-tree approach for easier debugging and better locality
    pub growing_strategy: GrowingStrategy,
    /// dual nodes information
    pub nodes: Vec<PrimalModuleSerialNodePtr>, 
    /// clusters of dual nodes
    pub clusters: Vec<PrimalClusterPtr>,
    /// pending dual variables to grow,
    pending_nodes: VecDeque<PrimalModuleSerialNodeWeak>,
    /// debug mode: record all decisions
    enable_debug: bool,
    /// debug recordings, every visualize call will clear the existing recordings
    debug_recordings: Mutex<Vec<DebugEntry>>,
    /// the optimization level
    pub optimization_level: OptimizationLevel,
}

/// strategy of growing the dual variables
#[derive(Debug, Clone, Copy)]
pub enum GrowingStrategy {
    /// focus on a single cluster at a time, for easier debugging and better locality
    SingleCluster,
    /// all clusters grow at the same time
    MultipleClusters,
}

pub type OptimizationLevel = usize;
/// the basic optimization level: just find a valid cluster and stop there
pub const OPT_LEVEL_UNION_FIND: OptimizationLevel = 0;
/// every dual variable has a single-hair solution independently
pub const OPT_LEVEL_INDEPENDENT_SINGLE_HAIR: OptimizationLevel = 1;
/// every dual variable have single-hair solution after other dual variables eliminates their non-single-hair edges
pub const OPT_LEVEL_JOINT_SINGLE_HAIR: OptimizationLevel = 2;
/// searching for rows to find multi-row coordination to further eliminate edges
pub const OPT_LEVEL_MULTI_ROW_COORDINATION: OptimizationLevel = 3;

pub struct PrimalModuleSerialNode {
    /// the dual node
    pub dual_node_ptr: DualNodePtr,
    /// the cluster that it belongs to
    pub cluster_weak: PrimalClusterWeak,
}

pub type PrimalModuleSerialNodePtr = ArcRwLock<PrimalModuleSerialNode>;
pub type PrimalModuleSerialNodeWeak = WeakRwLock<PrimalModuleSerialNode>;

pub struct PrimalCluster {
    /// the index in the cluster
    pub cluster_index: NodeIndex,
    /// the nodes that belongs to this cluster
    pub nodes: Vec<PrimalModuleSerialNodePtr>,
    /// all the edges ever exists in any hair
    pub edges: BTreeSet<EdgeIndex>,
    /// all the vertices ever touched by any tight edge
    pub vertices: BTreeSet<VertexIndex>,
    /// the parity matrix to determine whether it's a valid cluster and also find new ways to increase the dual
    pub matrix: ParityMatrix,
    /// the parity subgraph result, only valid when it's solved
    pub subgraph: Option<Subgraph>,
}

pub type PrimalClusterPtr = ArcRwLock<PrimalCluster>;
pub type PrimalClusterWeak = WeakRwLock<PrimalCluster>;

pub enum DebugEntry {
    /// print a matrix
    EchelonFormMatrix {
        /// the parity matrix in the echelon form
        matrix: ParityMatrix,
        /// hair edges that are reordered to the end of the columns, empty if don't care
        hair_edges: Vec<EdgeIndex>,
    },
    /// display a message
    Message(String),
    /// display an error
    Error(String),
}

impl PrimalModuleImpl for PrimalModuleSerial {

    fn new_empty(initializer: &SolverInitializer) -> Self {
        Self {
            initializer: Arc::new(initializer.clone()),
            growing_strategy: GrowingStrategy::SingleCluster,
            // growing_strategy: GrowingStrategy::MultipleClusters,
            nodes: vec![],
            clusters: vec![],
            pending_nodes: VecDeque::new(),
            enable_debug: true,
            debug_recordings: Mutex::new(vec![]),
            optimization_level: OPT_LEVEL_INDEPENDENT_SINGLE_HAIR,  // default to the highest optimization level currently supported
        }
    }

    fn clear(&mut self) {
        self.nodes.clear();
        self.clusters.clear();
    }

    fn load_defect_dual_node<D: DualModuleImpl>(&mut self, dual_node_ptr: &DualNodePtr, dual_module: &mut D) {
        let node = dual_node_ptr.read_recursive();
        assert_eq!(node.index, self.nodes.len(), "must load defect nodes in order");
        let mut matrix = ParityMatrix::new();
        let mut edges = BTreeSet::new();
        let mut vertices = BTreeSet::new();
        for &edge_index in node.hair_edges.iter() {
            matrix.add_variable(edge_index);
            edges.insert(edge_index);
        }
        for &vertex_index in node.internal_vertices.iter() {
            vertices.insert(vertex_index);
        }
        for &vertex_index in vertices.iter() {
            matrix.add_parity_check_with_dual_module(vertex_index, dual_module);
        }
        let primal_cluster_ptr = PrimalClusterPtr::new_value(PrimalCluster {
            cluster_index: self.clusters.len(),
            nodes: vec![],
            edges: BTreeSet::new(),
            vertices: BTreeSet::new(),
            matrix: matrix,
            subgraph: None,
        });
        let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
            dual_node_ptr: dual_node_ptr.clone(),
            cluster_weak: primal_cluster_ptr.downgrade(),
        });
        primal_cluster_ptr.write().nodes.push(primal_node_ptr.clone());
        self.nodes.push(primal_node_ptr);
        self.clusters.push(primal_cluster_ptr);
    }

    fn begin_resolving<D: DualModuleImpl>(&mut self, _interface_ptr: &DualModuleInterfacePtr, dual_module: &mut D) {
        if matches!(self.growing_strategy, GrowingStrategy::SingleCluster) {
            for primal_node_ptr in self.nodes.iter().skip(1) {
                let dual_node_ptr = primal_node_ptr.read_recursive().dual_node_ptr.clone();
                dual_module.set_grow_rate(&dual_node_ptr, Rational::zero());
                self.pending_nodes.push_back(primal_node_ptr.downgrade());
            }
        }
    }

    fn resolve(&mut self, mut group_max_update_length: GroupMaxUpdateLength, interface: &DualModuleInterfacePtr, dual_module: &mut impl DualModuleImpl) {
        debug_assert!(!group_max_update_length.is_unbounded() && group_max_update_length.get_valid_growth().is_none());
        let mut active_clusters = BTreeSet::<NodeIndex>::new();
        while let Some(conflict) = group_max_update_length.pop() {
            match conflict {
                MaxUpdateLength::Conflicting(edge_index) => {
                    // union all the dual nodes in the edge index and create new dual node by adding this edge to `internal_edges`
                    let dual_nodes = dual_module.get_edge_nodes(edge_index);
                    debug_assert!(dual_nodes.len() > 0, "should not conflict if no dual nodes are contributing");
                    let dual_node_ptr_0 = &dual_nodes[0];
                    // first union all the dual nodes
                    for dual_node_ptr in dual_nodes.iter() {
                        self.union(dual_node_ptr_0, dual_node_ptr, dual_module);
                    }
                    let cluster_ptr = self.nodes[dual_node_ptr_0.read_recursive().index].read_recursive().cluster_weak.upgrade_force();
                    let mut cluster = cluster_ptr.write();
                    // then add new constraints because these edges may touch new vertices
                    let incident_vertices = dual_module.get_edge_neighbors(edge_index);
                    for &vertex_index in incident_vertices.iter() {
                        if !cluster.vertices.contains(&vertex_index) {
                            cluster.vertices.insert(vertex_index);
                            cluster.matrix.add_parity_check_with_dual_module(vertex_index, dual_module);
                        }
                    }
                    // add to active cluster so that it's processed later
                    active_clusters.insert(cluster.cluster_index);
                },
                MaxUpdateLength::ShrinkProhibited(dual_node_ptr) => {
                    let cluster_ptr = self.nodes[dual_node_ptr.read_recursive().index].read_recursive().cluster_weak.upgrade_force();
                    let cluster_index = cluster_ptr.read_recursive().cluster_index;
                    active_clusters.insert(cluster_index);
                },
                _ => { unreachable!() }
            }
        }
        let mut all_solved = true;
        for &cluster_index in active_clusters.iter() {
            let solved = self.resolve_cluster(cluster_index, interface, dual_module);
            all_solved &= solved;
        }
        if all_solved {
            while !self.pending_nodes.is_empty() {
                let primal_node_weak = self.pending_nodes.pop_front().unwrap();
                let primal_node_ptr = primal_node_weak.upgrade_force();
                let primal_node = primal_node_ptr.read_recursive();
                let cluster_ptr = primal_node.cluster_weak.upgrade_force();
                if cluster_ptr.read_recursive().subgraph.is_none() {
                    dual_module.set_grow_rate(&primal_node.dual_node_ptr, Rational::one());
                    break
                }
            }
        }
    }

    fn subgraph(&mut self, _interface: &DualModuleInterfacePtr, _dual_module: &mut impl DualModuleImpl) -> Subgraph {
        let mut subgraph = Subgraph::new_empty();
        for cluster_ptr in self.clusters.iter() {
            let cluster = cluster_ptr.read_recursive();
            if cluster.nodes.is_empty() {
                continue
            }
            subgraph.extend(cluster.subgraph.clone().expect("bug occurs: cluster should be solved").iter());
        }
        subgraph
    }

}

impl PrimalModuleSerial {

    // union the cluster of two dual nodes
    pub fn union<D: DualModuleImpl>(&self, dual_node_ptr_1: &DualNodePtr, dual_node_ptr_2: &DualNodePtr, dual_module: &mut D) {
        let node_index_1 = dual_node_ptr_1.read_recursive().index;
        let node_index_2 = dual_node_ptr_2.read_recursive().index;
        let primal_node_1 = self.nodes[node_index_1].read_recursive();
        let primal_node_2 = self.nodes[node_index_2].read_recursive();
        if primal_node_1.cluster_weak.ptr_eq(&primal_node_2.cluster_weak) {
            return  // already in the same cluster
        }
        let cluster_ptr_1 = primal_node_1.cluster_weak.upgrade_force();
        let cluster_ptr_2 = primal_node_2.cluster_weak.upgrade_force();
        drop(primal_node_1);
        drop(primal_node_2);
        let mut cluster_1 = cluster_ptr_1.write();
        let mut cluster_2 = cluster_ptr_2.write();
        for primal_node_ptr in cluster_2.nodes.drain(..) {
            primal_node_ptr.write().cluster_weak = cluster_ptr_1.downgrade();
            cluster_1.nodes.push(primal_node_ptr);
        }
        for &edge_index in cluster_2.edges.iter() {
            if !cluster_1.edges.contains(&edge_index) {
                cluster_1.edges.insert(edge_index);
                cluster_1.matrix.add_variable(edge_index);
            }
        }
        for &vertex_index in cluster_2.vertices.iter() {
            if !cluster_1.vertices.contains(&vertex_index) {
                cluster_1.vertices.insert(vertex_index);
                cluster_1.matrix.add_parity_check_with_dual_module(vertex_index, dual_module);
            }
        }
    }

    /// analyze a cluster and return whether there exists an optimal solution (depending on optimization levels)
    fn resolve_cluster(&mut self, cluster_index: usize, interface: &DualModuleInterfacePtr, dual_module: &mut impl DualModuleImpl) -> bool {
        let cluster_ptr = self.clusters[cluster_index].clone();
        let mut cluster = cluster_ptr.write();
        if cluster.nodes.is_empty() {
            return true  // no longer a cluster, no need to handle
        }
        // set all nodes to stop growing in the cluster
        for primal_node_ptr in cluster.nodes.iter() {
            let dual_node_ptr = primal_node_ptr.read_recursive().dual_node_ptr.clone();
            dual_module.set_grow_rate(&dual_node_ptr, Rational::zero());
        }
        // update the matrix with new tight edges
        cluster.matrix.clear_implicit_shrink();
        cluster.matrix.update_with_dual_module(dual_module);
        let tight_edges = cluster.matrix.get_tight_edges();

        // 1. check if the cluster is valid (union-find decoder)
        cluster.matrix.row_echelon_form();
        if self.enable_debug {
            self.debug_recordings.lock().push(DebugEntry::EchelonFormMatrix {
                matrix: cluster.matrix.clone(),
                hair_edges: vec![],
            })
        }
        if !cluster.matrix.echelon_satisfiable {
            if self.enable_debug { self.debug_recordings.lock().push(DebugEntry::Error(format!("invalid cluster of tight edges {:?}", tight_edges))) }
            let internal_edges: BTreeSet<EdgeIndex> = tight_edges.into_iter().collect();
            let dual_node_ptr = interface.create_cluster_node_auto_vertices(internal_edges, dual_module);
            let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                dual_node_ptr: dual_node_ptr.clone(),
                cluster_weak: cluster_ptr.downgrade(),
            });
            cluster.nodes.push(primal_node_ptr.clone());
            self.nodes.push(primal_node_ptr);
            return false
        }
        if self.enable_debug { self.debug_recordings.lock().push(DebugEntry::Message(format!("It's a valid cluster"))) }
        if self.optimization_level <= OPT_LEVEL_UNION_FIND {  // this is the last step, generate a subgraph solution
            if self.enable_debug { self.debug_recordings.lock().push(DebugEntry::Error(format!("algorithm early terminate: optimization level {}", self.optimization_level))) }
            cluster.subgraph = Some(Subgraph::new(cluster.matrix.get_joint_solution().expect("satisfiable")));
            return true  // stop here according to the optimization level specification
        }

        // 2. check whether independent single-hair solution exists for every non-zero dual variable
        let cluster_nodes = cluster.nodes.clone();
        for primal_node_ptr in cluster_nodes.iter() {
            let dual_node_ptr = primal_node_ptr.read_recursive().dual_node_ptr.clone();
            let dual_node = dual_node_ptr.read_recursive();
            if dual_node.dual_variable.is_zero() {
                continue  // no requirement on zero dual variables
            }
            cluster.matrix.clear_implicit_shrink();
            let hair_edges: Vec<EdgeIndex> = dual_node.hair_edges.iter().cloned().collect();
            let mut first_implicit_shrink_edges: Option<Vec<EdgeIndex>> = None;
            loop {
                let implicit_shrink_edges = match cluster.matrix.get_implicit_shrink_edges(&hair_edges) {
                    Some(implicit_shrink_edges) => implicit_shrink_edges,
                    None => {  // it's already unsatisfiable, need to execute the previous actions
                        let first_implicit_shrink_edges = first_implicit_shrink_edges.expect("should not be unsatisfiable before the first shrink is executed");
                        drop(dual_node);
                        dual_module.set_grow_rate(&dual_node_ptr, -Rational::one());
                        let mut edges: BTreeSet<EdgeIndex> = tight_edges.iter().cloned().collect();
                        let implicit_shrink_edges_set: BTreeSet<EdgeIndex> = first_implicit_shrink_edges.iter().cloned().collect();
                        for &edge_index in hair_edges.iter() {
                            if !implicit_shrink_edges_set.contains(&edge_index) {
                                edges.remove(&edge_index);
                            }
                        }
                        let growing_dual_node_ptr = interface.create_cluster_node_auto_vertices(edges, dual_module);
                        let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                            dual_node_ptr: growing_dual_node_ptr.clone(),
                            cluster_weak: cluster_ptr.downgrade(),
                        });
                        cluster.nodes.push(primal_node_ptr.clone());
                        self.nodes.push(primal_node_ptr);
                        return false
                    },
                };
                if implicit_shrink_edges.is_empty() {
                    break  // finally found a single-hair solution
                }
                if first_implicit_shrink_edges.is_none() {
                    first_implicit_shrink_edges = Some(implicit_shrink_edges.clone());
                }
                cluster.matrix.add_implicit_shrink(&implicit_shrink_edges);
            }
            if self.enable_debug {
                self.debug_recordings.lock().push(DebugEntry::EchelonFormMatrix {
                    matrix: cluster.matrix.clone(),
                    hair_edges: hair_edges.clone(),
                })
            }
            if self.enable_debug { self.debug_recordings.lock().push(DebugEntry::Message(format!("Single-hair solution is found among hair edges {:?}", hair_edges))) }
        }
        if self.enable_debug { self.debug_recordings.lock().push(DebugEntry::Message(format!("Every dual variable has single-hair solution independently"))) }
        if self.optimization_level <= OPT_LEVEL_INDEPENDENT_SINGLE_HAIR {  // this is the last step, generate a subgraph solution
            if self.enable_debug { self.debug_recordings.lock().push(DebugEntry::Error(format!("algorithm early terminate: optimization level {}", self.optimization_level))) }
            cluster.subgraph = Some(Subgraph::new(cluster.matrix.get_joint_solution_local_minimum(&self.initializer).expect("satisfiable")));
            return true  // stop here according to the optimization level specification
        }

        true
    }

}

/*
Implementing visualization functions
*/

impl MWPSVisualizer for PrimalModuleSerial {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        let debug_recordings: Option<Vec<serde_json::Value>> = if self.enable_debug {
            let mut existing_recordings = self.debug_recordings.lock();
            let mut debug_recordings = vec![];
            for entry in existing_recordings.iter() {
                debug_recordings.push(entry.snapshot(abbrev));
            }
            existing_recordings.clear();
            Some(debug_recordings)
        } else { None };
        json!({
            "debug_recordings": debug_recordings,
        })
    }
}

impl MWPSVisualizer for DebugEntry {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        match self {
            DebugEntry::EchelonFormMatrix { matrix, hair_edges } => {
                json!({
                    "type": "echelon_form_matrix",
                    "matrix": matrix.to_visualize_json(hair_edges, abbrev),
                    "hair_edges": hair_edges,
                })
            },
            DebugEntry::Message(message) => {
                json!({
                    "type": "message",
                    "message": message,
                })
            },
            DebugEntry::Error(error) => {
                json!({
                    "type": "error",
                    "error": error,
                })
            }
        }
    }
}


#[cfg(test)]
pub mod tests {
    use super::*;
    use super::super::example_codes::*;
    use super::super::dual_module_serial::*;
    use crate::num_traits::FromPrimitive;

    pub fn primal_module_serial_basic_standard_syndrome_optional_viz(mut code: impl ExampleCode, visualize_filename: Option<String>, defect_vertices: Vec<VertexIndex>, final_dual: Weight)
            -> (DualModuleInterfacePtr, PrimalModuleSerial, DualModuleSerial) {
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
        let mut primal_module = PrimalModuleSerial::new_empty(&initializer);
        // try to work on a simple syndrome
        code.set_defect_vertices(&defect_vertices);
        let interface_ptr = DualModuleInterfacePtr::new_empty();
        primal_module.solve_visualizer(&interface_ptr, &code.get_syndrome(), &mut dual_module, visualizer.as_mut());
        let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module, &initializer);
        if let Some(visualizer) = visualizer.as_mut() {
            visualizer.snapshot_combined("subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph, &weight_range]).unwrap();
        }
        assert!(initializer.matches_subgraph_syndrome(&subgraph, &defect_vertices), "the result subgraph is invalid");
        assert_eq!(Rational::from_usize(final_dual).unwrap(), weight_range.upper, "unmatched sum dual variables");
        assert_eq!(Rational::from_usize(final_dual).unwrap(), weight_range.lower, "unexpected final dual variable sum");
        (interface_ptr, primal_module, dual_module)
    }

    pub fn primal_module_serial_basic_standard_syndrome(code: impl ExampleCode, visualize_filename: String, defect_vertices: Vec<VertexIndex>, final_dual: Weight)
            -> (DualModuleInterfacePtr, PrimalModuleSerial, DualModuleSerial) {
        primal_module_serial_basic_standard_syndrome_optional_viz(code, Some(visualize_filename), defect_vertices, final_dual)
    }

    /// test a simple case
    #[test]
    fn primal_module_serial_basic_1() {  // cargo test primal_module_serial_basic_1 -- --nocapture
        let visualize_filename = format!("primal_module_serial_basic_1.json");
        let defect_vertices = vec![23, 24, 29, 30];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(code, visualize_filename, defect_vertices, 1);
    }

    #[test]
    fn primal_module_serial_basic_2() {  // cargo test primal_module_serial_basic_2 -- --nocapture
        let visualize_filename = format!("primal_module_serial_basic_2.json");
        let defect_vertices = vec![16, 17, 23, 25, 29, 30];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(code, visualize_filename, defect_vertices, 2);
    }

    #[test]
    fn primal_module_serial_basic_3() {  // cargo test primal_module_serial_basic_3 -- --nocapture
        let visualize_filename = format!("primal_module_serial_basic_3.json");
        let defect_vertices = vec![14, 15, 16, 17, 22, 25, 28, 31, 36, 37, 38, 39];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(code, visualize_filename, defect_vertices, 5);
    }

    /// this is a case where the union find version will deterministically fail to decode, 
    /// because not all edges are fully grown and those fully grown will lead to suboptimal result
    #[test]
    fn primal_module_serial_basic_4() {  // cargo test primal_module_serial_basic_4 -- --nocapture
        let visualize_filename = format!("primal_module_serial_basic_4.json");
        let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(code, visualize_filename, defect_vertices, 4);
    }

    /// debug case: cargo run --release -- benchmark 5 0.1 --code-config='{"pxy":0}' --verifier strict-actual-error -p serial --print-syndrome-pattern --print-error-pattern
    /// error_pattern: [3, 5, 6, 10, 15, 17, 18, 24]
    #[test]
    fn primal_module_serial_basic_5() {  // cargo test primal_module_serial_basic_5 -- --nocapture
        let visualize_filename = format!("primal_module_serial_basic_5.json");
        let defect_vertices = vec![1, 4, 6, 7, 8, 9, 10, 16, 18, 19, 20, 23];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(code, visualize_filename, defect_vertices, 8);
    }

    /// debug case: cargo run --release -- benchmark 3 0.1 --code-config='{"pxy":0}' --verifier strict-actual-error -p serial --print-syndrome-pattern --print-error-pattern
    /// error_pattern: [2, 4, 5]
    #[test]
    fn primal_module_serial_basic_6() {  // cargo test primal_module_serial_basic_6 -- --nocapture
        let visualize_filename = format!("primal_module_serial_basic_6.json");
        let defect_vertices = vec![0, 3, 4, 5, 7];
        let code = CodeCapacityTailoredCode::new(3, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(code, visualize_filename, defect_vertices, 3);
    }

}
