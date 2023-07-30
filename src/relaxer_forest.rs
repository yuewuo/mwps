//! Relaxer Forest
//!
//! Maintain several lists of relaxers
//!

use crate::invalid_subgraph::*;
use crate::relaxer::*;
use crate::util::*;
use num_traits::Signed;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

pub type RelaxerVec = Vec<Relaxer>;

/// a forest of relaxers that possibly depend on each other
pub struct RelaxerForest {
    /// keep track of the remaining tight edges for quick validation:
    /// these edges cannot grow unless untightened by some relaxers
    pub tight_edges: BTreeSet<EdgeIndex>,
    /// keep track of the subgraphs that are allowed to shrink:
    /// these should be all positive dual variables, all others are yS = 0
    pub shrinkable_subgraphs: HashSet<Arc<InvalidSubgraph>>,
    /// each untightened edge corresponds to a relaxer with speed:
    /// to untighten the edge for a unit length, how much should a relaxer be executed
    pub edge_untightener: HashMap<EdgeIndex, (Arc<Relaxer>, Rational)>,
    /// expanded relaxer results, as part of the dynamic programming:
    /// the expanded relaxer is a valid relaxer only growing of initial un-tight edges,
    /// not any edges untightened by other relaxers
    pub expanded_relaxers: HashMap<Arc<Relaxer>, Relaxer>,
}

impl RelaxerForest {
    pub fn new<IterEdge, IterSubgraph>(tight_edges: IterEdge, shrinkable_subgraphs: IterSubgraph) -> Self
    where
        IterEdge: Iterator<Item = EdgeIndex>,
        IterSubgraph: Iterator<Item = Arc<InvalidSubgraph>>,
    {
        Self {
            tight_edges: BTreeSet::from_iter(tight_edges),
            shrinkable_subgraphs: HashSet::from_iter(shrinkable_subgraphs),
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
                return Err(format!("invalid relaxer: try to grow a tight edge {edge_index}"));
            }
        }
        // a relaxer cannot shrink any zero dual variable
        for (invalid_subgraph, grow_ratio) in relaxer.direction.iter() {
            if grow_ratio.is_negative() && !self.shrinkable_subgraphs.contains(invalid_subgraph) {
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

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn relaxer_forest_validate() {
        // cargo test --features=colorful relaxer_forest_validate -- --nocapture
        let tight_edges: BTreeSet<EdgeIndex> = [0, 1, 2, 3, 4, 5, 6].into();
    }
}
