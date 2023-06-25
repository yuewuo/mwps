//! Relaxer Pool
//!
//! Maintain several lists of relaxers
//!

use crate::framework::*;

pub type RelaxerVec = Vec<Relaxer>;

/// a pool of relaxers, each plugin corresponds to one vec
pub struct RelaxerPool {
    pub lists: Vec<RelaxerVec>,
}

pub trait RelaxerVecImpl {}

impl RelaxerVecImpl for RelaxerVec {}
