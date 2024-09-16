//! Serial Primal Module
//!
//! This implementation targets to be an exact MWPF solver, although it's not yet sure whether it is actually one.
//!
#![cfg_attr(feature="unsafe_pointer", allow(dropping_references))]

use color_print::cprintln;
use crate::decoding_hypergraph::*;
use crate::dual_module::*;
use crate::invalid_subgraph::*;
use crate::matrix::*;
use crate::num_traits::{One, Zero};
use crate::plugin::*;
use crate::pointers::*;
use crate::primal_module::*;
use crate::relaxer_optimizer::*;
use crate::util::*;
use crate::visualize::*;

use std::collections::BTreeMap;
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Instant;
use std::cmp::Ordering;

use crate::itertools::Itertools;
#[cfg(feature = "incr_lp")]
use parking_lot::Mutex;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::ops::DerefMut;

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};

use crate::dual_module_parallel::*;
use crate::dual_module_pq::*;

pub struct PrimalModuleSerial {
    /// growing strategy, default to single-tree approach for easier debugging and better locality
    pub growing_strategy: GrowingStrategy,
    /// dual nodes information
    pub nodes: Vec<PrimalModuleSerialNodePtr>,
    /// clusters of dual nodes
    pub clusters: Vec<PrimalClusterPtr>,
    /// pending dual variables to grow, when using SingleCluster growing strategy
    pending_nodes: VecDeque<PrimalModuleSerialNodeWeak>,
    /// plugins
    pub plugins: Arc<PluginVec>,
    /// how many plugins are actually executed for every cluster
    pub plugin_count: Arc<RwLock<usize>>,
    pub plugin_pending_clusters: Vec<PrimalClusterWeak>,
    /// configuration
    pub config: PrimalModuleSerialConfig,
    /// the time spent on resolving the obstacles
    pub time_resolve: f64,
    /// sorted clusters by affinity, only exist when needed
    pub sorted_clusters_aff: Option<BTreeSet<ClusterAffinity>>,
}

#[derive(Eq, Debug)]
pub struct ClusterAffinity {
    pub cluster_ptr: PrimalClusterPtr,
    pub affinity: Affinity,
}

impl PartialEq for ClusterAffinity {
    fn eq(&self, other: &Self) -> bool {
        self.affinity == other.affinity && self.cluster_ptr.eq(&other.cluster_ptr) 
    }
}

// first sort by affinity in descending order, then by cluster_index in ascending order
impl Ord for ClusterAffinity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // First, compare affinity in descending order
        match other.affinity.cmp(&self.affinity) {
            std::cmp::Ordering::Equal => {
                // If affinities are equal, compare cluster_index in ascending order
                self.cluster_ptr.read_recursive().cluster_index.cmp(&other.cluster_ptr.read_recursive().cluster_index)
            }
            other => other,
        }
    }
}

impl PartialOrd for ClusterAffinity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimalModuleSerialConfig {
    /// timeout for the whole solving process
    #[serde(default = "primal_serial_default_configs::timeout")]
    pub timeout: f64,
}

pub mod primal_serial_default_configs {
    pub fn timeout() -> f64 {
        (10 * 60) as f64
    }
}

/// strategy of growing the dual variables
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum GrowingStrategy {
    /// focus on a single cluster at a time, for easier debugging and better locality
    SingleCluster, // Question: Should this be deprecated?
    /// all clusters grow at the same time at the same speed
    MultipleClusters,
    /// utilizing the search/tune mode separation
    ModeBased,
}

pub struct PrimalModuleSerialNode {
    /// the dual node
    pub dual_node_ptr: DualNodePtr,
    /// the cluster that it belongs to
    pub cluster_weak: PrimalClusterWeak,
}

pub type PrimalModuleSerialNodePtr = ArcManualSafeLock<PrimalModuleSerialNode>;
pub type PrimalModuleSerialNodeWeak = WeakManualSafeLock<PrimalModuleSerialNode>;

impl std::fmt::Debug for PrimalModuleSerialNodePtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let primal_node = self.read_recursive(); // reading index is consistent
        write!(
            f,
            "dual_node_ptr: {:?}\ncluster_index: {:?}",
            primal_node.dual_node_ptr,
            primal_node.cluster_weak.upgrade_force().read_recursive().cluster_index,
        )
    }
}

pub struct PrimalCluster {
    /// the index in the cluster
    pub cluster_index: NodeIndex,
    /// the nodes that belongs to this cluster
    pub nodes: Vec<PrimalModuleSerialNodePtr>,
    /// all the edges ever exists in any hair
    pub edges: BTreeSet<EdgePtr>,
    /// all the vertices ever touched by any tight edge
    pub vertices: BTreeSet<VertexPtr>,
    /// the parity matrix to determine whether it's a valid cluster and also find new ways to increase the dual
    pub matrix: EchelonMatrix,
    /// the parity subgraph result, only valid when it's solved
    pub subgraph: Option<Subgraph>,
    /// plugin manager helps to execute the plugin and find an executable relaxer
    pub plugin_manager: PluginManager,
    /// optimizing the direction of relaxers
    pub relaxer_optimizer: RelaxerOptimizer,
    /// HIHGS solution stored for incrmental lp
    #[cfg(feature = "incr_lp")] //note: really depends where we want the error to manifest
    pub incr_solution: Option<Arc<Mutex<IncrLPSolution>>>,
}

pub type PrimalClusterPtr = ArcManualSafeLock<PrimalCluster>;
pub type PrimalClusterWeak = WeakManualSafeLock<PrimalCluster>;

impl std::fmt::Debug for PrimalClusterPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let cluster = self.read_recursive(); // reading index is consistent
        write!(
            f,
            "cluster_index: {:?}\tnodes: {:?}\tedges: {:?}\nvertices: {:?}\nsubgraph: {:?}",
            cluster.cluster_index,
            cluster.nodes,
            cluster.edges,
            cluster.vertices,
            cluster.subgraph,
        )
    }
}


impl Ord for PrimalClusterPtr {
    fn cmp(&self, other: &Self) -> Ordering {
        // compare the pointer address 
        let ptr1 = Arc::as_ptr(self.ptr());
        let ptr2 = Arc::as_ptr(other.ptr());
        // https://doc.rust-lang.org/reference/types/pointer.html
        // "When comparing raw pointers they are compared by their address, rather than by what they point to."
        ptr1.cmp(&ptr2)
    }
}

impl PartialOrd for PrimalClusterPtr {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PrimalModuleImpl for PrimalModuleSerial {
    fn new_empty(_initializer: &SolverInitializer) -> Self {
        Self {
            growing_strategy: GrowingStrategy::SingleCluster,
            nodes: vec![],
            clusters: vec![],
            pending_nodes: VecDeque::new(),
            plugins: Arc::new(vec![]), // default to UF decoder, i.e., without any plugins
            plugin_count: Arc::new(RwLock::new(1)),
            plugin_pending_clusters: vec![],
            config: serde_json::from_value(json!({})).unwrap(),
            time_resolve: 0.,
            sorted_clusters_aff: None,
        }
    }

    fn clear(&mut self) {
        self.nodes.clear();
        self.clusters.clear();
        self.pending_nodes.clear();
        *self.plugin_count.write() = 1;
        self.plugin_pending_clusters.clear();
        self.time_resolve = 0.;
    }

    #[allow(clippy::unnecessary_cast)]
    fn load<D: DualModuleImpl>(&mut self, interface_ptr: &DualModuleInterfacePtr, dual_module: &mut D) {
        let interface = interface_ptr.read_recursive();
        // println!("interface.nodes len: {:?}", interface.nodes.len());
        for index in 0..interface.nodes.len() as NodeIndex {
            let dual_node_ptr = &interface.nodes[index as usize];
            let node = dual_node_ptr.read_recursive();
            debug_assert!(
                node.invalid_subgraph.edges.is_empty(),
                "must load a fresh dual module interface, found a complex node"
            );
            debug_assert!(
                node.invalid_subgraph.vertices.len() == 1,
                "must load a fresh dual module interface, found invalid defect node"
            );
            debug_assert_eq!(
                node.index, index,
                "must load a fresh dual module interface, found index out of order"
            );
            assert_eq!(node.index as usize, self.nodes.len(), "must load defect nodes in order");
            // construct cluster and its parity matrix (will be reused over all iterations)
            let primal_cluster_ptr = PrimalClusterPtr::new_value(PrimalCluster {
                cluster_index: self.clusters.len() as NodeIndex,
                nodes: vec![],
                edges: node.invalid_subgraph.hair.clone(),
                vertices: node.invalid_subgraph.vertices.clone(),
                matrix: node.invalid_subgraph.generate_matrix(),
                subgraph: None,
                plugin_manager: PluginManager::new(self.plugins.clone(), self.plugin_count.clone()),
                relaxer_optimizer: RelaxerOptimizer::new(),
                #[cfg(all(feature = "incr_lp", feature = "highs"))]
                incr_solution: None,
            });
            // create the primal node of this defect node and insert into cluster
            let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                dual_node_ptr: dual_node_ptr.clone(),
                cluster_weak: primal_cluster_ptr.downgrade(),
            });
            drop(node);
            primal_cluster_ptr.write().nodes.push(primal_node_ptr.clone());
            // fill in the primal_module_serial_node in the corresponding dual node
            dual_node_ptr.write().primal_module_serial_node = Some(primal_node_ptr.clone().downgrade());
            
