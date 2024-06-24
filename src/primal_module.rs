//! Primal Module
//!
//! Generics for primal modules, defining the necessary interfaces for a primal module
//!

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::dual_module::*;
use crate::num_traits::{FromPrimitive, Signed, Zero};
use crate::pointers::*;
use crate::relaxer_optimizer::OptimizerResult;
use crate::util::*;
use crate::visualize::*;

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
            // Note: intersting, seems these aren't needed... But just kept here in case of future need, as well as correctness related failures
            if start {
                start = false;
                dual_module.advance_mode();
            }
            for cluster_index in self.pending_clusters() {
                let mut dual_node_deltas = BTreeMap::new();
                let mut conflicts = BTreeSet::new();
                let (mut resolved, optimizer_result) =
                    self.resolve_cluster_tune(cluster_index, interface, dual_module, &mut dual_node_deltas);

                match optimizer_result {
                    OptimizerResult::EarlyReturned => {
                        // optimizer early returned, don't update the states but check for if there is already going to be a conflict
                        for (dual_node_ptr, grow_rate) in dual_node_deltas.into_iter() {
                            // insert conflicts accordingly
                            let node_ptr_read = dual_node_ptr.ptr.read_recursive();
                            if grow_rate.is_negative() && node_ptr_read.dual_variable_at_last_updated_time.is_zero() {
                                conflicts.insert(MaxUpdateLength::ShrinkProhibited(OrderedDualNodePtr::new(
                                    node_ptr_read.index,
                                    dual_node_ptr.ptr.clone(),
                                )));
                            }
                            for edge_index in node_ptr_read.invalid_subgraph.hair.iter() {
                                if grow_rate.is_positive() && dual_module.is_edge_tight_tune(*edge_index) {
                                    conflicts.insert(MaxUpdateLength::Conflicting(*edge_index));
                                }
                            }
                        }
                    }
                    OptimizerResult::Skipped => {
                        // optimizer is skipped, meaning there is only a single direction to be grown, calculate the actual grow rate and grow
                        for (dual_node_ptr, grow_rate) in dual_node_deltas.into_iter() {
                            // calculate the actual grow rate
                            let mut actual_grow_rate = Rational::from_usize(std::usize::MAX).unwrap();
                            let node_ptr_read = dual_node_ptr.ptr.read_recursive();
                            for edge_index in node_ptr_read.invalid_subgraph.hair.iter() {
                                actual_grow_rate =
                                    std::cmp::min(actual_grow_rate, dual_module.get_edge_slack_tune(*edge_index));
                            }

                            // if grow_rate is zero, conflicts must have occured, and return conflicts
                            if actual_grow_rate.is_zero() {
                                for edge_index in node_ptr_read.invalid_subgraph.hair.iter() {
                                    if grow_rate.is_positive() && dual_module.is_edge_tight_tune(*edge_index) {
                                        conflicts.insert(MaxUpdateLength::Conflicting(*edge_index));
                                    }
                                }
                                if grow_rate.is_negative() && node_ptr_read.dual_variable_at_last_updated_time.is_zero() {
                                    conflicts.insert(MaxUpdateLength::ShrinkProhibited(OrderedDualNodePtr::new(
                                        node_ptr_read.index,
                                        dual_node_ptr.ptr.clone(),
                                    )));
                                }
                            } else {
                                drop(node_ptr_read);
                                let mut node_ptr_write = dual_node_ptr.ptr.write();
                                // update with the actual grow rate, both edges and dual nodes
                                for edge_index in node_ptr_write.invalid_subgraph.hair.iter() {
                                    dual_module.grow_edge(*edge_index, &actual_grow_rate);
                                    if actual_grow_rate.is_positive() && dual_module.is_edge_tight_tune(*edge_index) {
                                        conflicts.insert(MaxUpdateLength::Conflicting(*edge_index));
                                    }
                                }
                                node_ptr_write.dual_variable_at_last_updated_time += actual_grow_rate.clone();
                                if actual_grow_rate.is_negative()
                                    && node_ptr_write.dual_variable_at_last_updated_time.is_zero()
                                {
                                    conflicts.insert(MaxUpdateLength::ShrinkProhibited(OrderedDualNodePtr::new(
                                        node_ptr_write.index,
                                        dual_node_ptr.ptr.clone(),
                                    )));
                                }
                            }
                        }
                    }
                    _ => {
                        // otherwise, just grow following the optimizer resulting direction
                        for (dual_node_ptr, grow_rate) in dual_node_deltas.into_iter() {
                            let mut node_ptr_write = dual_node_ptr.ptr.write();

                            // grow the dual nodes and the associated edges
                            node_ptr_write.dual_variable_at_last_updated_time += grow_rate.clone();
                            if grow_rate.is_negative() && node_ptr_write.dual_variable_at_last_updated_time.is_zero() {
                                conflicts.insert(MaxUpdateLength::ShrinkProhibited(OrderedDualNodePtr::new(
                                    node_ptr_write.index,
                                    dual_node_ptr.ptr.clone(),
                                )));
                            }
                            for edge_index in node_ptr_write.invalid_subgraph.hair.iter() {
                                dual_module.grow_edge(*edge_index, &grow_rate);
                                if grow_rate.is_positive() && dual_module.is_edge_tight_tune(*edge_index) {
                                    conflicts.insert(MaxUpdateLength::Conflicting(*edge_index));
                                }
                            }
                        }
                    }
                }

                while !resolved {
                    let (_conflicts, _resolved) = self.resolve_tune(conflicts, interface, dual_module);
                    if _resolved {
                        break;
                    }
                    conflicts = _conflicts;
                    resolved = _resolved;
                }
            }
        }
    }

    fn subgraph(&mut self, interface: &DualModuleInterfacePtr, dual_module: &mut impl DualModuleImpl, seed: u64)
        -> Subgraph;

    fn subgraph_range(
        &mut self,
        interface: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
        seed: u64,
    ) -> (Subgraph, WeightRange) {
        let subgraph = self.subgraph(interface, dual_module, seed);
        let weight_range = WeightRange::new(
            interface.sum_dual_variables(),
            Rational::from_usize(
                interface
                    .read_recursive()
                    .decoding_graph
                    .model_graph
                    .initializer
                    .get_subgraph_total_weight(&subgraph),
            )
            .unwrap(),
        );
        (subgraph, weight_range)
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
        _dual_node_deltas: &mut BTreeMap<OrderedDualNodePtr, Rational>,
    ) -> (bool, OptimizerResult) {
        panic!("not implemented `resolve_cluster_tune`");
    }
}
