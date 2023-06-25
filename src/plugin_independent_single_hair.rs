//! independent single hair looks at every non-zero dual variable and 
//! 
//! Generics for plugins, defining the necessary interfaces for a plugin
//! 
//! A plugin must implement Clone trait, because it will be cloned multiple times for each cluster
//!

use crate::framework::*;
use crate::parity_matrix::*;
use crate::plugin::*;
use crate::dual_module::*;
use crate::num_traits::Zero;


#[derive(Debug, Clone, Default)]
pub struct PluginIndependentSingleHair {

}

impl PluginImpl for PluginIndependentSingleHair {

    fn find_relaxers(&self, mut matrix: ParityMatrix, dual_nodes: &[DualNodePtr]) -> Vec<Relaxer> {
        for dual_node_ptr in dual_nodes.iter() {
            let dual_node = dual_node_ptr.read_recursive();
            if dual_node.dual_variable.is_zero() {
                continue  // no requirement on zero dual variables
            }
            println!("find non-zero dual node: {}", dual_node.index);
            // matrix
            matrix.clear_implicit_shrink();

        }
        vec![]
    }

}