            // add to self
            self.nodes.push(primal_node_ptr);
            self.clusters.push(primal_cluster_ptr);
        }
        if matches!(self.growing_strategy, GrowingStrategy::SingleCluster) {
            for primal_node_ptr in self.nodes.iter().skip(1) {
                let dual_node_ptr = primal_node_ptr.read_recursive().dual_node_ptr.clone();
                dual_module.set_grow_rate(&dual_node_ptr, Rational::zero());
                self.pending_nodes.push_back(primal_node_ptr.downgrade());
            }
        }

    }

    fn resolve(
        &mut self,
        group_max_update_length: GroupMaxUpdateLength,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool {
        let begin = Instant::now();
        let res = self.resolve_core(group_max_update_length, interface_ptr, dual_module);
        self.time_resolve += begin.elapsed().as_secs_f64();
        res
    }

    fn old_resolve(
        &mut self,
        group_max_update_length: GroupMaxUpdateLength,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool {
        let begin = Instant::now();
        let res = self.old_resolve_core(group_max_update_length, interface_ptr, dual_module);
        self.time_resolve += begin.elapsed().as_secs_f64();
        res
    }

    fn resolve_tune(
        &mut self,
        group_max_update_length: BTreeSet<MaxUpdateLength>,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> (BTreeSet<MaxUpdateLength>, bool) {
        let begin = Instant::now();
        let res = self.resolve_core_tune(group_max_update_length, interface_ptr, dual_module);
        self.time_resolve += begin.elapsed().as_secs_f64();
        res
    }

    fn subgraph(
        &mut self,
        _interface: &DualModuleInterfacePtr,
        seed: u64,
    ) -> Subgraph {
        
        let mut subgraph = vec![];
        for cluster_ptr in self.clusters.iter() {
            let cluster = cluster_ptr.read_recursive();
            if cluster.nodes.is_empty() {
                continue;
            }
            // println!("cluster.subgraph: {:?}", cluster.subgraph);
            // println!("cluster: {:?}", cluster_ptr);
        
            subgraph.extend(
                cluster
                    .subgraph
                    .clone()
                    .unwrap_or_else(|| panic!("bug occurs: cluster should be solved, but the subgraph is not yet generated || the seed is {seed:?}")),
            );
    
           
        }
        // println!("subgraph: {:?}", subgraph);
        subgraph
    }

    /// check if there are more plugins to be applied
    ///     will return false if timeout has been reached, else consume a plugin
    fn has_more_plugins(&mut self) -> bool {
        if self.time_resolve > self.config.timeout {
            return false;
        }
        return if *self.plugin_count.read_recursive() < self.plugins.len() {
            // increment the plugin count
            *self.plugin_count.write() += 1;
            // self.plugin_pending_clusters = (0..self.clusters.len()).collect();
            // println!("start printing self.clusters for has_more_plugin");
            // println!("self.clusters.len: {:?}", self.clusters.len());
            // for cluster in self.clusters.iter() {
            //     println!("cluster {:?} in self.cluster", cluster.read_recursive().cluster_index);
            // }
            // println!("finish printing self.clusters for has_more_plugin");

            self.plugin_pending_clusters = self.clusters.iter().map(|c| c.downgrade()).collect();
            // println!("start printing self.plugin_pending_clusters");
            // for cluster in self.plugin_pending_clusters.iter() {
            //     println!("cluster {:?} in self.plugin_pending_clusters", cluster.upgrade_force().read_recursive().cluster_index);
            // }
            // println!("finish printing self.plugin_pending_clusters");
            true
        } else {
            false
        };
    }

    /// get the pending clusters
    fn pending_clusters(&mut self) -> Vec<PrimalClusterWeak> {
        self.plugin_pending_clusters.clone()
    }

    // TODO: extract duplicate codes

    /// analyze a cluster and return whether there exists an optimal solution (depending on optimization levels)
    #[allow(clippy::unnecessary_cast)]
    fn resolve_cluster(
        &mut self,
        cluster_ptr: &PrimalClusterPtr,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool {
        // cprintln!("<red>resolver cluster</red>");
        // cprintln!("This a <green,bold>green and bold text</green,bold>.");

        // let cluster_ptr = self.clusters[cluster_index as usize].clone();
        let mut cluster = cluster_ptr.write();
        if cluster.nodes.is_empty() {
            return true; // no longer a cluster, no need to handle
        }
        // set all nodes to stop growing in the cluster
        for primal_node_ptr in cluster.nodes.iter() {
            let dual_node_ptr = primal_node_ptr.read_recursive().dual_node_ptr.clone();
            dual_module.set_grow_rate(&dual_node_ptr, Rational::zero());
        }
        // update the matrix with new tight edges
        let cluster = &mut *cluster;
        for edge_weak in cluster.edges.iter() {
            // println!("{:?} cluster edge: {:?}", i, edge_weak.read_recursive().edge_index);
            cluster
                .matrix
                .update_edge_tightness(edge_weak.downgrade(), dual_module.is_edge_tight(edge_weak.clone()));
        }

        // find an executable relaxer from the plugin manager
        let relaxer = {
            let positive_dual_variables: Vec<DualNodePtr> = cluster
                .nodes
                .iter()
                .map(|p| p.read_recursive().dual_node_ptr.clone())
                .filter(|dual_node_ptr| !dual_node_ptr.read_recursive().get_dual_variable().is_zero())
                .collect();
            let cluster_mut = &mut *cluster; // must first get mutable reference
            let plugin_manager = &mut cluster_mut.plugin_manager;
            let matrix = &mut cluster_mut.matrix;
            plugin_manager.find_relaxer( matrix, &positive_dual_variables)
        };

        // if a relaxer is found, execute it and return
        if let Some(relaxer) = relaxer {
            for (invalid_subgraph, grow_rate) in relaxer.get_direction() {
                let (existing, dual_node_ptr) = interface_ptr.find_or_create_node(invalid_subgraph, dual_module);
                if !existing {
                    // create the corresponding primal node and add it to cluster
                    let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                        dual_node_ptr: dual_node_ptr.clone(),
                        cluster_weak: cluster_ptr.downgrade(),
                    });
                    cluster.nodes.push(primal_node_ptr.clone());
                    self.nodes.push(primal_node_ptr.clone());
                    dual_node_ptr.write().primal_module_serial_node = Some(primal_node_ptr.downgrade());
                }

                dual_module.set_grow_rate(&dual_node_ptr, grow_rate.clone());
            }
            cluster.relaxer_optimizer.insert(relaxer);
            return false;
        }

        // TODO idea: plugins can suggest subgraph (ideally, a global maximum), if so, then it will adopt th
        // subgraph with minimum weight from all plugins as the starting point to do local minimum

        // find a local minimum (hopefully a global minimum)
        let weight_of = |edge_weak: EdgeWeak| edge_weak.upgrade_force().read_recursive().weight;
        cluster.subgraph = Some(cluster.matrix.get_solution_local_minimum(weight_of).expect("satisfiable"));
        true
    }

    /// analyze a cluster and return whether there exists an optimal solution (depending on optimization levels)
    #[allow(clippy::unnecessary_cast)]
    fn resolve_cluster_tune(
        &mut self,
        cluster_ptr: &PrimalClusterPtr,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
        // dual_node_deltas: &mut BTreeMap<OrderedDualNodePtr, Rational>,
        dual_node_deltas: &mut BTreeMap<OrderedDualNodePtr, (Rational, PrimalClusterPtr)>,
    ) -> (bool, OptimizerResult) {
        let mut optimizer_result = OptimizerResult::default();
        // let cluster_ptr = self.clusters[cluster_index as usize].clone();
        
        let mut cluster = cluster_ptr.write();
        // println!(
        //     "cluster_index: {:?}\tnodes: {:?}\tedges: {:?}\nvertices: {:?}\nsubgraph: {:?}",
        //     cluster.cluster_index,
        //     cluster.nodes,
        //     cluster.edges,
        //     cluster.vertices,
        //     cluster.subgraph,
        // );
        if cluster.nodes.is_empty() {
            // println!("cluster.nodes.is_empty");
            return (true, optimizer_result); // no longer a cluster, no need to handle
        }
        // update the matrix with new tight edges
        let cluster = &mut *cluster;
        for edge_index in cluster.edges.iter() {
            cluster
                .matrix
                .update_edge_tightness(edge_index.downgrade(), dual_module.is_edge_tight_tune(edge_index.clone()));
        }

        // find an executable relaxer from the plugin manager
        let relaxer = {
            let positive_dual_variables: Vec<DualNodePtr> = cluster
                .nodes
                .iter()
                .map(|p| p.read_recursive().dual_node_ptr.clone())
                .filter(|dual_node_ptr| !dual_node_ptr.read_recursive().dual_variable_at_last_updated_time.is_zero())
                .collect();
            let cluster_mut = &mut *cluster; // must first get mutable reference
            let plugin_manager = &mut cluster_mut.plugin_manager;
            let matrix = &mut cluster_mut.matrix;
            // matrix.printstd();
            plugin_manager.find_relaxer( matrix, &positive_dual_variables)
        };

        // println!("relaxer: {:?}", relaxer);
        // if a relaxer is found, execute it and return
        if let Some(mut relaxer) = relaxer {
            #[cfg(feature = "float_lp")]
            // float_lp is enabled, optimizer really plays a role
            if cluster.relaxer_optimizer.should_optimize(&relaxer) {
                #[cfg(not(feature = "incr_lp"))]
                {
                    let dual_variables: BTreeMap<Arc<InvalidSubgraph>, Rational> = cluster
                        .nodes
                        .iter()
                        .map(|primal_node_ptr| {
                            let primal_node = primal_node_ptr.read_recursive();
                            let dual_node = primal_node.dual_node_ptr.read_recursive();
                            (
                                dual_node.invalid_subgraph.clone(),
                                dual_node.dual_variable_at_last_updated_time.clone(),
                            )
                        })
                        .collect();
                    let edge_slacks: BTreeMap<EdgePtr, Rational> = dual_variables
                        .keys()
                        .flat_map(|invalid_subgraph: &Arc<InvalidSubgraph>| invalid_subgraph.hair.iter())
                        .chain(
                            relaxer
                                .get_direction()
                                .keys()
                                .flat_map(|invalid_subgraph| invalid_subgraph.hair.iter()),
                        )
                        .unique()
                        .map(|edge_index| (edge_index.clone(), dual_module.get_edge_slack_tune(edge_index.clone())))
                        .collect();
                    let (new_relaxer, early_returned) =
                        cluster.relaxer_optimizer.optimize(relaxer, edge_slacks, dual_variables);
                    relaxer = new_relaxer;
                    if early_returned {
                        optimizer_result = OptimizerResult::EarlyReturned;
                    } else {
                        optimizer_result = OptimizerResult::Optimized;
                    }
                }

                #[cfg(feature = "incr_lp")]
                {
                    let mut dual_variables: BTreeMap<NodeIndex, (Arc<InvalidSubgraph>, Rational)> = BTreeMap::new();
                    let mut participating_dual_variable_indices = hashbrown::HashSet::new();
                    for primal_node_ptr in cluster.nodes.iter() {
                        let primal_node = primal_node_ptr.read_recursive();
                        let dual_node = primal_node.dual_node_ptr.read_recursive();
                        dual_variables.insert(
                            dual_node.index,
                            (
                                dual_node.invalid_subgraph.clone(),
                                dual_node.dual_variable_at_last_updated_time,
                            ),
                        );
                        participating_dual_variable_indices.insert(dual_node.index);
                    }

                    for (invalid_subgraph, _) in relaxer.get_direction().iter() {
                        let (existing, dual_node_ptr) =
                            interface_ptr.find_or_create_node_tune(invalid_subgraph, dual_module);
                        if !existing {
                            // create the corresponding primal node and add it to cluster
                            let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                                dual_node_ptr: dual_node_ptr.clone(),
                                cluster_weak: cluster_ptr.downgrade(),
                            });
                            cluster.nodes.push(primal_node_ptr.clone());
                            self.nodes.push(primal_node_ptr);
                            // participating_dual_variable_indices.insert(dual_node_ptr.read_recursive().index);

                            // maybe optimize here
                        }
                        match dual_variables.get_mut(&dual_node_ptr.read_recursive().index) {
                            Some(_) => {}
                            None => {
                                dual_variables.insert(
                                    dual_node_ptr.read_recursive().index,
                                    (
                                        dual_node_ptr.read_recursive().invalid_subgraph.clone(),
                                        dual_node_ptr.read_recursive().dual_variable_at_last_updated_time,
                                    ),
                                );
                            }
                        };
                    }
                    let edge_free_weights: BTreeMap<EdgeIndex, Rational> = dual_variables
                        .values()
                        .flat_map(|(invalid_subgraph, _)| invalid_subgraph.hair.iter().cloned())
                        .chain(
                            relaxer
                                .get_direction()
                                .keys()
                                .flat_map(|invalid_subgraph| invalid_subgraph.hair.iter().cloned()),
                        )
                        .unique()
                        .map(|edge_index| {
                            (
                                edge_index,
                                // dual_module.get_edge_free_weight(edge_index, &participating_dual_variable_indices),
                                dual_module.get_edge_free_weight_cluster(edge_index, cluster_index),
                            )
                        })
                        .collect();

                    let (new_relaxer, early_returned) = cluster.relaxer_optimizer.optimize_incr(
                        relaxer,
                        edge_free_weights,
                        dual_variables,
                        &mut cluster.incr_solution,
                    );
                    relaxer = new_relaxer;
                    if early_returned {
                        optimizer_result = OptimizerResult::EarlyReturned;
                    } else {
                        optimizer_result = OptimizerResult::Optimized;
                    }
                }
            } else {
                optimizer_result = OptimizerResult::Skipped;
            }

            #[cfg(not(feature = "float_lp"))]
            // with rationals, it is actually usually better when always optimized
            {
                let dual_variables: BTreeMap<Arc<InvalidSubgraph>, Rational> = cluster
                    .nodes
                    .iter()
                    .map(|primal_node_ptr| {
                        let primal_node = primal_node_ptr.read_recursive();
                        let dual_node = primal_node.dual_node_ptr.read_recursive();
                        (
                            dual_node.invalid_subgraph.clone(),
                            dual_node.dual_variable_at_last_updated_time.clone(),
                        )
                    })
                    .collect();
                let edge_slacks: BTreeMap<EdgeIndex, Rational> = dual_variables
                    .keys()
                    .flat_map(|invalid_subgraph: &Arc<InvalidSubgraph>| invalid_subgraph.hair.iter().cloned())
                    .chain(
                        relaxer
                            .get_direction()
                            .keys()
                            .flat_map(|invalid_subgraph| invalid_subgraph.hair.iter().cloned()),
                    )
                    .unique()
                    .map(|edge_index| (edge_index, dual_module.get_edge_slack_tune(edge_index)))
                    .collect();

                let (new_relaxer, early_returned) = cluster.relaxer_optimizer.optimize(relaxer, edge_slacks, dual_variables);
                relaxer = new_relaxer;
                if early_returned {
                    optimizer_result = OptimizerResult::EarlyReturned;
                } else {
                    optimizer_result = OptimizerResult::Optimized;
                }
            }

            for (invalid_subgraph, grow_rate) in relaxer.get_direction() {
                let (existing, dual_node_ptr) = interface_ptr.find_or_create_node_tune(invalid_subgraph, dual_module);
                if !existing {
                    // create the corresponding primal node and add it to cluster
                    let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                        dual_node_ptr: dual_node_ptr.clone(),
                        cluster_weak: cluster_ptr.downgrade(),
                    });
                    cluster.nodes.push(primal_node_ptr.clone());
                    self.nodes.push(primal_node_ptr.clone());
                    dual_node_ptr.write().primal_module_serial_node = Some(primal_node_ptr.downgrade());
                }

                // Document the desired deltas
                let index = dual_node_ptr.read_recursive().index;
                dual_node_deltas.insert(
                    OrderedDualNodePtr::new(index, dual_node_ptr),
                    (grow_rate.clone(), cluster_ptr.clone()),
                );
            }

            cluster.relaxer_optimizer.insert(relaxer);
            return (false, optimizer_result);
        }

        // find a local minimum (hopefully a global minimum)
        // let interface = interface_ptr.read_recursive();
        // let initializer = interface.decoding_graph.model_graph.initializer.as_ref();
        // let weight_of = |edge_index: EdgeIndex| initializer.weighted_edges[edge_index].weight;
        let weight_of = |edge_weak: EdgeWeak| edge_weak.upgrade_force().read_recursive().weight;
        cluster.subgraph = Some(cluster.matrix.get_solution_local_minimum(weight_of).expect("satisfiable"));

        (true, optimizer_result)
    }

    /// update the sorted clusters_aff, should be None to start with
    fn update_sorted_clusters_aff<D: DualModuleImpl>(&mut self, dual_module: &mut D) {
        let pending_clusters = self.pending_clusters();

        // println!("print pending clusters");
        // for cluster_weak in pending_clusters.iter() {
        //     let cluster_ptr = cluster_weak.upgrade_force();
        //     println!("cluster in pending clusters: {:?}", cluster_ptr.read_recursive().cluster_index);
        // }
        // println!("finished printing pending clusters");
        let mut sorted_clusters_aff = BTreeSet::default();

        for cluster_index in pending_clusters.iter() {
            // let cluster_ptr = self.clusters[*cluster_index].clone();
            let cluster_ptr = cluster_index.upgrade_force();
            let affinity = dual_module.calculate_cluster_affinity(cluster_ptr.clone());
            if let Some(affinity) = affinity {
                sorted_clusters_aff.insert(ClusterAffinity {
                    cluster_ptr: cluster_ptr.clone(),
                    affinity,
                });
            }
        }

        // println!("print sorted clusters aff");
        // for cluster_aff in sorted_clusters_aff.iter() {
        //     println!("cluster in sorted cluster aff: {:?}", cluster_aff.cluster_ptr.read_recursive().cluster_index);
        // }
        // println!("finished printing sorted clusters aff");

        self.sorted_clusters_aff = Some(sorted_clusters_aff);
    }

    /// consume the sorted_clusters_aff
    fn get_sorted_clusters_aff(&mut self) -> BTreeSet<ClusterAffinity> {
        self.sorted_clusters_aff.take().unwrap()
    }

    #[cfg(feature = "incr_lp")]
    fn calculate_edges_free_weight_clusters(&mut self, dual_module: &mut impl DualModuleImpl) {
        for cluster in self.clusters.iter() {
            let cluster = cluster.read_recursive();
            for node in cluster.nodes.iter() {
                let dual_node = node.read_recursive();
                let dual_node_read = dual_node.dual_node_ptr.read_recursive();
                for edge_index in dual_node_read.invalid_subgraph.hair.iter() {
                    dual_module.update_edge_cluster_weights(
                        *edge_index,
                        cluster.cluster_index,
                        dual_node_read.dual_variable_at_last_updated_time,
                    );
                }
            }
        }
    }
}

