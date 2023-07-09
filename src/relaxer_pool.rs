//! Relaxer Pool
//!
//! Maintain several lists of relaxers
//!

use crate::dual_module::*;
use crate::framework::*;
use crate::pointers::RwLockPtr;
use crate::util::*;
use std::collections::BTreeSet;
use std::sync::Arc;

pub type RelaxerVec = Vec<Relaxer>;

/// a pool of relaxers
pub struct RelaxerPool {
    /// keep track of the remaining tight edges for quick validation
    pub tight_edges: BTreeSet<EdgeIndex>,
    /// keep track of all positive dual variables, all others are yS = 0
    pub positive_dual_nodes: BTreeSet<NodeIndex>,
    /// existing relaxers in a structural
    pub lists: Vec<Arc<Relaxer>>,
}

impl RelaxerPool {
    pub fn new(tight_edges: BTreeSet<EdgeIndex>, positive_dual_nodes: &[DualNodePtr]) -> Self {
        Self {
            tight_edges,
            lists: vec![],
            positive_dual_nodes: positive_dual_nodes
                .iter()
                .map(|ptr| ptr.read_recursive().index)
                .collect(),
        }
    }

    /// check if the proposed relaxers are indeed relaxers given the edges
    /// untightened by existing relaxers
    pub fn validate(&self, relaxer: &Relaxer) -> Result<(), String> {
        // a relaxer cannot grow any tight edge
        for (edge_index, _) in relaxer.growing_edges.iter() {
            if self.tight_edges.contains(edge_index) {
                return Err(format!(
                    "invalid relaxer try to grow a tight edge {edge_index}"
                ));
            }
        }
        Ok(())
    }

    /// add a relaxer to the pool
    pub fn add(&mut self, relaxer: Relaxer) {
        self.validate(&relaxer).unwrap();
    }

    pub fn extend(&mut self, relaxers: RelaxerVec) {
        for relaxer in relaxers.into_iter() {
            self.add(relaxer);
        }
    }
}
