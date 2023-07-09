//! single hair looks at every non-zero dual variable and find relaxers that involves only 3 dual variables
//!
//! Generics for plugins, defining the necessary interfaces for a plugin
//!
//! A plugin must implement Clone trait, because it will be cloned multiple times for each cluster
//!

use crate::dual_module::*;
use crate::framework::*;
use crate::num_traits::Zero;
use crate::parity_matrix::*;
use crate::plugin::*;

#[derive(Debug, Clone, Default)]
pub struct PluginSingleHair {}

impl PluginImpl for PluginSingleHair {
    fn find_relaxers(
        &self,
        decoding_graph: &HyperDecodingGraph,
        matrix: &ParityMatrix,
        dual_nodes: &[DualNodePtr],
    ) -> Vec<Relaxer> {
        // for dual_node_ptr in dual_nodes.iter() {
        //     let dual_node = dual_node_ptr.read_recursive();
        //     if dual_node.dual_variable.is_zero() {
        //         continue; // no requirement on zero dual variables
        //     }
        //     println!("find non-zero dual node: {}", dual_node.index);
        //     // matrix
        //     matrix.clear_implicit_shrink();
        // }
        vec![]
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::example_codes::*;
    use crate::primal_module_serial::tests::*;
    use crate::primal_module_serial::*;

    #[test]
    fn primal_module_serial_basic_4_single_plug1() {
        // cargo test primal_module_serial_basic_4_single_plug1 -- --nocapture
        let visualize_filename = "primal_module_serial_basic_4_single_plug1.json".to_string();
        let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![PluginSingleHair::entry()],
            GrowingStrategy::SingleCluster,
        );
    }
}