impl PrimalModuleSerial {
    // union the cluster of two dual nodes
    #[allow(clippy::unnecessary_cast)]
    pub fn union(
        &self,
        dual_node_ptr_1: &DualNodePtr,
        dual_node_ptr_2: &DualNodePtr,
        dual_module: &mut impl DualModuleImpl, // note: remove if not for cluster-based
    ) {
        // cluster_1 will become the union of cluster_1 and cluster_2
        // and cluster_2 will be outdated
        // let node_index_1 = dual_node_ptr_1.read_recursive().index;
        // let node_index_2 = dual_node_ptr_2.read_recursive().index;
        // let primal_node_1 = self.nodes[node_index_1 as usize].read_recursive();
        // let primal_node_2 = self.nodes[node_index_2 as usize].read_recursive();
        let primal_node_1_weak = dual_node_ptr_1.read_recursive().primal_module_serial_node.clone().unwrap();
        let primal_node_2_weak = dual_node_ptr_2.read_recursive().primal_module_serial_node.clone().unwrap();
        let primal_node_1_ptr = primal_node_1_weak.upgrade_force();
        let primal_node_2_ptr = primal_node_2_weak.upgrade_force();
        let primal_node_1 = primal_node_1_ptr.read_recursive();
        let primal_node_2 = primal_node_2_ptr.read_recursive();
        if primal_node_1.cluster_weak.eq(&primal_node_2.cluster_weak) {
            return; // already in the same cluster
        }
        let cluster_ptr_1 = primal_node_1.cluster_weak.upgrade_force();
        let cluster_ptr_2 = primal_node_2.cluster_weak.upgrade_force();
        drop(primal_node_1);
        drop(primal_node_2);
        let mut cluster_1 = cluster_ptr_1.write();
        let mut cluster_2 = cluster_ptr_2.write();
        let cluster_2_index = cluster_2.cluster_index;
        for primal_node_ptr in cluster_2.nodes.drain(..) {
            #[cfg(feature = "incr_lp")]
            {
                let primal_node = primal_node_ptr.read_recursive();
                dual_module.update_edge_cluster_weights_union(
                    &primal_node.dual_node_ptr,
                    cluster_2_index,
                    cluster_1.cluster_index,
                );
            }

            primal_node_ptr.write().cluster_weak = cluster_ptr_1.downgrade();
            cluster_1.nodes.push(primal_node_ptr);
        }
        cluster_1.edges.extend(&mut cluster_2.edges.clone().into_iter());
        cluster_1.subgraph = None; // mark as no subgraph

        #[cfg(all(feature = "incr_lp", feature = "highs"))]
        match (&cluster_1.incr_solution, &cluster_2.incr_solution) {
            (None, Some(_)) => {
                cluster_1.incr_solution = cluster_2.incr_solution.take();
            }
            (Some(c1), Some(c2)) => {
                if c2.lock().constraints_len() > c1.lock().constraints_len() {
                    cluster_1.incr_solution = cluster_2.incr_solution.take();
                }
            }

            // no need to changes
            (None, None) => {}
            (Some(_), None) => {}
        }

        for vertex_ptr in cluster_2.vertices.iter() {
            if !cluster_1.vertices.contains(&vertex_ptr.clone()) {
                cluster_1.vertices.insert(vertex_ptr.clone());
                // let parity = decoding_graph.is_vertex_defect(vertex_index);
                let incident_edges = &vertex_ptr.read_recursive().edges;
                // let incident_edges = &vertex_ptr.get_edge_neighbors();
                let parity = vertex_ptr.read_recursive().is_defect;
                cluster_1.matrix.add_constraint(vertex_ptr.downgrade(), incident_edges, parity);
            }
        }
        cluster_1.relaxer_optimizer.append(&mut cluster_2.relaxer_optimizer);
        cluster_2.vertices.clear();
    }

