//! single hair looks at every non-zero dual variable and find relaxers that involves only 3 dual variables
//!
//! Generics for plugins, defining the necessary interfaces for a plugin
//!
//! A plugin must implement Clone trait, because it will be cloned multiple times for each cluster
//!

use crate::decoding_hypergraph::*;
use crate::dual_module::*;
use crate::plugin::*;
use crate::plugin_union_find::*;
use crate::relaxer::*;

#[derive(Debug, Clone, Default)]
pub struct PluginSingleHair {}

impl PluginImpl for PluginSingleHair {
    fn find_relaxers(
        &self,
        decoding_graph: &DecodingHyperGraph,
        matrix: &mut EchelonMatrix,
        positive_dual_nodes: &[DualNodePtr],
    ) -> Vec<Relaxer> {
        // single hair requires the matrix to have at least one feasible solution
        if let Some(relaxer) = PluginUnionFind::find_single_relaxer(decoding_graph, matrix) {
            return vec![relaxer];
        }
        // then try to find more relaxers
        for dual_node_ptr in positive_dual_nodes.iter() {
            let dual_node = dual_node_ptr.read_recursive();
            println!("find non-zero dual node: {}", dual_node.index);
            // matrix
        }
        vec![]
    }
}

// #[cfg(test)]
// pub mod tests {
//     use super::*;
//     use crate::example_codes::*;
//     use crate::primal_module_serial::tests::*;
//     use crate::primal_module_serial::*;

//     #[test]
//     fn plugin_single_hair_basic_1() {
//         // cargo test plugin_single_hair_basic_1 -- --nocapture
//         let visualize_filename = "plugin_single_hair_basic_1.json".to_string();
//         let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
//         let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
//         primal_module_serial_basic_standard_syndrome(
//             code,
//             visualize_filename,
//             defect_vertices,
//             4,
//             vec![PluginSingleHair::entry()],
//             GrowingStrategy::SingleCluster,
//         );
//     }
// }
