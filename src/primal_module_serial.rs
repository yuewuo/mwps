//! Serial Primal Module
//! 
//! This implementation targets to be an exact MWPS solver, although it's not yet sure whether it is actually one.
//! 

use crate::framework::*;
use crate::parity_matrix::*;
use crate::util::*;
use crate::primal_module::*;
use crate::visualize::*;
use crate::dual_module::*;
use crate::pointers::*;
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Debug;
use crate::num_traits::{Zero, One};
use std::sync::Arc;
use crate::plugin::*;


pub struct PrimalModuleSerial {
    /// growing strategy, default to single-tree approach for easier debugging and better locality
    pub growing_strategy: GrowingStrategy,
    /// dual nodes information
    pub nodes: Vec<PrimalModuleSerialNodePtr>, 
    /// clusters of dual nodes
    pub clusters: Vec<PrimalClusterPtr>,
    /// the indices of live clusters: those actively updating the dual module
    pub live_clusters: BTreeSet<usize>,
    /// pending dual variables to grow, when using SingleCluster growing strategy
    pending_nodes: VecDeque<PrimalModuleSerialNodeWeak>,
    /// plugins
    pub plugins: Vec<Arc<dyn PluginImpl>>,
}

/// strategy of growing the dual variables
#[derive(Debug, Clone, Copy)]
pub enum GrowingStrategy {
    /// focus on a single cluster at a time, for easier debugging and better locality
    SingleCluster,
    /// all clusters grow at the same time at the same speed
    MultipleClusters,
}

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

impl PrimalModuleImpl for PrimalModuleSerial {

    fn new_empty(_initializer: &SolverInitializer) -> Self {
        Self {
            // growing_strategy: GrowingStrategy::SingleCluster,
            growing_strategy: GrowingStrategy::MultipleClusters,
            nodes: vec![],
            clusters: vec![],
            live_clusters: BTreeSet::new(),
            pending_nodes: VecDeque::new(),
            plugins: vec![],  // default to UF decoder, i.e., without any plugins
        }
    }

    fn clear(&mut self) {
        self.nodes.clear();
        self.clusters.clear();
        self.live_clusters.clear();
        self.pending_nodes.clear();
    }

