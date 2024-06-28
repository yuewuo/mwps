//! Serial Primal Module
//!
//! This implementation targets to be an exact MWPF solver, although it's not yet sure whether it is actually one.
//!

use crate::decoding_hypergraph::*;
use crate::dual_module::*;
use crate::invalid_subgraph::*;
use crate::matrix::*;
use crate::num_traits::{One, Signed, ToPrimitive, Zero};
use crate::plugin::*;
use crate::pointers::*;
use crate::primal_module::*;
use crate::relaxer::Relaxer;
use crate::relaxer_optimizer::*;
use crate::util::*;
use crate::visualize::*;

use std::collections::BTreeMap;
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Instant;

use highs::HighsModelStatus;
use highs::Model;
use highs::SolvedModel;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

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
    pub plugin_pending_clusters: Vec<usize>,
    /// configuration
    pub config: PrimalModuleSerialConfig,
    /// the time spent on resolving the obstacles
    pub time_resolve: f64,
    /// sorted clusters by affinity, only exist when needed
    pub sorted_clusters_aff: Option<BTreeSet<ClusterAffinity>>,
}

#[derive(Eq, Debug)]
pub struct ClusterAffinity {
    pub cluster_index: NodeIndex,
    pub affinity: Affinity,
}

impl PartialEq for ClusterAffinity {
    fn eq(&self, other: &Self) -> bool {
        self.affinity == other.affinity && self.cluster_index == other.cluster_index
    }
}

