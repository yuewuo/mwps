//! Primal Module
//!
//! Generics for primal modules, defining the necessary interfaces for a primal module
//!

use crate::dual_module::*;
use crate::num_traits::{FromPrimitive, One, Signed};
use crate::pointers::*;
use crate::util::*;
use crate::visualize::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// common trait that must be implemented for each implementation of primal module
pub trait PrimalModuleImpl {
    fn get_grow_rate(&self, cluster_index: NodeIndex, dual_node_ptr: &DualNodePtr) -> Rational {
        panic!("not implemented lol");
    }

    fn set_zeros<D: DualModuleImpl>(&mut self, dual_module: &mut D) {
        panic!("not implemented lol 345");
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
    ) -> (BTreeSet<MaxUpdateLength>, bool) {
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

        // from here, all things should be syncronized
        let mut start = true;

        // We know that things are in an unbounded state here: All edges and nodes are not growing as of now
        // Tune
        while self.has_more_plugins() {
            if start {
                start = false;
                dual_module.advance_mode();
                self.set_zeros(dual_module);
            }
            for cluster_index in self.pending_clusters() {
                let mut edge_deltas = BTreeMap::new();
                let (mut conflicts, mut resolved) =
                    self.resolve_cluster_tune(cluster_index, interface, dual_module, &mut edge_deltas);
                for (edge_index, grow_rate) in edge_deltas.into_iter() {
                    dual_module.grow_edge(edge_index, &grow_rate);
                    if grow_rate.is_positive() && dual_module.is_edge_tight(edge_index) {
                        conflicts.insert(MaxUpdateLength::Conflicting(edge_index));
                    }
                }
                while !resolved {
                    let (_conflicts, _resolved) = self.resolve_tune(conflicts, interface, dual_module);
                    if resolved {
                        break;
                    }
                    conflicts = _conflicts;
                    resolved = _resolved;
                }
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

    /// check if there are more plugins to be applied, defaulted to having no plugins
    fn has_more_plugins(&mut self) -> bool {
        false
    }

    /// in "tune" mode, return the list of clusters that need to be resolved
    fn pending_clusters(&mut self) -> Vec<usize> {
        panic!("!!!");
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
        edge_deltas: &mut BTreeMap<EdgeIndex, Rational>,
    ) -> (BTreeSet<MaxUpdateLength>, bool) {
        panic!("falskdj")
    }
}
