//! Primal Module
//! 
//! Generics for primal modules, defining the necessary interfaces for a primal module
//!

use crate::util::*;
use crate::dual_module::*;
use crate::visualize::*;
use crate::pointers::*;
use crate::num_traits::FromPrimitive;


/// common trait that must be implemented for each implementation of primal module
pub trait PrimalModuleImpl {

    /// create a primal module given the dual module
    fn new_empty(solver_initializer: &SolverInitializer) -> Self;

    /// clear all states; however this method is not necessarily called when load a new decoding problem, so you need to call it yourself
    fn clear(&mut self);

    fn load_defect_dual_node<D: DualModuleImpl>(&mut self, dual_node_ptr: &DualNodePtr, dual_module: &mut D);

    /// indicate all syndrome have been loaded and called before resolving any defect
    fn begin_resolving<D: DualModuleImpl>(&mut self, interface_ptr: &DualModuleInterfacePtr, dual_module: &mut D);

    /// load a single syndrome and update the dual module and the interface
    fn load_defect<D: DualModuleImpl>(&mut self, defect_vertex: VertexIndex, interface_ptr: &DualModuleInterfacePtr, dual_module: &mut D) {
        interface_ptr.create_defect_node(defect_vertex, dual_module);
        let interface = interface_ptr.read_recursive();
        let index = interface.nodes.len() - 1;
        self.load_defect_dual_node(&interface.nodes[index], dual_module)
    }

    /// load a new decoding problem given dual interface: note that all nodes MUST be syndrome node
    fn load<D: DualModuleImpl>(&mut self, interface_ptr: &DualModuleInterfacePtr, dual_module: &mut D) {
        let interface = interface_ptr.read_recursive();
        for index in 0..interface.nodes.len() as NodeIndex {
            let node_ptr = &interface.nodes[index as usize];
            let node = node_ptr.read_recursive();
            debug_assert!(node.internal_edges.is_empty(), "must load a fresh dual module interface, found a complex node");
            debug_assert!(node.internal_vertices.len() == 1, "must load a fresh dual module interface, found invalid defect node");
            debug_assert_eq!(node.index, index, "must load a fresh dual module interface, found index out of order");
            self.load_defect_dual_node(node_ptr, dual_module);
        }
    }

    /// analyze the reason why dual module cannot further grow, update primal data structure (alternating tree, temporary matches, etc)
    /// and then tell dual module what to do to resolve these conflicts;
    /// note that this function doesn't necessarily resolve all the conflicts, but can return early if some major change is made.
    /// when implementing this function, it's recommended that you resolve as many conflicts as possible.
    fn resolve(&mut self, group_max_update_length: GroupMaxUpdateLength, interface: &DualModuleInterfacePtr, dual_module: &mut impl DualModuleImpl);

    fn solve(&mut self, interface: &DualModuleInterfacePtr, syndrome_pattern: &SyndromePattern, dual_module: &mut impl DualModuleImpl) {
        self.solve_step_callback(interface, syndrome_pattern, dual_module, |_, _, _, _| {})
    }

    fn solve_visualizer<D: DualModuleImpl + MWPSVisualizer>(&mut self, interface: &DualModuleInterfacePtr, syndrome_pattern: &SyndromePattern, dual_module: &mut D
            , visualizer: Option<&mut Visualizer>) where Self: MWPSVisualizer + Sized {
        if let Some(visualizer) = visualizer {
            self.solve_step_callback(interface, syndrome_pattern, dual_module, |interface, dual_module, primal_module, group_max_update_length| {
                if cfg!(debug_assertions) {
                    println!("group_max_update_length: {:?}", group_max_update_length);
                }
                if group_max_update_length.is_unbounded() {
                    visualizer.snapshot_combined(format!("unbounded grow"), vec![interface, dual_module, primal_module]).unwrap();
                } else if let Some(length) = group_max_update_length.get_valid_growth() {
                    visualizer.snapshot_combined(format!("grow {length}"), vec![interface, dual_module, primal_module]).unwrap();
                } else {
                    let first_conflict = format!("{:?}", group_max_update_length.peek().unwrap());
                    visualizer.snapshot_combined(format!("resolve {first_conflict}"), vec![interface, dual_module, primal_module]).unwrap();
                };
            });
            visualizer.snapshot_combined("solved".to_string(), vec![interface, dual_module, self]).unwrap();
        } else {
            self.solve(interface, syndrome_pattern, dual_module);
        }
    }

    fn solve_step_callback<D: DualModuleImpl, F>(&mut self, interface: &DualModuleInterfacePtr, syndrome_pattern: &SyndromePattern, dual_module: &mut D, callback: F)
            where F: FnMut(&DualModuleInterfacePtr, &mut D, &mut Self, &GroupMaxUpdateLength) {
        interface.load(syndrome_pattern, dual_module);
        self.load(interface, dual_module);
        self.solve_step_callback_interface_loaded(interface, dual_module, callback);
    }

    fn solve_step_callback_interface_loaded<D: DualModuleImpl, F>(&mut self, interface: &DualModuleInterfacePtr, dual_module: &mut D, mut callback: F)
            where F: FnMut(&DualModuleInterfacePtr, &mut D, &mut Self, &GroupMaxUpdateLength) {
        self.begin_resolving(interface, dual_module);
        let mut group_max_update_length = dual_module.compute_maximum_update_length();
        while !group_max_update_length.is_unbounded() {
            callback(interface, dual_module, self, &group_max_update_length);
            if let Some(length) = group_max_update_length.get_valid_growth() {
                dual_module.grow(length);
            } else {
                self.resolve(group_max_update_length, interface, dual_module);
            }
            group_max_update_length = dual_module.compute_maximum_update_length();
        }
    }

    fn subgraph(&mut self, interface: &DualModuleInterfacePtr, dual_module: &mut impl DualModuleImpl) -> Subgraph;

    fn subgraph_range(&mut self, interface: &DualModuleInterfacePtr, dual_module: &mut impl DualModuleImpl, initializer: &SolverInitializer) -> (Subgraph, WeightRange) {
        let subgraph = self.subgraph(interface, dual_module);
        let weight_range = WeightRange::new(interface.sum_dual_variables(), Rational::from_usize(initializer.get_subgraph_total_weight(&subgraph)).unwrap());
        (subgraph, weight_range)
    }

    /// performance profiler report
    fn generate_profiler_report(&self) -> serde_json::Value { json!({}) }

}
