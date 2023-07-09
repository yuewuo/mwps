//! Relaxer Pool
//!
//! Maintain several lists of relaxers
//!

use crate::framework::*;
use crate::util::*;
use std::collections::BTreeSet;

pub type RelaxerVec = Vec<Relaxer>;

/// a pool of relaxers
pub struct RelaxerPool {
    pub tight_edges: BTreeSet<EdgeIndex>,
    pub lists: RelaxerVec,
}

impl RelaxerPool {
    pub fn new(tight_edges: BTreeSet<EdgeIndex>) -> Self {
        Self {
            tight_edges,
            lists: vec![],
        }
    }

    /// check if the proposed relaxers are indeed relaxers given the edges
    /// untightened by existing relaxers
    pub fn validate(&self, relaxer: &Relaxer) -> Result<(), String> {
        Ok(())
    }

    /// add a relaxer to the pool
    pub fn add(&mut self, relaxer: Relaxer) {
        self.validate(&relaxer).unwrap();
    }
}