    fn load<D: DualModuleImpl>(&mut self, interface_ptr: &DualModuleInterfacePtr, dual_module: &mut D) {
        let interface = interface_ptr.read_recursive();
        for index in 0..interface.nodes.len() as NodeIndex {
            let dual_node_ptr = &interface.nodes[index as usize];
            let node = dual_node_ptr.read_recursive();
            debug_assert!(node.invalid_subgraph.edges.is_empty(), "must load a fresh dual module interface, found a complex node");
            debug_assert!(node.invalid_subgraph.vertices.len() == 1, "must load a fresh dual module interface, found invalid defect node");
            debug_assert_eq!(node.index, index, "must load a fresh dual module interface, found index out of order");
            assert_eq!(node.index, self.nodes.len(), "must load defect nodes in order");
            assert_eq!(node.index, self.live_clusters.len(), "must load defect nodes in order");
            // construct cluster and its parity matrix (will be reused over all iterations)
            let primal_cluster_ptr = PrimalClusterPtr::new_value(PrimalCluster {
                cluster_index: self.clusters.len(),
                nodes: vec![],
                edges: node.invalid_subgraph.hairs.clone(),
                vertices: node.invalid_subgraph.vertices.clone(),
                matrix: node.invalid_subgraph.generate_matrix(&interface.decoding_graph),
                subgraph: None,
            });
            // create the primal node of this defect node and insert into cluster
            let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                dual_node_ptr: dual_node_ptr.clone(),
                cluster_weak: primal_cluster_ptr.downgrade(),
            });
            primal_cluster_ptr.write().nodes.push(primal_node_ptr.clone());
            // add to self
            self.nodes.push(primal_node_ptr);
            self.live_clusters.insert(self.clusters.len());
            self.clusters.push(primal_cluster_ptr);
        }
        if matches!(self.growing_strategy, GrowingStrategy::SingleCluster) {
            for primal_node_ptr in self.nodes.iter().skip(1) {
                let dual_node_ptr = primal_node_ptr.read_recursive().dual_node_ptr.clone();
                dual_module.set_grow_rate(&dual_node_ptr, Rational::zero());
                self.pending_nodes.push_back(primal_node_ptr.downgrade());
                let cluster_index = primal_node_ptr.read_recursive().cluster_weak.upgrade_force().read_recursive().cluster_index;
                self.live_clusters.remove(&cluster_index);
            }
        }
    }

    fn resolve(&mut self, mut group_max_update_length: GroupMaxUpdateLength, interface_ptr: &DualModuleInterfacePtr, dual_module: &mut impl DualModuleImpl) {
        debug_assert!(!group_max_update_length.is_unbounded() && group_max_update_length.get_valid_growth().is_none());
        let mut active_clusters = BTreeSet::<NodeIndex>::new();
        let interface = interface_ptr.read_recursive();
        let decoding_graph = &interface.decoding_graph;
        while let Some(conflict) = group_max_update_length.pop() {
            match conflict {
                MaxUpdateLength::Conflicting(edge_index) => {
                    // union all the dual nodes in the edge index and create new dual node by adding this edge to `internal_edges`
                    let dual_nodes = dual_module.get_edge_nodes(edge_index);
                    debug_assert!(dual_nodes.len() > 0, "should not conflict if no dual nodes are contributing");
                    let dual_node_ptr_0 = &dual_nodes[0];
                    // first union all the dual nodes
                    for dual_node_ptr in dual_nodes.iter().skip(1) {
                        self.union(dual_node_ptr_0, dual_node_ptr, &interface.decoding_graph);
                    }
                    let cluster_ptr = self.nodes[dual_node_ptr_0.read_recursive().index].read_recursive().cluster_weak.upgrade_force();
                    let mut cluster = cluster_ptr.write();
                    cluster.matrix.add_variable(edge_index);
                    // then add new constraints because these edges may touch new vertices
                    let incident_vertices = decoding_graph.get_edge_neighbors(edge_index);
                    for &vertex_index in incident_vertices.iter() {
                        if !cluster.vertices.contains(&vertex_index) {
                            cluster.vertices.insert(vertex_index);
                            cluster.matrix.add_parity_check_with_decoding_graph(vertex_index, decoding_graph);
                        }
                    }
                    cluster.edges.insert(edge_index);
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
        drop(interface);
        let mut all_solved = true;
        for &cluster_index in active_clusters.iter() {
            let solved = self.resolve_cluster(cluster_index, interface_ptr, dual_module);
            if solved {
                self.live_clusters.remove(&cluster_index);
            }
            all_solved &= solved;
        }
        if all_solved {
            while !self.pending_nodes.is_empty() {
                let primal_node_weak = self.pending_nodes.pop_front().unwrap();
                let primal_node_ptr = primal_node_weak.upgrade_force();
                let primal_node = primal_node_ptr.read_recursive();
                let cluster_ptr = primal_node.cluster_weak.upgrade_force();
                self.live_clusters.insert(cluster_ptr.read_recursive().cluster_index);
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
            subgraph.extend(cluster.subgraph.clone().expect("bug occurs: cluster should be solved, but the subgraph is not yet generated").iter());
        }
        subgraph
    }

}

impl PrimalModuleSerial {

    // union the cluster of two dual nodes
    pub fn union(&self, dual_node_ptr_1: &DualNodePtr, dual_node_ptr_2: &DualNodePtr, decoding_graph: &HyperDecodingGraph) {
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
        cluster_2.edges.clear();
        for &vertex_index in cluster_2.vertices.iter() {
            if !cluster_1.vertices.contains(&vertex_index) {
                cluster_1.vertices.insert(vertex_index);
                cluster_1.matrix.add_parity_check_with_decoding_graph(vertex_index, decoding_graph);
            }
        }
        cluster_2.vertices.clear();
    }

    /// analyze a cluster and return whether there exists an optimal solution (depending on optimization levels)
    fn resolve_cluster(&mut self, cluster_index: usize, interface_ptr: &DualModuleInterfacePtr, dual_module: &mut impl DualModuleImpl) -> bool {
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
        println!("tight_edges: {tight_edges:?}");

        // 1. check if the cluster is valid (hypergraph union-find decoder)
        cluster.matrix.row_echelon_form();
        if !cluster.matrix.echelon_satisfiable {
            let internal_edges: BTreeSet<EdgeIndex> = tight_edges.into_iter().collect();
            let vertices: BTreeSet<VertexIndex> = cluster.vertices.iter().cloned().collect();
            let dual_node_ptr = interface_ptr.create_cluster_node(internal_edges, vertices, dual_module);
            let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                dual_node_ptr: dual_node_ptr.clone(),
                cluster_weak: cluster_ptr.downgrade(),
            });
            cluster.nodes.push(primal_node_ptr.clone());
            self.nodes.push(primal_node_ptr);
            return false
        }

        // TODO: call plugins for further optimizations

        cluster.subgraph = Some(cluster.matrix.get_joint_solution_local_minimum(interface_ptr.read_recursive().decoding_graph.model_graph.initializer.as_ref()).expect("satisfiable"));
        true
    }

}


impl MWPSVisualizer for PrimalModuleSerial {
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
    use crate::num_traits::FromPrimitive;

    pub fn primal_module_serial_basic_standard_syndrome_optional_viz(code: impl ExampleCode, visualize_filename: Option<String>, defect_vertices: Vec<VertexIndex>, final_dual: Weight, plugins: Vec<Arc<dyn PluginImpl>>)
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
        let model_graph = code.get_model_graph();
        let mut dual_module = DualModuleSerial::new_empty(&model_graph.initializer);
        // create primal module
        let mut primal_module = PrimalModuleSerial::new_empty(&model_graph.initializer);
        primal_module.plugins = plugins;
        // try to work on a simple syndrome
        let decoding_graph = HyperDecodingGraph::new_defects(model_graph, defect_vertices.clone());
        let interface_ptr = DualModuleInterfacePtr::new(decoding_graph.model_graph.clone());
        primal_module.solve_visualizer(&interface_ptr, decoding_graph.syndrome_pattern.clone(), &mut dual_module, visualizer.as_mut());
        let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module);
        if let Some(visualizer) = visualizer.as_mut() {
            visualizer.snapshot_combined("subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph, &weight_range]).unwrap();
        }
        assert!(decoding_graph.model_graph.matches_subgraph_syndrome(&subgraph, &defect_vertices), "the result subgraph is invalid");
        assert_eq!(Rational::from_usize(final_dual).unwrap(), weight_range.upper, "unmatched sum dual variables");
        assert_eq!(Rational::from_usize(final_dual).unwrap(), weight_range.lower, "unexpected final dual variable sum");
        (interface_ptr, primal_module, dual_module)
    }

    pub fn primal_module_serial_basic_standard_syndrome(code: impl ExampleCode, visualize_filename: String, defect_vertices: Vec<VertexIndex>, final_dual: Weight, plugins: Vec<Arc<dyn PluginImpl>>)
            -> (DualModuleInterfacePtr, PrimalModuleSerial, DualModuleSerial) {
        primal_module_serial_basic_standard_syndrome_optional_viz(code, Some(visualize_filename), defect_vertices, final_dual, plugins)
    }

    /// test a simple case
    #[test]
    fn primal_module_serial_basic_1() {  // cargo test primal_module_serial_basic_1 -- --nocapture
        let visualize_filename = format!("primal_module_serial_basic_1.json");
        let defect_vertices = vec![23, 24, 29, 30];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(code, visualize_filename, defect_vertices, 1, vec![]);
    }

    #[test]
    fn primal_module_serial_basic_2() {  // cargo test primal_module_serial_basic_2 -- --nocapture
        let visualize_filename = format!("primal_module_serial_basic_2.json");
        let defect_vertices = vec![16, 17, 23, 25, 29, 30];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(code, visualize_filename, defect_vertices, 2, vec![]);
    }

    #[test]
    fn primal_module_serial_basic_3() {  // cargo test primal_module_serial_basic_3 -- --nocapture
        let visualize_filename = format!("primal_module_serial_basic_3.json");
        let defect_vertices = vec![14, 15, 16, 17, 22, 25, 28, 31, 36, 37, 38, 39];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(code, visualize_filename, defect_vertices, 5, vec![]);
    }

    /// this is a case where the union find version will deterministically fail to decode, 
    /// because not all edges are fully grown and those fully grown edges lead to suboptimal result
    #[test]
    fn primal_module_serial_basic_4() {  // cargo test primal_module_serial_basic_4 -- --nocapture
        let visualize_filename = format!("primal_module_serial_basic_4.json");
        let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(code, visualize_filename, defect_vertices, 4, vec![]);
    }

}
