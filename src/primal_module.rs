//! Primal Module
//!
//! Generics for primal modules, defining the necessary interfaces for a primal module
//!
#[cfg(feature = "cluster_size_limit")]
use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::dual_module::*;
use crate::num_traits::FromPrimitive;
use crate::ordered_float::OrderedFloat;
use crate::pointers::*;
use crate::primal_module_serial::ClusterAffinity;
use crate::relaxer_optimizer::OptimizerResult;
use crate::util::*;
use crate::visualize::*;

pub type Affinity = OrderedFloat;

#[cfg(feature = "cluster_size_limit")]
const MAX_HISTORY: usize = 10;

/// common trait that must be implemented for each implementation of primal module
pub trait PrimalModuleImpl {
    /// create a primal module given the dual module
    fn new_empty(solver_initializer: &SolverInitializer) -> Self;

    /// clear all states; however this method is not necessarily called when load a new decoding problem, so you need to call it yourself
    fn clear(&mut self);

    /// load a new decoding problem given dual interface: note that all nodes MUST be defect node
    fn load<D: DualModuleImpl>(&mut self, interface_ptr: &DualModuleInterfacePtr, dual_module: &mut D);

    /// analyze the reason why dual module cannot further grow, update primal data structure (alternating tree, temporary matches, etc)
    /// and then tell dual module what to do to resolve these conflicts;
    /// note that this function doesn't necessarily resolve all the conflicts, but can return early if some major change is made.
    /// when implementing this function, it's recommended that you resolve as many conflicts as possible.
    ///
    /// note: this is only ran in the "search" mode
    fn resolve(
        &mut self,
        group_max_update_length: GroupMaxUpdateLength,
        interface: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool;

    /// kept in case of future need for this deprecated function (backwards compatibility for cases such as `SingleCluster` growing strategy)
    fn old_resolve(
        &mut self,
        _group_max_update_length: GroupMaxUpdateLength,
        _interface: &DualModuleInterfacePtr,
        _dual_module: &mut impl DualModuleImpl,
    ) -> bool {
        false
    }

    /// resolve the conflicts in the "tune" mode
    fn resolve_tune(
        &mut self,
        _group_max_update_length: BTreeSet<MaxUpdateLength>,
        _interface: &DualModuleInterfacePtr,
        _dual_module: &mut impl DualModuleImpl,
    ) -> (BTreeSet<MaxUpdateLength>, bool) {
        panic!("`resolve_tune` not implemented, this primal module does not work with tuning mode");
    }

    fn solve(
        &mut self,
        interface: &DualModuleInterfacePtr,
        syndrome_pattern: Arc<SyndromePattern>,
        dual_module: &mut impl DualModuleImpl,
    ) {
        self.solve_step_callback(interface, syndrome_pattern, dual_module, |_, _, _, _| {})
    }

    fn solve_visualizer<D: DualModuleImpl + MWPSVisualizer>(
        &mut self,
        interface: &DualModuleInterfacePtr,
        syndrome_pattern: Arc<SyndromePattern>,
        dual_module: &mut D,
        visualizer: Option<&mut Visualizer>,
    ) where
        Self: MWPSVisualizer + Sized,
    {
        if let Some(visualizer) = visualizer {
            self.solve_step_callback(
                interface,
                syndrome_pattern,
                dual_module,
                |interface, dual_module, primal_module, group_max_update_length| {
                    if cfg!(debug_assertions) {
                        println!("group_max_update_length: {:?}", group_max_update_length);
                        // dual_module.debug_print();
                    }
                    if group_max_update_length.is_unbounded() {
                        visualizer
                            .snapshot_combined("unbounded grow".to_string(), vec![interface, dual_module, primal_module])
                            .unwrap();
                    } else if let Some(length) = group_max_update_length.get_valid_growth() {
                        visualizer
                            .snapshot_combined(format!("grow {length}"), vec![interface, dual_module, primal_module])
                            .unwrap();
                    } else {
                        let first_conflict = format!("{:?}", group_max_update_length.peek().unwrap());
                        visualizer
                            .snapshot_combined(
                                format!("resolve {first_conflict}"),
                                vec![interface, dual_module, primal_module],
                            )
                            .unwrap();
                    };
                },
            );
            visualizer
                .snapshot_combined("solved".to_string(), vec![interface, dual_module, self])
                .unwrap();
        } else {
            self.solve(interface, syndrome_pattern, dual_module);
        }
    }

    fn solve_step_callback<D: DualModuleImpl, F>(
        &mut self,
        interface: &DualModuleInterfacePtr,
        syndrome_pattern: Arc<SyndromePattern>,
        dual_module: &mut D,
        callback: F,
    ) where
        F: FnMut(&DualModuleInterfacePtr, &mut D, &mut Self, &GroupMaxUpdateLength),
    {
        // subgraph_set.into_iter().collect()
        interface.load(syndrome_pattern, dual_module);
        self.load(interface, dual_module);
        self.solve_step_callback_interface_loaded(interface, dual_module, callback);
    }

    fn solve_step_callback_interface_loaded<D: DualModuleImpl, F>(
        &mut self,
        interface: &DualModuleInterfacePtr,
        dual_module: &mut D,
        mut callback: F,
    ) where
        F: FnMut(&DualModuleInterfacePtr, &mut D, &mut Self, &GroupMaxUpdateLength),
    {
        // Search, this part is unchanged
        let mut group_max_update_length = dual_module.compute_maximum_update_length();

        while !group_max_update_length.is_unbounded() {
            callback(interface, dual_module, self, &group_max_update_length);
            match group_max_update_length.get_valid_growth() {
                Some(length) => dual_module.grow(length),
                None => {
                    self.resolve(group_max_update_length, interface, dual_module);
                }
            }
            group_max_update_length = dual_module.compute_maximum_update_length();
        }

        // from here, all states should be syncronized
        let mut start = true;

        // starting with unbounded state here: All edges and nodes are not growing as of now
        // Tune
        while self.has_more_plugins() {
            if start {
                start = false;
                dual_module.advance_mode();
            }
            self.update_sorted_clusters_aff(dual_module);
            let cluster_affs = self.get_sorted_clusters_aff();

            for cluster_affinity in cluster_affs.into_iter() {
                let cluster_index = cluster_affinity.cluster_index;
                let mut dual_node_deltas = BTreeMap::new();
                let (mut resolved, optimizer_result) =
                    self.resolve_cluster_tune(cluster_index, interface, dual_module, &mut dual_node_deltas);

                let mut conflicts = dual_module.get_conflicts_tune(optimizer_result, dual_node_deltas);

                // for cycle resolution
                #[cfg(feature = "cluster_size_limit")]
                let mut order: VecDeque<BTreeSet<MaxUpdateLength>> = VecDeque::with_capacity(MAX_HISTORY); // fifo order of the conflicts sets seen
                #[cfg(feature = "cluster_size_limit")]
                let mut current_sequences: Vec<(usize, BTreeSet<MaxUpdateLength>)> = Vec::new(); // the indexes that are currently being processed

                '_resolving: while !resolved {
                    let (_conflicts, _resolved) = self.resolve_tune(conflicts.clone(), interface, dual_module);

                    #[cfg(feature = "cluster_size_limit")]
                    {
                        // cycle resolution
                        let drained: Vec<(usize, BTreeSet<MaxUpdateLength>)> = std::mem::take(&mut current_sequences);
                        for (idx, start) in drained.into_iter() {
                            if _conflicts.eq(&start) {
                                dual_module.end_tuning();
                                break '_resolving;
                            }
                            if _conflicts.eq(order
                                .get(MAX_HISTORY - idx - 1)
                                .unwrap_or(order.get(order.len() - idx - 1).unwrap()))
                            {
                                current_sequences.push((idx + 1, start));
                            }
                        }

                        order.push_back(_conflicts.clone());
                        if order.len() > MAX_HISTORY {
                            order.pop_front();
                            current_sequences = current_sequences
                                .into_iter()
                                .filter_map(|(x, start)| if x >= MAX_HISTORY { None } else { Some((x + 1, start)) })
                                .collect();
                        }

                        for (idx, c) in order.iter().enumerate() {
                            if c.eq(&_conflicts) {
                                current_sequences.push((idx, c.clone()));
                            }
                        }
                    }

                    if _resolved {
                        dual_module.end_tuning();
                        break;
                    }

                    conflicts = _conflicts;
                    resolved = _resolved;
                }
            }
        }
    }

