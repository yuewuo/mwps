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

pub const FOREST_ERR_MSG_GROW_TIGHT_EDGE: &str = "invalid relaxer: try to grow a tight edge";
pub const FOREST_ERR_MSG_UNSHRINKABLE: &str = "invalid relaxer: try to shrink a unshrinkable subgraph";

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
            if self.tight_edges.contains(edge_index) && !self.edge_untightener.contains_key(edge_index) {
                return Err(format!("{FOREST_ERR_MSG_GROW_TIGHT_EDGE}: {edge_index}"));
            }
        }
        // a relaxer cannot shrink any zero dual variable
        for (invalid_subgraph, grow_ratio) in relaxer.direction.iter() {
            if grow_ratio.is_negative() && !self.shrinkable_subgraphs.contains(invalid_subgraph) {
                return Err(format!("{FOREST_ERR_MSG_UNSHRINKABLE}: {invalid_subgraph:?}"));
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
    use crate::num_traits::One;

    #[test]
    fn relaxer_forest_validate() {
        // cargo test relaxer_forest_validate -- --nocapture
        let tight_edges = [0, 1, 2, 3, 4, 5, 6];
        let shrinkable_subgraphs = [
            Arc::new(InvalidSubgraph::new_raw([1].into(), [].into(), [1, 2].into())),
            Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [].into())),
        ];
        let relaxer_forest = RelaxerForest::new(tight_edges.into_iter(), shrinkable_subgraphs.iter().cloned());
        println!("relaxer_forest: {:?}", relaxer_forest.shrinkable_subgraphs);
        // invalid relaxer is forbidden
        let invalid_relaxer = Relaxer::new_raw(
            [(
                Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [].into())),
                -Rational::one(),
            )]
            .into(),
        );
        let error_message = relaxer_forest.validate(&invalid_relaxer).expect_err("should panic");
        assert_eq!(
            &error_message[..RELAXER_ERR_MSG_NEGATIVE_SUMMATION.len()],
            RELAXER_ERR_MSG_NEGATIVE_SUMMATION
        );
        // relaxer that increases a tight edge is forbidden
        let relaxer = Relaxer::new_raw(
            [(
                Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [1].into())),
                Rational::one(),
            )]
            .into(),
        );
        let error_message = relaxer_forest.validate(&relaxer).expect_err("should panic");
        assert_eq!(
            &error_message[..FOREST_ERR_MSG_GROW_TIGHT_EDGE.len()],
            FOREST_ERR_MSG_GROW_TIGHT_EDGE
        );
        // relaxer that shrinks a zero dual variable is forbidden
        let relaxer = Relaxer::new_raw(
            [
                (
                    Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [9].into())),
                    Rational::one(),
                ),
                (
                    Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [2, 3].into())),
                    -Rational::one(),
                ),
            ]
            .into(),
        );
        let error_message = relaxer_forest.validate(&relaxer).expect_err("should panic");
        assert_eq!(
            &error_message[..FOREST_ERR_MSG_UNSHRINKABLE.len()],
            FOREST_ERR_MSG_UNSHRINKABLE
        );
        // otherwise a relaxer is ok
        let relaxer = Relaxer::new_raw(
            [(
                Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [9].into())),
                Rational::one(),
            )]
            .into(),
        );
        relaxer_forest.validate(&relaxer).unwrap();
    }
}
