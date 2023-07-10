//! Relaxer Forest
//!
//! Maintain several lists of relaxers
//!

use num_traits::Signed;

use crate::dual_module::*;
use crate::invalid_subgraph::*;
use crate::pointers::RwLockPtr;
use crate::relaxer::*;
use crate::util::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

pub type RelaxerVec = Vec<Relaxer>;

/// a forest of relaxers that possibly depend on each other
pub struct RelaxerForest {
    /// keep track of the remaining tight edges for quick validation
    pub tight_edges: BTreeSet<EdgeIndex>,
    /// keep track of all positive dual variables, all others are yS = 0
    pub positive_dual_nodes: HashSet<Arc<InvalidSubgraph>>,
    /// each untightened edge corresponds to a relaxer with speed:
    /// to untighten the edge for a unit length, how much should a relaxer be executed
    pub edge_untightener: HashMap<EdgeIndex, (Arc<Relaxer>, Rational)>,
    /// expanded relaxer results, as part of the dynamic programming:
    /// the expanded relaxer is a valid relaxer only growing of initial untight edges,
    /// not any edges untightened by other relaxers
    pub expanded_relaxers: HashMap<Arc<Relaxer>, Relaxer>,
}

impl RelaxerForest {
    pub fn new(tight_edges: BTreeSet<EdgeIndex>, positive_dual_nodes: &[DualNodePtr]) -> Self {
        Self {
            tight_edges,
            positive_dual_nodes: positive_dual_nodes
                .iter()
                .map(|ptr| ptr.read_recursive().invalid_subgraph.clone())
                .collect(),
            edge_untightener: HashMap::new(),
            expanded_relaxers: HashMap::new(),
        }
    }

    /// check if the proposed relaxers are indeed relaxers given the edges
    /// untightened by existing relaxers
    pub fn validate(&self, relaxer: &Relaxer) -> Result<(), String> {
        // non-negative overall speed and effectiveness check
        relaxer.sanity_check()?;
        // a relaxer cannot grow any tight edge
        for (edge_index, _) in relaxer.growing_edges.iter() {
            if self.tight_edges.contains(edge_index) {
                return Err(format!(
                    "invalid relaxer: try to grow a tight edge {edge_index}"
                ));
            }
        }
        // a relaxer cannot shrink any zero dual variable
        for (invalid_subgraph, grow_ratio) in relaxer.direction.iter() {
            if grow_ratio.is_negative() && !self.positive_dual_nodes.contains(invalid_subgraph) {
                return Err(format!(
                    "invalid relaxer: try to shrink a zero dual node {invalid_subgraph:?}"
                ));
            }
        }
        Ok(())
    }

    /// add a relaxer to the forest
    pub fn add(&mut self, relaxer: Relaxer) {
        // validate only at debug mode to improve speed
        debug_assert_eq!(self.validate(&relaxer), Ok(()));
        // add this relaxer to the forest
    }

    pub fn extend(&mut self, relaxers: RelaxerVec) {
        for relaxer in relaxers.into_iter() {
            self.add(relaxer);
        }
    }
}
