use crate::invalid_subgraph::*;
use crate::util::*;
use num_traits::{Signed, Zero};
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Relaxer {
    /// the hash value calculated by other fields
    pub hash_value: u64,
    /// the direction of invalid subgraphs
    pub direction: Vec<(Arc<InvalidSubgraph>, Rational)>,
    /// the edges that will be untightened after growing along `direction`;
    /// basically all the edges that have negative `overall_growing_rate`
    pub untighten_edges: Vec<(EdgeIndex, Rational)>,
    /// the edges that will grow
    pub growing_edges: Vec<(EdgeIndex, Rational)>,
}

impl Hash for Relaxer {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash_value.hash(state);
    }
}

impl Relaxer {
    pub fn new(direction: Vec<(Arc<InvalidSubgraph>, Rational)>) -> Self {
        let mut edges = BTreeMap::new();
        for (invalid_subgraph, speed) in direction.iter() {
            for &edge_index in invalid_subgraph.hairs.iter() {
                if let Some(edge) = edges.get_mut(&edge_index) {
                    *edge += speed;
                } else {
                    edges.insert(edge_index, speed.clone());
                }
            }
        }
        let mut untighten_edges = vec![];
        let mut growing_edges = vec![];
        for (edge_index, speed) in edges {
            if speed.is_negative() {
                untighten_edges.push((edge_index, speed));
            } else if speed.is_positive() {
                growing_edges.push((edge_index, speed));
            }
        }
        let mut relaxer = Self {
            hash_value: 0,
            direction,
            untighten_edges,
            growing_edges,
        };
        debug_assert_eq!(relaxer.sanity_check(), Ok(()));
        relaxer.update_hash();
        relaxer
    }

    pub fn sanity_check(&self) -> Result<(), String> {
        // check summation of ΔyS >= 0
        let mut sum_speed = Rational::zero();
        for (_, speed) in self.direction.iter() {
            sum_speed += speed;
        }
        if sum_speed.is_negative() {
            return Err(format!("the summation of ΔyS is negative: {:?}", sum_speed));
        }
        if self.untighten_edges.is_empty() && sum_speed.is_zero() {
            return Err(
                "a valid relaxer must either increase overall ΔyS or untighten some edges"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub fn update_hash(&mut self) {
        let mut hasher = DefaultHasher::new();
        // only hash the direction since other field are derived from the direction
        self.direction.hash(&mut hasher);
        self.hash_value = hasher.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyper_decoding_graph::tests::*;
    use std::collections::BTreeSet;

    #[test]
    fn relaxer_good() {
        // cargo test relaxer_good -- --nocapture
        let visualize_filename = "relaxer_good.json".to_string();
        let (decoding_graph, ..) = color_code_5_decoding_graph(vec![7, 1], visualize_filename);
        let invalid_subgraph = Arc::new(InvalidSubgraph::new_complete(
            vec![7].into_iter().collect(),
            BTreeSet::new(),
            decoding_graph.as_ref(),
        ));
        use num_traits::One;
        let relaxer = Relaxer::new(vec![(invalid_subgraph, Rational::one())]);
        println!("relaxer: {relaxer:?}");
        assert!(relaxer.untighten_edges.is_empty());
    }

    #[test]
    #[should_panic]
    fn relaxer_bad() {
        // cargo test relaxer_bad -- --nocapture
        let visualize_filename = "relaxer_bad.json".to_string();
        let (decoding_graph, ..) = color_code_5_decoding_graph(vec![7, 1], visualize_filename);
        let invalid_subgraph = Arc::new(InvalidSubgraph::new_complete(
            vec![7].into_iter().collect(),
            BTreeSet::new(),
            decoding_graph.as_ref(),
        ));
        let relaxer: Relaxer = Relaxer::new(vec![(invalid_subgraph, Rational::zero())]);
        println!("relaxer: {relaxer:?}"); // should not print because it panics
    }
}