// first sort by affinity in descending order, then by cluster_index in ascending order
impl Ord for ClusterAffinity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // First, compare affinity in descending order
        match other.affinity.cmp(&self.affinity) {
            std::cmp::Ordering::Equal => {
                // If affinities are equal, compare cluster_index in ascending order
                self.cluster_index.cmp(&other.cluster_index)
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
    pub matrix: EchelonMatrix,
    /// the parity subgraph result, only valid when it's solved
    pub subgraph: Option<Subgraph>,
    /// plugin manager helps to execute the plugin and find an executable relaxer
    pub plugin_manager: PluginManager,
    /// optimizing the direction of relaxers
    pub relaxer_optimizer: RelaxerOptimizer,

    /* for incrmental LP */
    pub incr_solution: Option<IncrLPSolution>,
}

pub type PrimalClusterPtr = ArcRwLock<PrimalCluster>;
pub type PrimalClusterWeak = WeakRwLock<PrimalCluster>;

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
                matrix: node.invalid_subgraph.generate_matrix(&interface.decoding_graph),
                subgraph: None,
                plugin_manager: PluginManager::new(self.plugins.clone(), self.plugin_count.clone()),
                relaxer_optimizer: RelaxerOptimizer::new(),
                incr_solution: None,
            });
            // create the primal node of this defect node and insert into cluster
            let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                dual_node_ptr: dual_node_ptr.clone(),
                cluster_weak: primal_cluster_ptr.downgrade(),
            });
            primal_cluster_ptr.write().nodes.push(primal_node_ptr.clone());
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
        _dual_module: &mut impl DualModuleImpl,
        seed: u64,
    ) -> Subgraph {
        let mut subgraph = vec![];
        for cluster_ptr in self.clusters.iter() {
            let cluster = cluster_ptr.read_recursive();
            if cluster.nodes.is_empty() {
                continue;
            }
            subgraph.extend(
                cluster
                    .subgraph
                    .clone()
                    .unwrap_or_else(|| panic!("bug occurs: cluster should be solved, but the subgraph is not yet generated || the seed is {seed:?}"))
                    .iter(),
            );
        }
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
            self.plugin_pending_clusters = (0..self.clusters.len()).collect();
            true
        } else {
            false
        };
    }

    /// get the pending clusters
    fn pending_clusters(&mut self) -> Vec<usize> {
        self.plugin_pending_clusters.clone()
    }

    // TODO: extract duplicate codes

    /// analyze a cluster and return whether there exists an optimal solution (depending on optimization levels)
    #[allow(clippy::unnecessary_cast)]
    fn resolve_cluster(
        &mut self,
        cluster_index: NodeIndex,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool {
        let cluster_ptr = self.clusters[cluster_index as usize].clone();
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
        for &edge_index in cluster.edges.iter() {
            cluster
                .matrix
                .update_edge_tightness(edge_index, dual_module.is_edge_tight(edge_index));
        }

        // find an executable relaxer from the plugin manager
        let relaxer = {
            let positive_dual_variables: Vec<DualNodePtr> = cluster
                .nodes
                .iter()
                .map(|p| p.read_recursive().dual_node_ptr.clone())
                .filter(|dual_node_ptr| !dual_node_ptr.read_recursive().get_dual_variable().is_zero())
                .collect();
            let decoding_graph = &interface_ptr.read_recursive().decoding_graph;
            let cluster_mut = &mut *cluster; // must first get mutable reference
            let plugin_manager = &mut cluster_mut.plugin_manager;
            let matrix = &mut cluster_mut.matrix;
            plugin_manager.find_relaxer(decoding_graph, matrix, &positive_dual_variables)
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
                    self.nodes.push(primal_node_ptr);
                }

                dual_module.set_grow_rate(&dual_node_ptr, grow_rate.clone());
            }
            cluster.relaxer_optimizer.insert(relaxer);
            return false;
        }

        // TODO idea: plugins can suggest subgraph (ideally, a global maximum), if so, then it will adopt th
        // subgraph with minimum weight from all plugins as the starting point to do local minimum

        // find a local minimum (hopefully a global minimum)
        let interface = interface_ptr.read_recursive();
        let initializer = interface.decoding_graph.model_graph.initializer.as_ref();
        let weight_of = |edge_index: EdgeIndex| initializer.weighted_edges[edge_index].weight;
        cluster.subgraph = Some(cluster.matrix.get_solution_local_minimum(weight_of).expect("satisfiable"));
        true
    }

    /// analyze a cluster and return whether there exists an optimal solution (depending on optimization levels)
    #[allow(clippy::unnecessary_cast)]
    fn resolve_cluster_tune(
        &mut self,
        cluster_index: NodeIndex,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
        dual_node_deltas: &mut BTreeMap<OrderedDualNodePtr, Rational>,
    ) -> (bool, OptimizerResult) {
        let mut optimizer_result = OptimizerResult::default();
        let cluster_ptr = self.clusters[cluster_index as usize].clone();
        let mut cluster = cluster_ptr.write();
        if cluster.nodes.is_empty() {
            return (true, optimizer_result); // no longer a cluster, no need to handle
        }
        // update the matrix with new tight edges
        let cluster = &mut *cluster;
        for &edge_index in cluster.edges.iter() {
            cluster
                .matrix
                .update_edge_tightness(edge_index, dual_module.is_edge_tight_tune(edge_index));
        }

        // find an executable relaxer from the plugin manager
        let relaxer = {
            let positive_dual_variables: Vec<DualNodePtr> = cluster
                .nodes
                .iter()
                .map(|p| p.read_recursive().dual_node_ptr.clone())
                .filter(|dual_node_ptr| !dual_node_ptr.read_recursive().dual_variable_at_last_updated_time.is_zero())
                .collect();
            let decoding_graph = &interface_ptr.read_recursive().decoding_graph;
            let cluster_mut = &mut *cluster; // must first get mutable reference
            let plugin_manager = &mut cluster_mut.plugin_manager;
            let matrix = &mut cluster_mut.matrix;
            plugin_manager.find_relaxer(decoding_graph, matrix, &positive_dual_variables)
        };

        // if a relaxer is found, execute it and return
        if let Some(mut relaxer) = relaxer {
            #[cfg(feature = "float_lp")]
            // float_lp is enabled, optimizer really plays a role
            if cluster.relaxer_optimizer.should_optimize(&relaxer) {
                match &mut cluster.incr_solution {
                    Some(incr_solution) => {
                        println!("HERE");
                        let mut done = false;
                        let old_dvs: BTreeMap<Arc<InvalidSubgraph>, Rational> = cluster
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

                        let mut original_dual_variables_sum = incr_solution.current_dual_variables_sum;

                        let mut dual_variables = BTreeMap::default();
                        for (invalid_subgraph, dual_variable) in old_dvs.iter() {
                            if !incr_solution.partcipating_dual_variables.contains_key(invalid_subgraph) {
                                dual_variables.insert(invalid_subgraph.clone(), dual_variable.clone());
                                original_dual_variables_sum += dual_variable;
                            }
                        }
                        for invalid_subgraph in relaxer.get_direction().keys() {
                            if !dual_variables.contains_key(invalid_subgraph) {
                                dual_variables.insert(invalid_subgraph.clone(), Rational::zero());
                            }
                        }

                        let mut participating_dual_variables = dual_variables.clone();
                        participating_dual_variables.append(&mut old_dvs.clone());

                        let edge_free_weights: BTreeMap<EdgeIndex, Rational> = dual_variables
                            .keys()
                            .flat_map(|invalid_subgraph: &Arc<InvalidSubgraph>| invalid_subgraph.hair.iter().cloned())
                            .chain(
                                relaxer
                                    .get_direction()
                                    .keys()
                                    .flat_map(|invalid_subgraph| invalid_subgraph.hair.iter().cloned()),
                            )
                            .map(|edge_index| {
                                (
                                    edge_index,
                                    dual_module.get_edge_free_weight(edge_index, &participating_dual_variables),
                                )
                            })
                            .collect();

                        let mut model: Model = Arc::<Option<SolvedModel>>::get_mut(&mut incr_solution.solution)
                            .unwrap()
                            .take()
                            .unwrap()
                            .into();

                        let mut x_vars = vec![];
                        let mut og_dv = vec![];
                        let mut invalid_subgraphs = Vec::with_capacity(dual_variables.len());
                        let mut edge_contributor: BTreeMap<EdgeIndex, Vec<usize>> =
                            edge_free_weights.keys().map(|&edge_index| (edge_index, vec![])).collect();

                        for (var_index, (invalid_subgraph, dual_variable)) in dual_variables.iter().enumerate() {
                            og_dv.push(dual_variable.clone());
                            // constraint of the dual variable >= 0
                            let x = model.add_col(1.0, 0.0.., []);
                            x_vars.push(x);

                            // constraint for xs ys <= dual_variable
                            invalid_subgraphs.push(invalid_subgraph.clone());

                            for &edge_index in invalid_subgraph.hair.iter() {
                                edge_contributor.get_mut(&edge_index).unwrap().push(var_index);
                            }
                        }

                        for (&edge_index, &weight) in edge_free_weights.iter() {
                            let mut row_entries = vec![];
                            for &var_index in edge_contributor[&edge_index].iter() {
                                row_entries.push((x_vars[var_index], 1.0));
                            }
                            model.add_row(..=weight.to_f64().unwrap(), row_entries);
                        }

                        let solved = model.solve();
                        let mut direction: BTreeMap<Arc<InvalidSubgraph>, Rational> = BTreeMap::new();
                        if solved.status() == HighsModelStatus::Optimal {
                            let solution = solved.get_solution();

                            // calculate the objective function
                            let mut optimal_objective = Rational::zero();
                            let cols = solution.columns();
                            for i in 0..x_vars.len() {
                                optimal_objective += Rational::new(cols[i]);
                            }

                            let delta = &optimal_objective - &original_dual_variables_sum;
                            // println!("optimal_objective: {:?}", delta);

                            // check positivity of the objective
                            if !(delta.is_positive()) {
                                // println!("delta: {:?}", delta);
                                done = true;
                                optimizer_result = OptimizerResult::EarlyReturned;
                                cluster.incr_solution = None;

                                // return (relaxer, true);
                            } else {
                                for (var_index, (invalid_subgraph, _)) in dual_variables.into_iter().enumerate() {
                                    let desired_amount = Rational::from(cols[var_index]);
                                    // println!("desired_amount: {:?}", desired_amount);
                                    let overall_growth = desired_amount - og_dv[var_index].clone();
                                    if !overall_growth.is_zero() {
                                        direction.insert(invalid_subgraph, Rational::from(overall_growth));
                                    }
                                }
                                cluster.incr_solution = Some(IncrLPSolution {
                                    current_dual_variables_sum: optimal_objective,
                                    partcipating_dual_variables: participating_dual_variables,
                                    solution: Arc::new(Some(solved)),
                                });
                            }
                        } else {
                            println!("solved status: {:?}", solved.status());
                            unreachable!();
                        }
                        if !done {
                            cluster.relaxer_optimizer.relaxers.insert(relaxer);
                            optimizer_result = OptimizerResult::Optimized;
                            relaxer = Relaxer::new(direction)
                        }
                    }
                    None => {
                        let mut dual_variables = BTreeMap::default();
                        let mut original_dual_variables_sum = Rational::zero();
                        for node in cluster.nodes.iter() {
                            let primal_node = node.read_recursive();
                            let dual_node = primal_node.dual_node_ptr.read_recursive();
                            original_dual_variables_sum += &dual_node.dual_variable_at_last_updated_time;
                            dual_variables.insert(
                                dual_node.invalid_subgraph.clone(),
                                dual_node.dual_variable_at_last_updated_time.clone(),
                            );
                        }

                        let edge_free_weights: BTreeMap<EdgeIndex, Rational> = dual_variables
                            .keys()
                            .flat_map(|invalid_subgraph: &Arc<InvalidSubgraph>| invalid_subgraph.hair.iter().cloned())
                            .chain(
                                relaxer
                                    .get_direction()
                                    .keys()
                                    .flat_map(|invalid_subgraph| invalid_subgraph.hair.iter().cloned()),
                            )
                            .map(|edge_index| (edge_index, dual_module.get_edge_free_weight(edge_index, &dual_variables)))
                            .collect();

                        let (new_relaxer, early_returned) = cluster.relaxer_optimizer.optimize_incr(
                            relaxer,
                            edge_free_weights,
                            dual_variables,
                            original_dual_variables_sum,
                            &mut cluster.incr_solution,
                        );
                        relaxer = new_relaxer;
                        if early_returned {
                            optimizer_result = OptimizerResult::EarlyReturned;
                        } else {
                            optimizer_result = OptimizerResult::Optimized;
                        }
                    }
                }

                // let edge_slacks: BTreeMap<EdgeIndex, Rational> = dual_variables
                //     .keys()
                //     .flat_map(|invalid_subgraph: &Arc<InvalidSubgraph>| invalid_subgraph.hair.iter().cloned())
                //     .chain(
                //         relaxer
                //             .get_direction()
                //             .keys()
                //             .flat_map(|invalid_subgraph| invalid_subgraph.hair.iter().cloned()),
                //     )
                //     .map(|edge_index| (edge_index, dual_module.get_edge_slack_tune(edge_index)))
                //     .collect();
                // let (new_relaxer, early_returned) = cluster.relaxer_optimizer.optimize(relaxer, edge_slacks, dual_variables);
            } else {
                optimizer_result = OptimizerResult::Skipped;
            }

            // #[cfg(not(feature = "float_lp"))]
            // // with rationals, it is actually usually better when always optimized
            // {
            //     let mut dual_variables = BTreeMap::default();
            //     let mut original_dual_variables_sum = Rational::zero();
            //     for node in cluster.nodes.iter() {
            //         let primal_node = node.read_recursive();
            //         let dual_node = primal_node.dual_node_ptr.read_recursive();
            //         original_dual_variables_sum += &dual_node.dual_variable_at_last_updated_time;
            //         dual_variables.insert(
            //             dual_node.invalid_subgraph.clone(),
            //             dual_node.dual_variable_at_last_updated_time.clone(),
            //         );
            //     }

            //     let edge_free_weights: BTreeMap<EdgeIndex, Rational> = dual_variables
            //         .keys()
            //         .flat_map(|invalid_subgraph: &Arc<InvalidSubgraph>| invalid_subgraph.hair.iter().cloned())
            //         .chain(
            //             relaxer
            //                 .get_direction()
            //                 .keys()
            //                 .flat_map(|invalid_subgraph| invalid_subgraph.hair.iter().cloned()),
            //         )
            //         .map(|edge_index| (edge_index, dual_module.get_edge_free_weight(edge_index, &dual_variables)))
            //         .collect();
            //     let (new_relaxer, early_returned) = cluster.relaxer_optimizer.optimize_incr(
            //         relaxer,
            //         edge_free_weights,
            //         dual_variables,
            //         original_dual_variables_sum,
            //     );

            //     // let edge_slacks: BTreeMap<EdgeIndex, Rational> = dual_variables
            //     //     .keys()
            //     //     .flat_map(|invalid_subgraph: &Arc<InvalidSubgraph>| invalid_subgraph.hair.iter().cloned())
            //     //     .chain(
            //     //         relaxer
            //     //             .get_direction()
            //     //             .keys()
            //     //             .flat_map(|invalid_subgraph| invalid_subgraph.hair.iter().cloned()),
            //     //     )
            //     //     .map(|edge_index| (edge_index, dual_module.get_edge_slack_tune(edge_index)))
            //     //     .collect();
            //     // let (new_relaxer, early_returned) = cluster.relaxer_optimizer.optimize(relaxer, edge_slacks, dual_variables);

            //     relaxer = new_relaxer;
            //     if early_returned {
            //         optimizer_result = OptimizerResult::EarlyReturned;
            //     } else {
            //         optimizer_result = OptimizerResult::Optimized;
            //     }
            // }

            for (invalid_subgraph, grow_rate) in relaxer.get_direction() {
                let (existing, dual_node_ptr) = interface_ptr.find_or_create_node_tune(invalid_subgraph, dual_module);
                if !existing {
                    // create the corresponding primal node and add it to cluster
                    let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                        dual_node_ptr: dual_node_ptr.clone(),
                        cluster_weak: cluster_ptr.downgrade(),
                    });
                    cluster.nodes.push(primal_node_ptr.clone());
                    self.nodes.push(primal_node_ptr);
                }

                // Document the desired deltas
                let index = dual_node_ptr.read_recursive().index;

                // println!(
                //     "setting dual node [index: {:?}] to have grow rate: [{:?}]\n\tresult will be: [{:?}]",
                //     index,
                //     grow_rate.clone(),
                //     dual_node_ptr.read_recursive().dual_variable_at_last_updated_time + grow_rate.clone()
                // );

                dual_node_deltas.insert(OrderedDualNodePtr::new(index, dual_node_ptr), grow_rate.clone());
            }

            cluster.relaxer_optimizer.insert(relaxer);
            return (false, optimizer_result);
        }

        // find a local minimum (hopefully a global minimum)
        let interface = interface_ptr.read_recursive();
        let initializer = interface.decoding_graph.model_graph.initializer.as_ref();
        let weight_of = |edge_index: EdgeIndex| initializer.weighted_edges[edge_index].weight;
        cluster.subgraph = Some(cluster.matrix.get_solution_local_minimum(weight_of).expect("satisfiable"));

        (true, optimizer_result)
    }

    /// update the sorted clusters_aff, should be None to start with
    fn update_sorted_clusters_aff<D: DualModuleImpl>(&mut self, dual_module: &mut D) {
        let pending_clusters = self.pending_clusters();
        let mut sorted_clusters_aff = BTreeSet::default();

        for cluster_index in pending_clusters.iter() {
            let cluster_ptr = self.clusters[*cluster_index].clone();
            let affinity = dual_module.calculate_cluster_affinity(cluster_ptr);
            if let Some(affinity) = affinity {
                sorted_clusters_aff.insert(ClusterAffinity {
                    cluster_index: *cluster_index,
                    affinity,
                });
            }
        }
        self.sorted_clusters_aff = Some(sorted_clusters_aff);
    }

    /// consume the sorted_clusters_aff
    fn get_sorted_clusters_aff(&mut self) -> BTreeSet<ClusterAffinity> {
        self.sorted_clusters_aff.take().unwrap()
    }
}