    #[allow(clippy::unnecessary_cast)]
    fn resolve_core(
        &mut self,
        mut group_max_update_length: GroupMaxUpdateLength,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool {
        debug_assert!(!group_max_update_length.is_unbounded() && group_max_update_length.get_valid_growth().is_none());
        let mut active_clusters = BTreeSet::<PrimalClusterPtr>::new();
        let interface = interface_ptr.read_recursive();
        // println!("in resolve core");
        while let Some(conflict) = group_max_update_length.pop() {
            // println!("conflict in resolve_core: {:?}", conflict);
            match conflict {
                MaxUpdateLength::Conflicting(edge_ptr) => {
                    // union all the dual nodes in the edge index and create new dual node by adding this edge to `internal_edges`
                    // println!("conflict edge_ptr: {:?}", edge_ptr);
                    let dual_nodes = dual_module.get_edge_nodes(edge_ptr.clone());
                    debug_assert!(
                        !dual_nodes.is_empty(),
                        "should not conflict if no dual nodes are contributing"
                    );
                    let dual_node_ptr_0 = &dual_nodes[0];
                    // first union all the dual nodes
                    for dual_node_ptr in dual_nodes.iter().skip(1) {
                        self.union(dual_node_ptr_0, dual_node_ptr, dual_module);
                    }
                    let primal_node_weak = dual_node_ptr_0.read_recursive().primal_module_serial_node.clone().unwrap();
                    let cluster_ptr = primal_node_weak.upgrade_force().read_recursive().cluster_weak.upgrade_force();
                    // let cluster_ptr = self.nodes[dual_node_ptr_0.read_recursive().index as usize]
                    //     .read_recursive()
                    //     .cluster_weak
                    //     .upgrade_force();
                    let mut cluster = cluster_ptr.write();
                    // then add new constraints because these edges may touch new vertices
                    // let incident_vertices = &edge_ptr.get_vertex_neighbors();
                    let incident_vertices = &edge_ptr.read_recursive().vertices;
                    // println!("incidenet_vertices: {:?}", incident_vertices);
                    // println!("cluster matrix before add constraint: {:?}", cluster.matrix.printstd());
                    for vertex_weak in incident_vertices.iter() {
                        // println!("incident vertex: {:?}", vertex_weak.upgrade_force().read_recursive().vertex_index);
                        if !cluster.vertices.contains(&vertex_weak.upgrade_force()) {
                            cluster.vertices.insert(vertex_weak.upgrade_force());
                            let vertex_ptr = vertex_weak.upgrade_force();
                            let vertex = vertex_ptr.read_recursive();
                            let incident_edges = &vertex.edges;
                            // let incident_edges = &vertex_ptr.get_edge_neighbors();
                            // println!("vertex {:?}, fusion_done: {:?}, is_mirror: {:?}, incident_edges: {:?}", vertex_ptr.read_recursive().vertex_index,
                            // vertex_ptr.read_recursive().fusion_done, vertex_ptr.read_recursive().is_mirror, incident_edges);
                            let parity = vertex.is_defect;
                            
                            cluster.matrix.add_constraint(vertex_weak.clone(), &incident_edges, parity);
                        }
                    }
                    // println!("cluster matrix after add constraint: {:?}", cluster.matrix.printstd());
                    cluster.edges.insert(edge_ptr.clone());
                    // add to active cluster so that it's processed later
                    active_clusters.insert(cluster_ptr.clone());
                }
                MaxUpdateLength::ShrinkProhibited(dual_node_ptr) => {
                    let primal_node_weak = dual_node_ptr.ptr.read_recursive().primal_module_serial_node.clone().unwrap();
                    let cluster_ptr = primal_node_weak.upgrade_force().read_recursive().cluster_weak.upgrade_force();
                    // let cluster_ptr = self.nodes[dual_node_ptr.index as usize]
                    //     .read_recursive()
                    //     .cluster_weak
                    //     .upgrade_force();
                    // let cluster_index = cluster_ptr.read_recursive().cluster_index;
                    active_clusters.insert(cluster_ptr.clone());
                }
                _ => {
                    unreachable!()
                }
            }
        }
        drop(interface);
        if *self.plugin_count.read_recursive() != 0 && self.time_resolve > self.config.timeout {
            *self.plugin_count.write() = 0; // force only the first plugin
        }
        let mut all_solved = true;
        for cluster_ptr in active_clusters.iter() {
            // println!("active cluster index: {:?}", cluster_ptr.read_recursive().cluster_index);
            let solved = self.resolve_cluster(cluster_ptr, interface_ptr, dual_module);
            all_solved &= solved;
        }
        if !all_solved {
            return false; // already give dual module something to do
        }

        true
    }

    #[allow(clippy::unnecessary_cast)]
    /// for backwards-compatibility
    fn old_resolve_core(
        &mut self,
        mut group_max_update_length: GroupMaxUpdateLength,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool {
        debug_assert!(!group_max_update_length.is_unbounded() && group_max_update_length.get_valid_growth().is_none());
        let mut active_clusters = BTreeSet::<PrimalClusterPtr>::new();
        let interface = interface_ptr.read_recursive();
        while let Some(conflict) = group_max_update_length.pop() {
            match conflict {
                MaxUpdateLength::Conflicting(edge_ptr) => {
                    // union all the dual nodes in the edge index and create new dual node by adding this edge to `internal_edges`
                    let dual_nodes = dual_module.get_edge_nodes(edge_ptr.clone());
                    debug_assert!(
                        !dual_nodes.is_empty(),
                        "should not conflict if no dual nodes are contributing"
                    );
                    let dual_node_ptr_0 = &dual_nodes[0];
                    // first union all the dual nodes
                    for dual_node_ptr in dual_nodes.iter().skip(1) {
                        println!("iiii");
                        // self.union(dual_node_ptr_0, dual_node_ptr, &interface.decoding_graph);
                        self.union(dual_node_ptr_0, dual_node_ptr,  dual_module);
                    }
                    let cluster_ptr = self.nodes[dual_node_ptr_0.read_recursive().index as usize]
                        .read_recursive()
                        .cluster_weak
                        .upgrade_force();
                    let mut cluster = cluster_ptr.write();
                    // then add new constraints because these edges may touch new vertices
                    // let incident_vertices = decoding_graph.get_edge_neighbors(edge_index);
                    let incident_vertices = &edge_ptr.read_recursive().vertices;
                    for vertex_weak in incident_vertices.iter() {
                        if !cluster.vertices.contains(&vertex_weak.upgrade_force()) {
                            cluster.vertices.insert(vertex_weak.upgrade_force());
                            // let parity = decoding_graph.is_vertex_defect(vertex_index);
                            let vertex_ptr = vertex_weak.upgrade_force();
                            let vertex = vertex_ptr.read_recursive();
                            let incident_edges = &vertex.edges;
                            let parity = vertex.is_defect;
                            cluster.matrix.add_constraint(vertex_weak.clone(), incident_edges, parity);
                        }
                    }
                    cluster.edges.insert(edge_ptr.clone());
                    // add to active cluster so that it's processed later
                    active_clusters.insert(cluster_ptr.clone());
                }
                MaxUpdateLength::ShrinkProhibited(dual_node_ptr) => {
                    let cluster_ptr = self.nodes[dual_node_ptr.index as usize]
                        .read_recursive()
                        .cluster_weak
                        .upgrade_force();
                    // let cluster_index = cluster_ptr.read_recursive().cluster_index;
                    active_clusters.insert(cluster_ptr.clone());
                }
                _ => {
                    unreachable!()
                }
            }
        }
        drop(interface);
        if *self.plugin_count.read_recursive() != 0 && self.time_resolve > self.config.timeout {
            *self.plugin_count.write() = 0; // force only the first plugin
        }
        let mut all_solved = true;
        for cluster_index in active_clusters.iter() {
            let solved = self.resolve_cluster(cluster_index, interface_ptr, dual_module);
            all_solved &= solved;
        }
        if !all_solved {
            return false; // already give dual module something to do
        }
        while !self.pending_nodes.is_empty() {
            let primal_node_weak = self.pending_nodes.pop_front().unwrap();
            let primal_node_ptr = primal_node_weak.upgrade_force();
            let primal_node = primal_node_ptr.read_recursive();
            let cluster_ptr = primal_node.cluster_weak.upgrade_force();
            if cluster_ptr.read_recursive().subgraph.is_none() {
                dual_module.set_grow_rate(&primal_node.dual_node_ptr, Rational::one());
                return false; // let the dual module to find more obstacles
            }
        }
        if *self.plugin_count.read_recursive() == 0 {
            return true;
        }
        // check that all clusters have passed the plugins
        loop {
            while let Some(cluster_index) = self.plugin_pending_clusters.pop() {
                let solved = self.resolve_cluster(&cluster_index.upgrade_force(), interface_ptr, dual_module);
                if !solved {
                    return false; // let the dual module to handle one
                }
            }
            if *self.plugin_count.read_recursive() < self.plugins.len() {
                // increment the plugin count
                *self.plugin_count.write() += 1;
                // self.plugin_pending_clusters = (0..self.clusters.len()).collect();
                self.plugin_pending_clusters = self.clusters.iter().map(|c| c.downgrade()).collect();
            } else {
                break; // nothing more to check
            }
        }
        true
    }

    #[allow(clippy::unnecessary_cast)]
    // returns (conflicts_needing_to_be_resolved, should_grow)
    fn resolve_core_tune(
        &mut self,
        group_max_update_length: BTreeSet<MaxUpdateLength>,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> (BTreeSet<MaxUpdateLength>, bool) {
        let mut active_clusters = BTreeSet::<PrimalClusterPtr>::new();
        let interface = interface_ptr.read_recursive();
        for conflict in group_max_update_length.into_iter() {
            // println!("conflict: {:?}", conflict);
            match conflict {
                MaxUpdateLength::Conflicting(edge_ptr) => {
                    // union all the dual nodes in the edge index and create new dual node by adding this edge to `internal_edges`
                    let dual_nodes = dual_module.get_edge_nodes(edge_ptr.clone());
                    debug_assert!(
                        !dual_nodes.is_empty(),
                        "should not conflict if no dual nodes are contributing"
                    );
                    let dual_node_ptr_0 = &dual_nodes[0];
                    // first union all the dual nodes
                    for dual_node_ptr in dual_nodes.iter().skip(1) {
                        // self.union(dual_node_ptr_0, dual_node_ptr, &interface.decoding_graph);
                        self.union(dual_node_ptr_0, dual_node_ptr, dual_module);
                    }
                    let primal_node_weak = dual_node_ptr_0.read_recursive().primal_module_serial_node.clone().unwrap();
                    let cluster_ptr = primal_node_weak.upgrade_force().read_recursive().cluster_weak.upgrade_force();
                    // let cluster_ptr = self.nodes[dual_node_ptr_0.read_recursive().index as usize]
                    //     .read_recursive()
                    //     .cluster_weak
                    //     .upgrade_force();
                    let mut cluster = cluster_ptr.write();
                    // then add new constraints because these edges may touch new vertices
                    let incident_vertices = &edge_ptr.read_recursive().vertices;
                    // let incident_vertices = &edge_ptr.get_vertex_neighbors();
                    for vertex_weak in incident_vertices.iter() {
                        if !cluster.vertices.contains(&vertex_weak.upgrade_force()) {
                            cluster.vertices.insert(vertex_weak.upgrade_force());
                            // let parity = decoding_graph.is_vertex_defect(vertex_index);
                            let vertex_ptr = vertex_weak.upgrade_force();
                            let vertex = vertex_ptr.read_recursive();
                            let incident_edges = &vertex.edges;
                            // let incident_edges = &vertex_ptr.get_edge_neighbors();
                            let parity = vertex.is_defect;
                            cluster.matrix.add_constraint(vertex_weak.clone(), incident_edges, parity);
                        }
                    }
                    cluster.edges.insert(edge_ptr.clone());
                    // add to active cluster so that it's processed later
                    active_clusters.insert(cluster_ptr.clone());
                }
                MaxUpdateLength::ShrinkProhibited(dual_node_ptr) => {
                    let primal_node_weak = dual_node_ptr.ptr.read_recursive().primal_module_serial_node.clone().unwrap();
                    let cluster_ptr = primal_node_weak.upgrade_force().read_recursive().cluster_weak.upgrade_force();
                    // let cluster_ptr = self.nodes[dual_node_ptr.index as usize]
                    //     .read_recursive()
                    //     .cluster_weak
                    //     .upgrade_force();
                    // let cluster_index = cluster_ptr.read_recursive().cluster_index;
                    active_clusters.insert(cluster_ptr.clone());
                }
                _ => {
                    unreachable!()
                }
            }
        }
        drop(interface);
        if *self.plugin_count.read_recursive() != 0 && self.time_resolve > self.config.timeout {
            *self.plugin_count.write() = 0; // force only the first plugin
        }
        let mut all_solved = true;
        let mut dual_node_deltas = BTreeMap::new();
        let mut optimizer_result = OptimizerResult::default();
        for cluster_ptr in active_clusters.iter() {
            let (solved, other) =
                self.resolve_cluster_tune(cluster_ptr, interface_ptr, dual_module, &mut dual_node_deltas);
            if !solved {
                // todo: investigate more
                return (dual_module.get_conflicts_tune(other, dual_node_deltas), false);
            }
            all_solved &= solved;
            optimizer_result.or(other);
        }

        let all_conflicts = dual_module.get_conflicts_tune(optimizer_result, dual_node_deltas);

        (all_conflicts, all_solved)
    }
}


impl PrimalModuleSerial {
    // // for parallel
    // #[allow(clippy::unnecessary_cast)]
    // fn load_ptr<DualSerialModule: DualModuleImpl + Send + Sync, Queue>(
    //     &mut self, 
    //     interface_ptr: &DualModuleInterfacePtr, 
    //     dual_module_ptr: &mut DualModuleParallelUnitPtr<DualSerialModule, Queue>,
    // ) where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
    // {
    //     let interface = interface_ptr.read_recursive();
    //     println!("interface.nodes len: {:?}", interface.nodes.len());
    //     for index in 0..interface.nodes.len() as NodeIndex {
    //         let dual_node_ptr = &interface.nodes[index as usize];
    //         let node = dual_node_ptr.read_recursive();
    //         debug_assert!(
    //             node.invalid_subgraph.edges.is_empty(),
    //             "must load a fresh dual module interface, found a complex node"
    //         );
    //         debug_assert!(
    //             node.invalid_subgraph.vertices.len() == 1,
    //             "must load a fresh dual module interface, found invalid defect node"
    //         );
    //         debug_assert_eq!(
    //             node.index, index,
    //             "must load a fresh dual module interface, found index out of order"
    //         );
    //         assert_eq!(node.index as usize, self.nodes.len(), "must load defect nodes in order");
    //         // construct cluster and its parity matrix (will be reused over all iterations)
    //         let primal_cluster_ptr = PrimalClusterPtr::new_value(PrimalCluster {
    //             cluster_index: self.clusters.len() as NodeIndex,
    //             nodes: vec![],
    //             edges: node.invalid_subgraph.hair.clone(),
    //             vertices: node.invalid_subgraph.vertices.clone(),
    //             matrix: node.invalid_subgraph.generate_matrix(),
    //             subgraph: None,
    //             plugin_manager: PluginManager::new(self.plugins.clone(), self.plugin_count.clone()),
    //             relaxer_optimizer: RelaxerOptimizer::new(),
    //             #[cfg(all(feature = "incr_lp", feature = "highs"))]
    //             incr_solution: None,
    //         });
    //         // create the primal node of this defect node and insert into cluster
    //         let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
    //             dual_node_ptr: dual_node_ptr.clone(),
    //             cluster_weak: primal_cluster_ptr.downgrade(),
    //         });
    //         drop(node);
    //         primal_cluster_ptr.write().nodes.push(primal_node_ptr.clone());
    //         // fill in the primal_module_serial_node in the corresponding dual node
    //         dual_node_ptr.write().primal_module_serial_node = Some(primal_node_ptr.clone().downgrade());
            
    //         // add to self
    //         self.nodes.push(primal_node_ptr);
    //         self.clusters.push(primal_cluster_ptr);
    //     }
    //     if matches!(self.growing_strategy, GrowingStrategy::SingleCluster) {
    //         for primal_node_ptr in self.nodes.iter().skip(1) {
    //             let dual_node_ptr = primal_node_ptr.read_recursive().dual_node_ptr.clone();
    //             dual_module_ptr.write().set_grow_rate(&dual_node_ptr, Rational::zero());
    //             self.pending_nodes.push_back(primal_node_ptr.downgrade());
    //         }
    //     }
    // }

    // for parallel 
    pub fn solve_step_callback_ptr<DualSerialModule: DualModuleImpl + Send + Sync, Queue, F>(
        &mut self,
        interface: &DualModuleInterfacePtr,
        syndrome_pattern: Arc<SyndromePattern>,
        dual_module_ptr: &mut DualModuleParallelUnitPtr<DualSerialModule, Queue>,
        callback: F,
    ) where
        F: FnMut(&DualModuleInterfacePtr, &DualModuleParallelUnit<DualSerialModule, Queue>, &mut Self, &GroupMaxUpdateLength),
        Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
    {
        // let mut dual_module = dual_module_ptr.write();
        // interface.load_ptr(syndrome_pattern, dual_module_ptr);
        interface.load(syndrome_pattern, dual_module_ptr.write().deref_mut());
        self.load(interface, dual_module_ptr.write().deref_mut());
        // drop(dual_module);
        self.solve_step_callback_interface_loaded_ptr(interface, dual_module_ptr, callback);
    }

    
    pub fn solve_step_callback_interface_loaded_ptr<DualSerialModule: DualModuleImpl + Send + Sync, Queue, F>(
        &mut self,
        interface: &DualModuleInterfacePtr,
        dual_module_ptr: &mut DualModuleParallelUnitPtr<DualSerialModule, Queue>,
        mut callback: F,
    ) where
        F: FnMut(&DualModuleInterfacePtr, &DualModuleParallelUnit<DualSerialModule, Queue>, &mut Self, &GroupMaxUpdateLength),
        Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
    {
        // println!(" in solve step callback interface loaded ptr");
        // Search, this part is unchanged
        let mut group_max_update_length = dual_module_ptr.compute_maximum_update_length();
        // println!("first group max update length: {:?}", group_max_update_length);

        while !group_max_update_length.is_unbounded() {
            callback(interface, &dual_module_ptr.read_recursive(), self, &group_max_update_length);
            match group_max_update_length.get_valid_growth() {
                Some(length) => dual_module_ptr.grow(length),
                None => {
                    self.resolve(group_max_update_length, interface, dual_module_ptr.write().deref_mut());
                }
            }
            group_max_update_length = dual_module_ptr.compute_maximum_update_length();
            // println!("group max update length: {:?}", group_max_update_length);
        }

        // from here, all states should be syncronized
        let mut start = true;

        // starting with unbounded state here: All edges and nodes are not growing as of now
        // Tune
        let mut dual_module = dual_module_ptr.write();
        while self.has_more_plugins() {
            // println!("self.has more plugins");
            // Note: intersting, seems these aren't needed... But just kept here in case of future need, as well as correctness related failures
            if start {
                start = false;
                dual_module.advance_mode();
                #[cfg(feature = "incr_lp")]
                self.calculate_edges_free_weight_clusters(dual_module);
            }
            self.update_sorted_clusters_aff(dual_module.deref_mut());
            let cluster_affs = self.get_sorted_clusters_aff();

            // println!("start counting");
            // for cluster_affinity in cluster_affs.iter() {
            //     let cluster_ptr = &cluster_affinity.cluster_ptr;
            //     println!("cluster {:?} in cluster_affinity", cluster_ptr.read_recursive().cluster_index);
            // }
            // println!("finished counting");
            // println!("cluster_aff: {:?}")

            for cluster_affinity in cluster_affs.into_iter() {
                let cluster_ptr = cluster_affinity.cluster_ptr;
                let mut dual_node_deltas = BTreeMap::new();
                let (mut resolved, optimizer_result) =
                self.resolve_cluster_tune(&cluster_ptr, interface, dual_module.deref_mut(), &mut dual_node_deltas);

                let mut conflicts = dual_module.get_conflicts_tune(optimizer_result, dual_node_deltas);
                while !resolved {
                    let (_conflicts, _resolved) = self.resolve_tune(conflicts, interface, dual_module.deref_mut());
                    if _resolved {
                        break;
                    }
                    conflicts = _conflicts;
                    resolved = _resolved;
                }
            }
        }
        drop(dual_module);
    }
}

impl MWPSVisualizer for PrimalModuleSerial {
    fn snapshot(&self, _abbrev: bool) -> serde_json::Value {
        json!({})
    }
}

#[cfg(test)]
pub mod tests {
    use super::super::dual_module_pq::*;
    use super::super::dual_module_serial::*;
    use super::super::example_codes::*;
    use super::*;
    use crate::dual_module;
    use crate::num_traits::FromPrimitive;
    use crate::plugin_single_hair::PluginSingleHair;
    use crate::plugin_union_find::PluginUnionFind;

    #[allow(clippy::too_many_arguments)]
    pub fn primal_module_serial_basic_standard_syndrome_optional_viz(
        _code: impl ExampleCode,
        defect_vertices: Vec<VertexIndex>,
        final_dual: Weight,
        plugins: PluginVec,
        growing_strategy: GrowingStrategy,
        mut dual_module: impl DualModuleImpl + MWPSVisualizer + Send + Sync,
        model_graph: Arc<crate::model_hypergraph::ModelHyperGraph>,
        mut visualizer: Option<Visualizer>,
    ) -> (
        DualModuleInterfacePtr,
        PrimalModuleSerial,
        impl DualModuleImpl + MWPSVisualizer,
    ) {
        // create primal module
        let mut primal_module = PrimalModuleSerial::new_empty(&model_graph.initializer);
        primal_module.growing_strategy = growing_strategy;
        primal_module.plugins = Arc::new(plugins);
        // primal_module.config = serde_json::from_value(json!({"timeout":1})).unwrap();
        // try to work on a simple syndrome
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, defect_vertices.clone());
        let interface_ptr = DualModuleInterfacePtr::new();

        let begin_time = std::time::Instant::now();
        primal_module.solve_visualizer(
            &interface_ptr,
            decoding_graph.syndrome_pattern.clone(),
            &mut dual_module,
            None,
        );

        let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, 0);
        // if let Some(visualizer) = visualizer.as_mut() {
        //     visualizer
        //         .snapshot_combined(
        //             "subgraph".to_string(),
        //             vec![&interface_ptr, &dual_module, &subgraph, &weight_range],
        //         )
        //         .unwrap();
        // }
        assert!(
            decoding_graph
                .model_graph
                .matches_subgraph_syndrome(&subgraph, &defect_vertices),
            "the result subgraph is invalid"
        );
        primal_module.clear();
        dual_module.clear();
        let end_time = std::time::Instant::now();
        println!("resolve_time: {:?}", end_time - begin_time);
        // assert_eq!(
        //     Rational::from_usize(final_dual).unwrap(),
        //     weight_range.upper,
        //     "unmatched sum dual variables"
        // );
        // assert_eq!(
        //     Rational::from_usize(final_dual).unwrap(),
        //     weight_range.lower,
        //     "unexpected final dual variable sum"
        // );
        (interface_ptr, primal_module, dual_module)
    }

    pub fn primal_module_serial_basic_standard_syndrome(
        code: impl ExampleCode,
        visualize_filename: String,
        defect_vertices: Vec<VertexIndex>,
        final_dual: Weight,
        plugins: PluginVec,
        growing_strategy: GrowingStrategy,
    ) -> (
        DualModuleInterfacePtr,
        PrimalModuleSerial,
        impl DualModuleImpl + MWPSVisualizer,
    ) {
        println!("{defect_vertices:?}");
        let visualizer = {
            let visualizer = Visualizer::new(
                Some(visualize_data_folder() + visualize_filename.as_str()),
                code.get_positions(),
                true,
            )
            .unwrap();
            print_visualize_link(visualize_filename.clone());
            visualizer
        };
        // create dual module
        let model_graph = code.get_model_graph();
        let mut dual_module: DualModulePQ<FutureObstacleQueue<Rational>> = DualModulePQ::new_empty(&model_graph.initializer);
        primal_module_serial_basic_standard_syndrome_optional_viz(
            code,
            defect_vertices,
            final_dual,
            plugins,
            growing_strategy,
            dual_module,
            model_graph,
            None,
        )
    }

    pub fn primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
        code: impl ExampleCode,
        visualize_filename: String,
        defect_vertices: Vec<VertexIndex>,
        final_dual: Weight,
        plugins: PluginVec,
        growing_strategy: GrowingStrategy,
    ) -> (
        DualModuleInterfacePtr,
        PrimalModuleSerial,
        impl DualModuleImpl + MWPSVisualizer,
    ) {
        println!("{defect_vertices:?}");
        let visualizer = {
            let visualizer = Visualizer::new(
                Some(visualize_data_folder() + visualize_filename.as_str()),
                code.get_positions(),
                true,
            )
            .unwrap();
            print_visualize_link(visualize_filename.clone());
            visualizer
        };
        // create dual module
        let model_graph = code.get_model_graph();
        primal_module_serial_basic_standard_syndrome_optional_viz(
            code,
            defect_vertices,
            final_dual,
            plugins,
            growing_strategy,
            DualModulePQ::<FutureObstacleQueue<Rational>>::new_empty(&model_graph.initializer),
            model_graph,
            None,
        )
    }

