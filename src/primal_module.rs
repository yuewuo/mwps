//! Primal Module
//!
//! Generics for primal modules, defining the necessary interfaces for a primal module
//!

use crate::dual_module::*;
use crate::num_traits::{FromPrimitive, One};
use crate::pointers::*;
use crate::util::*;
use crate::visualize::*;
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
        group_max_update_length: GroupMaxUpdateLength,
        interface: &DualModuleInterfacePtr,
        dual_module: &mut impl DualModuleImpl,
    ) -> bool {
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
        // let mut group_max_update_length = dual_module.compute_maximum_update_length();
        // while !group_max_update_length.is_unbounded() {
        //     callback(interface, dual_module, self, &group_max_update_length);
        //     if let Some(length) = group_max_update_length.get_valid_growth() {
        //         dual_module.grow(length);
        //     } else {
        //         self.resolve(group_max_update_length, interface, dual_module);
        //     }
        //     group_max_update_length = dual_module.compute_maximum_update_length();
        // }

        // Search
        let mut resolved = false;
        let mut group_max_update_length = dual_module.compute_maximum_update_length();
        while !group_max_update_length.is_unbounded() {
            callback(interface, dual_module, self, &group_max_update_length);
            if resolved {
                // println!("group_max_update_length: {:?}", group_max_update_length);
            }
            if let Some(length) = group_max_update_length.get_valid_growth() {
                if resolved {
                    // println!("resolved; growing: {:?}", length);
                }
                dual_module.grow(length);
            } else if self.resolve(group_max_update_length, interface, dual_module) {
                // println!("ADVANCING MODE: {:?}", dual_module.mode());
                // dual_module.advance_mode();
                // dual_module.grow(Rational::one());
                // moving onto the tuning mode
                // if resolved {
                //     println!("double resolved")
                // } else {
                //     println!("RESOLVED");
                // }
                resolved = true;
                // let length = dual_module.compute_maximum_update_length().get_valid_growth().unwrap();
                // dual_module.grow(length);
                // dual_module.grow(Rational::one());
                // break;
            } else {
                // println!("failed to resolve");
                if resolved {
                    // println!("turned back being unresolved");
                }
            }
            group_max_update_length = dual_module.compute_maximum_update_length();
            if group_max_update_length.is_unbounded() {
                // println!("UNBOUNDED");
            } else {
                if resolved {
                    // println!("another iteration");
                }
            }
        }

        let resolve = 0;
        let grow = 1;
        let mut start = true;
        // Tune
        while self.has_more_plugins() {
            for cluster_index in self.pending_clusters() {
                let mut new_start = true;
                if !self.resolve_cluster_tune(cluster_index, interface, dual_module) {
                    // println!("TUNING");

                    // let mut group_max_update_length = dual_module.compute_maximum_update_length();
                    // if group_max_update_length.is_unbounded() {
                    //     println!("UNBOUNDED123");
                    // } else {
                    //     // println!("\n135 group_max_update_length: {:?}", group_max_update_length);
                    // }
                    // let mut curr = 0;
                    // // dual_module.debug_print();

                    // while !group_max_update_length.is_unbounded() {
                    //     callback(interface, dual_module, self, &group_max_update_length);
                    //     if let Some(length) = group_max_update_length.get_valid_growth() {
                    //         // println!("\nTUNING GROW: {:?}", length);
                    //         // if resolved {
                    //         //     println!("RESOLVED: group_max_update_length: {:?}", group_max_update_length);
                    //         // }
                    //         if (length != Rational::one()) {
                    //             println!("wurf");
                    //             dual_module.debug_print();
                    //         }
                    //         dual_module.grow(length);
                    //         // println!("growing: is last operation resolve: {:?}", curr == resolve);
                    //         // println!("is this start: 1{:?}", start);
                    //         // println!("is this new start: 2{:?}", new_start);
                    //         curr = grow;
                    //         start = false;
                    //         new_start = false;
                    //     } else {
                    //         if self.resolve_tune(group_max_update_length, interface, dual_module) {
                    //             // println!("ADVANCING MODE: {:?}", dual_module.mode());
                    //             // dual_module.advance_mode();
                    //             // println!("last growth 1");
                    //             dual_module.grow(Rational::one());
                    //             // println!("\nBroken");
                    //             // moving onto the tuning mode
                    //             // group_max_update_length = dual_module.compute_maximum_update_length();
                    //             // println!("last operation is: {:?}", if curr == resolve { "resolve" } else { "grow" });
                    //             break;
                    //             // resolved = true;
                    //         }
                    //         // println!("resolving: is last operation grow: {:?}", curr == grow);
                    //         // println!("is this start: 1{:?}", start);
                    //         // println!("is this new start: 2{:?}", new_start);

                    //         start = false;

                    //         curr = resolve;
                    //         new_start = false;
                    //     }
                    //     group_max_update_length = dual_module.compute_maximum_update_length();
                    //     if group_max_update_length.is_unbounded() {
                    //         println!("UNBOUNDED456");
                    //     }
                    //     // dual_module.debug_print();
                    // }

                    let mut group_max_update_length = dual_module.compute_maximum_update_length();
                    if group_max_update_length.is_unbounded() {
                        continue;
                    }
                    if group_max_update_length.get_valid_growth().is_some() {
                        dual_module.grow(Rational::one());
                        group_max_update_length = dual_module.compute_maximum_update_length()
                    }
                    while !self.resolve_tune(group_max_update_length, interface, dual_module) {
                        self.resolve_tune(dual_module.compute_maximum_update_length(), interface, dual_module);
                        dual_module.grow(Rational::one());
                        group_max_update_length = dual_module.compute_maximum_update_length();
                        // println!("group_max_update_length after growth: {:?}", group_max_update_length);
                        // dual_module.compute_maximum_update_length();
                    }
                }
            }
        }

        // while self.has_more_plugins() {
        //     while !self.is_solved(interface, dual_module) {
        //         group_max_update_length = dual_module.compute_maximum_update_length();

        //         while !group_max_update_length.is_unbounded() {
        //             callback(interface, dual_module, self, &group_max_update_length);
        //             if let Some(length) = group_max_update_length.get_valid_growth() {
        //                 dual_module.grow(length);
        //             } else {
        //                 if self.resolve(group_max_update_length, interface, dual_module) {
        //                     break;
        //                 }
        //                 //     // eprintln!("RESOLVED!!!!!!");
        //                 //     break;
        //                 // }
        //             }
        //             group_max_update_length = dual_module.compute_maximum_update_length();
        //         }
        //     }
        // }
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
    ) -> bool {
        panic!("falskdj")
    }
}
