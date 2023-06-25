//! Plugin
//! 
//! Generics for plugins, defining the necessary interfaces for a plugin
//! 
//! A plugin must implement Clone trait, because it will be cloned multiple times for each cluster
//!

use crate::parity_matrix::*;
use crate::dual_module::*;
use crate::relaxer_pool::*;

/// common trait that must be implemented for each plugin
pub trait PluginImpl {

    /// given the tight edges and parity constraints, find relaxers
    fn find_relaxers(&self, matrix: ParityMatrix, dual_nodes: &[DualNodePtr]) -> RelaxerVec;

}