    /// test a simple case
    #[test]
    fn primal_module_serial_basic_1_m() {
        // cargo test primal_module_serial_basic_1_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_1_m.json".to_string();
        // let defect_vertices = vec![23, 24, 29, 30];
        // let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        let weight = 1;
        let code = CodeCapacityPlanarCode::new(7, 0.1, weight);
        let defect_vertices = vec![16, 19, 29, 39];
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_1_with_dual_pq_impl_m() {
        // cargo test -r primal_module_serial_basic_1_with_dual_pq_impl_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_1_with_dual_pq_impl_m.json".to_string();
        let defect_vertices = vec![23, 24, 29, 30];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices,
            1,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_2_m() {
        // cargo test primal_module_serial_basic_2_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_2_m.json".to_string();
        let defect_vertices = vec![16, 17, 23, 25, 29, 30];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            2,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_2_with_dual_pq_impl_m() {
        // cargo test primal_module_serial_basic_2_with_dual_pq_impl_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_2_with_dual_pq_impl_m.json".to_string();
        let defect_vertices = vec![16, 17, 23, 25, 29, 30];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices,
            2,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    // should fail because single growing will have sum y_S = 3 instead of 5
    #[test]
    // #[should_panic] no more panics, as we are not using the single growing strategy
    fn primal_module_serial_basic_3_single_m() {
        // cargo test primal_module_serial_basic_3_single_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_3_single_m.json".to_string();
        let defect_vertices = vec![14, 15, 16, 17, 22, 25, 28, 31, 36, 37, 38, 39];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            5,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    // #[should_panic] no more panics, as we are not using the single growing strategy
    fn primal_module_serial_basic_3_single_with_dual_pq_impl_m() {
        // cargo test primal_module_serial_basic_3_single_with_dual_pq_impl_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_3_single_with_dual_pq_impl_m.json".to_string();
        let defect_vertices = vec![14, 15, 16, 17, 22, 25, 28, 31, 36, 37, 38, 39];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices,
            5,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_3_improved_m() {
        // cargo test primal_module_serial_basic_3_improved_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_3_improved_m.json".to_string();
        let defect_vertices = vec![14, 15, 16, 17, 22, 25, 28, 31, 36, 37, 38, 39];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            5,
            vec![
                PluginUnionFind::entry(),
                PluginSingleHair::entry_with_strategy(RepeatStrategy::Once),
            ],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_3_improved_with_dual_pq_impl_m() {
        // cargo test primal_module_serial_basic_3_improved_with_dual_pq_impl_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_3_improved_with_dual_pq_impl_m.json".to_string();
        let defect_vertices = vec![14, 15, 16, 17, 22, 25, 28, 31, 36, 37, 38, 39];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices,
            5,
            vec![
                PluginUnionFind::entry(),
                PluginSingleHair::entry_with_strategy(RepeatStrategy::Once),
            ],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_3_multi_m() {
        // cargo test primal_module_serial_basic_3_multi_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_3_multi_m.json".to_string();
        let defect_vertices = vec![14, 15, 16, 17, 22, 25, 28, 31, 36, 37, 38, 39];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            5,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_3_multi_with_dual_pq_impl_m() {
        // cargo test primal_module_serial_basic_3_multi_with_dual_pq_impl_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_3_multi_with_dual_pq_impl_m.json".to_string();
        let defect_vertices = vec![14, 15, 16, 17, 22, 25, 28, 31, 36, 37, 38, 39];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices,
            5,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    #[should_panic]
    fn primal_module_serial_basic_4_single_m() {
        // cargo test primal_module_serial_basic_4_single_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_single_m.json".to_string();
        let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    #[should_panic]
    fn primal_module_serial_basic_4_single_with_dual_pq_impl_m() {
        // cargo test primal_module_serial_basic_4_single_with_dual_pq_impl_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_single_with_dual_pq_impl_m.json".to_string();
        let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_4_single_improved_m() {
        // cargo test primal_module_serial_basic_4_single_improved_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_single_improved_m.json".to_string();
        let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![
                PluginUnionFind::entry(),
                PluginSingleHair::entry_with_strategy(RepeatStrategy::Once),
            ],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_4_single_improved_with_dual_pq_impl_m() {
        // cargo test primal_module_serial_basic_4_single_improved_with_dual_pq_impl_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_single_improved_with_dual_pq_impl_m.json".to_string();
        let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![
                PluginUnionFind::entry(),
                PluginSingleHair::entry_with_strategy(RepeatStrategy::Once),
            ],
            GrowingStrategy::MultipleClusters,
        );
    }

    /// this is a case where the union find version will deterministically fail to decode,
    /// because not all edges are fully grown and those fully grown edges lead to suboptimal result
    #[test]
    #[should_panic]
    fn primal_module_serial_basic_4_multi_m() {
        // cargo test primal_module_serial_basic_4_multi_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_multi_m.json".to_string();
        let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    #[should_panic]
    fn primal_module_serial_basic_4_multi_with_dual_pq_impl_m() {
        // cargo test primal_module_serial_basic_4_multi_with_dual_pq_impl_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_multi_with_dual_pq_impl_m.json".to_string();
        let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    /// verify that each cluster is indeed growing one by one
    #[test]
    fn primal_module_serial_basic_4_cluster_single_growth_m() {
        // cargo test primal_module_serial_basic_4_cluster_single_growth_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_cluster_single_growth_m.json".to_string();
        let defect_vertices = vec![32, 33, 37, 47, 86, 87, 72, 82];
        let code = CodeCapacityPlanarCode::new(11, 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_4_cluster_single_growth_with_dual_pq_impl_m() {
        // cargo test primal_module_serial_basic_4_cluster_single_growth_with_dual_pq_impl_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_cluster_single_growth_with_dual_pq_impl_m.json".to_string();
        let defect_vertices = vec![32, 33, 37, 47, 86, 87, 72, 82];
        let code = CodeCapacityPlanarCode::new(11, 0.01, 1);
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    /// verify that the plugins are applied one by one
    #[test]
    fn primal_module_serial_basic_4_plugin_one_by_one_m() {
        // cargo test primal_module_serial_basic_4_plugin_one_by_one_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_plugin_one_by_one_m.json".to_string();
        let defect_vertices = vec![12, 22, 23, 32, 17, 26, 27, 37, 62, 72, 73, 82, 67, 76, 77, 87];
        let code = CodeCapacityPlanarCode::new(11, 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            12,
            vec![
                PluginUnionFind::entry(),
                PluginSingleHair::entry_with_strategy(RepeatStrategy::Once),
            ],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_4_plugin_one_by_one_with_dual_pq_impl_m() {
        // cargo test primal_module_serial_basic_4_plugin_one_by_one_with_dual_pq_impl_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_plugin_one_by_one_with_dual_pq_impl_m.json".to_string();
        let defect_vertices = vec![12, 22, 23, 32, 17, 26, 27, 37, 62, 72, 73, 82, 67, 76, 77, 87];
        let code = CodeCapacityPlanarCode::new(11, 0.01, 1);
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices,
            12,
            vec![
                PluginUnionFind::entry(),
                PluginSingleHair::entry_with_strategy(RepeatStrategy::Once),
            ],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_circuit_level_noise_1() {
        // cargo test -r primal_module_serial_circuit_level_noise_1 -- --nocapture
        let config = json!({
            "code_type": qecp::code_builder::CodeType::RotatedPlanarCode,
            "nm": 2000,
        });
        
        let mut code = QECPlaygroundCode::new(7, 0.005, config);
        let defect_vertices = code.clone().generate_random_errors(132).0.defect_vertices;

        let visualize_filename = "primal_module_serial_circuit_level_noise_1.json".to_string();
        primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
            code,
            visualize_filename,
            defect_vertices.clone(),
            9474048,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    // /// feasible but non-optimal solution
    // #[test]
    // fn primal_module_serial_test_for_seed_131() {
    //     // cargo test primal_module_serial_test_for_seed_131 -- --nocapture
    //     let visualize_filename = "primal_module_serial_test_for_seed_131.json".to_string();
    //     let defect_vertices = vec![24, 42, 50, 51, 53, 56, 57, 60, 62, 68, 75, 80, 86, 88, 93, 94, 96, 98, 104, 106, 115, 127, 128, 129, 133, 134, 136, 141, 142, 146, 150, 151, 152, 154, 164, 172, 173, 182, 183, 191, 192, 199, 207, 218, 225, 226, 229, 230, 231, 232, 235, 243, 245, 246, 247, 259, 260, 281, 282, 292, 293, 309, 326];
    //     let code = CodeCapacityPlanarCode::new(19, 0.05, 1000);
    //     primal_module_serial_basic_standard_syndrome_with_dual_pq_impl(
    //         code,
    //         visualize_filename,
    //         defect_vertices,
    //         39000,
    //         vec![
    //             // PluginUnionFind::entry(),
    //             // PluginSingleHair::entry_with_strategy(RepeatStrategy::Once),
    //         ],
    //         GrowingStrategy::ModeBased,
    //     );
    // }
}
