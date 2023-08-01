//! Relaxer Optimizer
//!
//! It's possible that two (or more) positive relaxers are bouncing, and the actual growth
//! is exponentially smaller but never reaching the optimal. In this case, we need
//! this module to optimize the positive relaxers. This only takes effect when such bouncing
//! is detected, and remains minimum in all other cases to avoid reduce time complexity.
//!

use derivative::Derivative;

use crate::relaxer::*;
use std::collections::BTreeSet;

#[derive(Derivative)]
#[derivative(Default(new = "true"))]
pub struct RelaxerOptimizer {
    /// the set of existing relaxers
    relaxers: BTreeSet<Relaxer>,
}

impl RelaxerOptimizer {
    /// moves all relaxer from other to here, when merging clusters
    pub fn append(&mut self, other: &mut RelaxerOptimizer) {
        self.relaxers.append(&mut other.relaxers)
    }

    pub fn insert(&mut self, relaxer: Relaxer) {
        self.relaxers.insert(relaxer);
    }

    pub fn should_optimize(&self, relaxer: &Relaxer) -> bool {
        self.relaxers.contains(relaxer)
    }

    pub fn optimize(&self, relaxer: Relaxer) -> Relaxer {
        // look at existing relaxers and propose a best direction
        relaxer
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn relaxer_optimizer_simple() {
        // cargo test relaxer_optimizer_simple -- --nocapture
        let mut relaxer_optimizer = RelaxerOptimizer::new();
    }
}
