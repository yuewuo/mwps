//! Plugin
//!
//! Generics for plugins, defining the necessary interfaces for a plugin
//!
//! A plugin must implement Clone trait, because it will be cloned multiple times for each cluster
//!

use crate::decoding_hypergraph::*;
use crate::derivative::Derivative;
use crate::dual_module::*;
use crate::matrix::*;
use crate::plugin_union_find::*;
use crate::relaxer::*;
use crate::relaxer_forest::*;
use std::collections::BTreeSet;
use std::sync::Arc;

pub type EchelonMatrix = Echelon<Tail<Tight<BasicMatrix>>>;

/// common trait that must be implemented for each plugin
pub trait PluginImpl {
    /// given the tight edges and parity constraints, find relaxers
    fn find_relaxers(
        &self,
        decoding_graph: &DecodingHyperGraph,
        matrix: &mut EchelonMatrix,
        positive_dual_nodes: &[DualNodePtr],
    ) -> RelaxerVec;

    /// create a plugin entry with default settings
    fn entry() -> PluginEntry
    where
        Self: 'static + Send + Sync + Sized + Default,
    {
        PluginEntry {
            plugin: Arc::new(Self::default()),
            repeat_strategy: RepeatStrategy::default(),
        }
    }
}

/// configuration of how a plugin should be repeated
#[derive(Derivative, PartialEq, Eq, Clone)]
#[derivative(Debug, Default(new = "true"))]
pub enum RepeatStrategy {
    /// single execution
    #[derivative(Default)]
    Once,
    /// repeated execution
    Multiple {
        /// it stops after `max_repetition` repeats
        max_repetition: usize,
    },
}

/// describes what plugins to enable and also the recursive strategy
pub struct PluginEntry {
    /// the implementation of a plugin
    pub plugin: Arc<dyn PluginImpl + Send + Sync>,
    /// repetition strategy
    pub repeat_strategy: RepeatStrategy,
}

pub type PluginVec = Vec<PluginEntry>;

pub struct PluginManager {
    pub plugins: Arc<PluginVec>,
}

impl PluginManager {
    pub fn new(plugins: Arc<PluginVec>) -> Self {
        Self { plugins }
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn find_relaxer(
        &mut self,
        decoding_graph: &DecodingHyperGraph,
        matrix: &mut EchelonMatrix,
        positive_dual_nodes: &[DualNodePtr],
    ) -> Option<Relaxer> {
        let mut relaxer_forest = RelaxerForest::new(BTreeSet::from_iter(matrix.get_view_edges()), positive_dual_nodes);
        for plugin_entry in self.plugins.iter().chain(std::iter::once(&PluginUnionFind::entry())) {
            let mut repeat = true;
            let mut repeat_count = 0;
            while repeat {
                // execute the plugin
                let relaxers = plugin_entry
                    .plugin
                    .find_relaxers(decoding_graph, &mut *matrix, positive_dual_nodes);
                relaxer_forest.extend(relaxers);
                // determine whether repeat again
                match plugin_entry.repeat_strategy {
                    RepeatStrategy::Once => {
                        repeat = false;
                    }
                    RepeatStrategy::Multiple { max_repetition } => {
                        if repeat_count + 1 >= max_repetition {
                            repeat = false;
                        }
                    }
                }
                repeat_count += 1;
            }
        }
        // TODO: recover implicit shrink here
        None
    }
}