impl PrimalModuleSerial {
    // union the cluster of two dual nodes
    #[allow(clippy::unnecessary_cast)]
    pub fn union(&self, dual_node_ptr_1: &DualNodePtr, dual_node_ptr_2: &DualNodePtr, decoding_graph: &DecodingHyperGraph) {
        let node_index_1 = dual_node_ptr_1.read_recursive().index;
        let node_index_2 = dual_node_ptr_2.read_recursive().index;
        let primal_node_1 = self.nodes[node_index_1 as usize].read_recursive();
        let primal_node_2 = self.nodes[node_index_2 as usize].read_recursive();
        if primal_node_1.cluster_weak.ptr_eq(&primal_node_2.cluster_weak) {
            return; // already in the same cluster
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
        cluster_1.edges.append(&mut cluster_2.edges);
        cluster_1.subgraph = None; // mark as no subgraph
        for &vertex_index in cluster_2.vertices.iter() {
            if !cluster_1.vertices.contains(&vertex_index) {
                cluster_1.vertices.insert(vertex_index);
                let incident_edges = decoding_graph.get_vertex_neighbors(vertex_index);
                let parity = decoding_graph.is_vertex_defect(vertex_index);
                cluster_1.matrix.add_constraint(vertex_index, incident_edges, parity);
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
        let mut active_clusters = BTreeSet::<NodeIndex>::new();
        let interface = interface_ptr.read_recursive();
        let decoding_graph = &interface.decoding_graph;
        while let Some(conflict) = group_max_update_length.pop() {
            match conflict {
                MaxUpdateLength::Conflicting(edge_index) => {
                    // union all the dual nodes in the edge index and create new dual node by adding this edge to `internal_edges`
                    let dual_nodes = dual_module.get_edge_nodes(edge_index);
                    debug_assert!(
                        !dual_nodes.is_empty(),
                        "should not conflict if no dual nodes are contributing"
                    );
                    let dual_node_ptr_0 = &dual_nodes[0];
                    // first union all the dual nodes
                    for dual_node_ptr in dual_nodes.iter().skip(1) {
                        self.union(dual_node_ptr_0, dual_node_ptr, &interface.decoding_graph);
                    }
                    let cluster_ptr = self.nodes[dual_node_ptr_0.read_recursive().index as usize]
                        .read_recursive()
                        .cluster_weak
                        .upgrade_force();
                    let mut cluster = cluster_ptr.write();
                    // then add new constraints because these edges may touch new vertices
                    let incident_vertices = decoding_graph.get_edge_neighbors(edge_index);
                    for &vertex_index in incident_vertices.iter() {
                        if !cluster.vertices.contains(&vertex_index) {
                            cluster.vertices.insert(vertex_index);
                            let incident_edges = decoding_graph.get_vertex_neighbors(vertex_index);
                            let parity = decoding_graph.is_vertex_defect(vertex_index);
                            cluster.matrix.add_constraint(vertex_index, incident_edges, parity);
                        }
                    }
                    cluster.edges.insert(edge_index);
                    // add to active cluster so that it's processed later
                    active_clusters.insert(cluster.cluster_index);
                }
                MaxUpdateLength::ShrinkProhibited(dual_node_ptr) => {
                    let cluster_ptr = self.nodes[dual_node_ptr.index as usize]
                        .read_recursive()
                        .cluster_weak
                        .upgrade_force();
                    let cluster_index = cluster_ptr.read_recursive().cluster_index;
                    active_clusters.insert(cluster_index);
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
        for &cluster_index in active_clusters.iter() {
            let solved = self.resolve_cluster(cluster_index, interface_ptr, dual_module);
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
        let mut active_clusters = BTreeSet::<NodeIndex>::new();
        let interface = interface_ptr.read_recursive();
        let decoding_graph = &interface.decoding_graph;
        while let Some(conflict) = group_max_update_length.pop() {
            match conflict {
                MaxUpdateLength::Conflicting(edge_index) => {
                    // union all the dual nodes in the edge index and create new dual node by adding this edge to `internal_edges`
                    let dual_nodes = dual_module.get_edge_nodes(edge_index);
                    debug_assert!(
                        !dual_nodes.is_empty(),
                        "should not conflict if no dual nodes are contributing"
                    );
                    let dual_node_ptr_0 = &dual_nodes[0];
                    // first union all the dual nodes
                    for dual_node_ptr in dual_nodes.iter().skip(1) {
                        self.union(dual_node_ptr_0, dual_node_ptr, &interface.decoding_graph);
                    }
                    let cluster_ptr = self.nodes[dual_node_ptr_0.read_recursive().index as usize]
                        .read_recursive()
                        .cluster_weak
                        .upgrade_force();
                    let mut cluster = cluster_ptr.write();
                    // then add new constraints because these edges may touch new vertices
                    let incident_vertices = decoding_graph.get_edge_neighbors(edge_index);
                    for &vertex_index in incident_vertices.iter() {
                        if !cluster.vertices.contains(&vertex_index) {
                            cluster.vertices.insert(vertex_index);
                            let incident_edges = decoding_graph.get_vertex_neighbors(vertex_index);
                            let parity = decoding_graph.is_vertex_defect(vertex_index);
                            cluster.matrix.add_constraint(vertex_index, incident_edges, parity);
                        }
                    }
                    cluster.edges.insert(edge_index);
                    // add to active cluster so that it's processed later
                    active_clusters.insert(cluster.cluster_index);
                }
                MaxUpdateLength::ShrinkProhibited(dual_node_ptr) => {
                    let cluster_ptr = self.nodes[dual_node_ptr.index as usize]
                        .read_recursive()
                        .cluster_weak
                        .upgrade_force();
                    let cluster_index = cluster_ptr.read_recursive().cluster_index;
                    active_clusters.insert(cluster_index);
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
        for &cluster_index in active_clusters.iter() {
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
                let solved = self.resolve_cluster(cluster_index, interface_ptr, dual_module);
                if !solved {
                    return false; // let the dual module to handle one
                }
            }
            if *self.plugin_count.read_recursive() < self.plugins.len() {
                // increment the plugin count
                *self.plugin_count.write() += 1;
                self.plugin_pending_clusters = (0..self.clusters.len()).collect();
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
        let mut active_clusters = BTreeSet::<NodeIndex>::new();
        let interface = interface_ptr.read_recursive();
        let decoding_graph = &interface.decoding_graph;
        for conflict in group_max_update_length.into_iter() {
            match conflict {
                MaxUpdateLength::Conflicting(edge_index) => {
                    // union all the dual nodes in the edge index and create new dual node by adding this edge to `internal_edges`
                    let dual_nodes = dual_module.get_edge_nodes(edge_index);
                    debug_assert!(
                        !dual_nodes.is_empty(),
                        "should not conflict if no dual nodes are contributing"
                    );
                    let dual_node_ptr_0 = &dual_nodes[0];
                    // first union all the dual nodes
                    for dual_node_ptr in dual_nodes.iter().skip(1) {
                        self.union(dual_node_ptr_0, dual_node_ptr, &interface.decoding_graph);
                    }
                    let cluster_ptr = self.nodes[dual_node_ptr_0.read_recursive().index as usize]
                        .read_recursive()
                        .cluster_weak
                        .upgrade_force();
                    let mut cluster = cluster_ptr.write();
                    // then add new constraints because these edges may touch new vertices
                    let incident_vertices = decoding_graph.get_edge_neighbors(edge_index);
                    for &vertex_index in incident_vertices.iter() {
                        if !cluster.vertices.contains(&vertex_index) {
                            cluster.vertices.insert(vertex_index);
                            let incident_edges = decoding_graph.get_vertex_neighbors(vertex_index);
                            let parity = decoding_graph.is_vertex_defect(vertex_index);
                            cluster.matrix.add_constraint(vertex_index, incident_edges, parity);
                        }
                    }
                    cluster.edges.insert(edge_index);
                    // add to active cluster so that it's processed later
                    active_clusters.insert(cluster.cluster_index);
                }
                MaxUpdateLength::ShrinkProhibited(dual_node_ptr) => {
                    let cluster_ptr = self.nodes[dual_node_ptr.index as usize]
                        .read_recursive()
                        .cluster_weak
                        .upgrade_force();
                    let cluster_index = cluster_ptr.read_recursive().cluster_index;
                    active_clusters.insert(cluster_index);
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
        for &cluster_index in active_clusters.iter() {
            let (solved, other) =
                self.resolve_cluster_tune(cluster_index, interface_ptr, dual_module, &mut dual_node_deltas);
            all_solved &= solved;
            optimizer_result.or(other);
        }

        // println!("optimizer_result: {:?}", optimizer_result);

        let all_conflicts = dual_module.get_conflicts_tune(optimizer_result, dual_node_deltas);

        (all_conflicts, all_solved)
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
        mut dual_module: impl DualModuleImpl + MWPSVisualizer,
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
        let interface_ptr = DualModuleInterfacePtr::new(decoding_graph.model_graph.clone());
        primal_module.solve_visualizer(
            &interface_ptr,
            decoding_graph.syndrome_pattern.clone(),
            &mut dual_module,
            visualizer.as_mut(),
        );

        let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module, 0);
        if let Some(visualizer) = visualizer.as_mut() {
            visualizer
                .snapshot_combined(
                    "subgraph".to_string(),
                    vec![&interface_ptr, &dual_module, &subgraph, &weight_range],
                )
                .unwrap();
        }
        assert!(
            decoding_graph
                .model_graph
                .matches_subgraph_syndrome(&subgraph, &defect_vertices),
            "the result subgraph is invalid"
        );
        assert_eq!(
            Rational::from_usize(final_dual).unwrap(),
            weight_range.upper,
            "unmatched sum dual variables"
        );
        assert_eq!(
            Rational::from_usize(final_dual).unwrap(),
            weight_range.lower,
            "unexpected final dual variable sum"
        );
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
        primal_module_serial_basic_standard_syndrome_optional_viz(
            code,
            defect_vertices,
            final_dual,
            plugins,
            growing_strategy,
            DualModuleSerial::new_empty(&model_graph.initializer),
            model_graph,
            Some(visualizer),
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
            Some(visualizer),
        )
    }

    /// test a simple case
    #[test]
    fn primal_module_serial_basic_1_m() {
        // cargo test primal_module_serial_basic_1_m -- --nocapture
        let visualize_filename = "primal_module_serial_basic_1_m.json".to_string();
        let defect_vertices = vec![23, 24, 29, 30];
        let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            1,
            vec![],
            GrowingStrategy::ModeBased,
        );
    }

    #[test]
    fn primal_module_serial_basic_1_with_dual_pq_impl_m() {
        // cargo test primal_module_serial_basic_1_with_dual_pq_impl_m -- --nocapture
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
}
