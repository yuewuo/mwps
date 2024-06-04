//! Primal Module
//!
//! Generics for primal modules, defining the necessary interfaces for a primal module
//!

use crate::dual_module::*;
use crate::num_traits::{FromPrimitive, One};
use crate::pointers::*;
use crate::util::*;
use crate::visualize::*;
use std::collections::BTreeSet;
use std::sync::Arc;

/// common trait that must be implemented for each implementation of primal module
pub trait PrimalModuleImpl {
    fn get_grow_rate(&self, cluster_index: NodeIndex, dual_node_ptr: &DualNodePtr) -> Rational {
        panic!("not implemented lol");
    }
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
    fn resolve(
        &mut self,
        group_max_update_length: GroupMaxUpdateLength,
        interface: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool;

    fn old_resolve(
        &mut self,
        group_max_update_length: GroupMaxUpdateLength,
        interface: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool {
        false
    }

    fn resolve_tune(
        &mut self,
        group_max_update_length: BTreeSet<MaxUpdateLength>,
        interface: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> (BTreeSet<EdgeIndex>, BTreeSet<DualNodePtr>, bool) {
        panic!("not implemented")
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
        // Search
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

        // println!("start");
        // dual_module.debug_print();
        // dual_module.sync();
        // return;

        // from here, all things should be syncronized
        let mut start = true;
        // We know that things are in an unbounded state here: All edges and nodes are not growing as of now
        // Tune
        while self.has_more_plugins() {
            if start {
                start = false;
                dual_module.advance_mode();
            }
            for cluster_index in self.pending_clusters() {
                let (_impacted_edges, _impacted_nodes, _resolved) =
                    self.resolve_cluster_tune(cluster_index, interface, dual_module);
                if !_resolved {
                    // check if any of the current moves will be conflict

                    // if not, we grow once

                    // what about now?
                    let mut conflicts = dual_module.get_current_conflicts(&_impacted_edges, &_impacted_nodes);
                    if conflicts.is_empty() {
                        dual_module.grow(Rational::one());
                        // dual_module.debug_print();
                        dual_module.sync(); // FIXME: should jsut set to the optimize value, so don't need to care about time any more
                                            // println!("grew one 0");
                        conflicts = dual_module.get_current_conflicts(&_impacted_edges, &_impacted_nodes);
                        // dual_module.set_nodes_to_zero();
                    } else {
                        // dual_module.set_nodes_to_zero();
                        println!("AAAAAAAAA need to resolve again");
                    }

                    loop {
                        // dual_module.debug_print();

                        let (impacted_edges, impacted_nodes, resolved) =
                            self.resolve_tune(conflicts, interface, dual_module);
                        // dual_module.set_nodes_to_zero();
                        conflicts = dual_module.get_current_conflicts(&impacted_edges, &impacted_nodes);
                        // println!("_conflicts: {:?}", conflicts);
                        if resolved {
                            // println!("_resolved");
                            break;
                        }
                        if !conflicts.is_empty() {
                            println!("BBBBBB need to resolve again");
                            // println!("conflicts: {:?}", conflicts);
                            continue;
                        }
                        if conflicts.is_empty() {
                            dual_module.grow(Rational::one());
                            dual_module.sync();
                            // println!("grew one 3");
                            // dual_module.debug_print();
                        }
                        conflicts = dual_module.get_current_conflicts(&impacted_edges, &impacted_nodes);
                        // println!("conflicts: {:?}", conflicts);
                        // dual_module.set_nodes_to_zero();
                        // println!("jacking off");
                    }
                }
                // println!("done");
                // dual_module.set_nodes_to_zero();
            }
        }
    }

    fn subgraph(&mut self, interface: &DualModuleInterfacePtr, dual_module: &mut impl DualModuleImpl) -> Subgraph;

    fn subgraph_range(
        &mut self,
        interface: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> (Subgraph, WeightRange) {
        let subgraph = self.subgraph(interface, dual_module);
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

    fn has_more_plugins(&mut self) -> bool {
        false
    }

    fn pending_clusters(&mut self) -> Vec<usize> {
        panic!("!!!");
    }

    fn is_solved<D: DualModuleImpl>(&mut self, interface_ptr: &DualModuleInterfacePtr, dual_module: &mut D) -> bool {
        false
    }

    fn resolve_cluster(
        &mut self,
        cluster_index: NodeIndex,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool {
        panic!("falskdj")
    }
    fn resolve_cluster_tune(
        &mut self,
        cluster_index: NodeIndex,
        interface_ptr: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> (BTreeSet<EdgeIndex>, BTreeSet<DualNodePtr>, bool) {
        panic!("falskdj")
    }
}