    fn subgraph(&mut self, interface: &DualModuleInterfacePtr, dual_module: &mut impl DualModuleImpl) -> OutputSubgraph;

    fn subgraph_range(
        &mut self,
        interface: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> (OutputSubgraph, WeightRange) {
        let output_subgraph = self.subgraph(interface, dual_module);
        let weight_range = WeightRange::new(
            interface.sum_dual_variables() + dual_module.get_negative_weight_sum(),
            Rational::from_usize(
                interface
                    .read_recursive()
                    .decoding_graph
                    .model_graph
                    .initializer
                    .get_subgraph_total_weight(&output_subgraph),
            )
            .unwrap()
                + dual_module.get_negative_weight_sum(), // this uses the initailizer, we would need to update this if were to keep this consistent
        );
        (output_subgraph, weight_range)
    }

    /// performance profiler report
    fn generate_profiler_report(&self) -> serde_json::Value {
        json!({})
    }

    /* tune mode methods */
    /// check if there are more plugins to be applied, defaulted to having no plugins
    fn has_more_plugins(&mut self) -> bool {
        false
    }

    /// in "tune" mode, return the list of clusters that need to be resolved
    fn pending_clusters(&mut self) -> Vec<usize> {
        panic!("not implemented `pending_clusters`");
    }

    /// check if a cluster has been solved, if not then resolve it
    fn resolve_cluster(
        &mut self,
        _cluster_index: NodeIndex,
        _interface_ptr: &DualModuleInterfacePtr,
        _dual_module: &mut impl DualModuleImpl,
    ) -> bool {
        panic!("not implemented `resolve_cluster`");
    }

    /// `resolve_cluster` but in tuning mode, optimizer result denotes what the optimizer has accomplished
    fn resolve_cluster_tune(
        &mut self,
        _cluster_index: NodeIndex,
        _interface_ptr: &DualModuleInterfacePtr,
        _dual_module: &mut impl DualModuleImpl,
        // _dual_node_deltas: &mut BTreeMap<OrderedDualNodePtr, Rational>,
        _dual_node_deltas: &mut BTreeMap<OrderedDualNodePtr, (Rational, NodeIndex)>,
    ) -> (bool, OptimizerResult) {
        panic!("not implemented `resolve_cluster_tune`");
    }

    /* affinity */

    /// calculate the affinity map of clusters and maintain an decreasing order of priority
    fn update_sorted_clusters_aff<D: DualModuleImpl>(&mut self, _dual_module: &mut D) {
        panic!("not implemented `update_sorted_clusters_aff`");
    }

    /// get the sorted clusters by affinity
    fn get_sorted_clusters_aff(&mut self) -> BTreeSet<ClusterAffinity> {
        panic!("not implemented `get_sorted_clusters_aff`");
    }

    #[cfg(feature = "incr_lp")]
    /// calculate the edges free weight map by cluster
    fn calculate_edges_free_weight_clusters(&mut self, _dual_module: &mut impl DualModuleImpl) {
        panic!("not implemented `calculate_edges_free_weight_clusters`");
    }

    /// unset the cluster_weight parameter
    #[cfg(feature = "incr_lp")]
    fn uninit_cluster_weight(&mut self) {}

    /// get the cluster_weight parameter
    #[cfg(feature = "incr_lp")]
    fn is_cluster_weight_initialized(&self) -> bool {
        true
    }
}
