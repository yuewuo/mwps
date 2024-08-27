//! if the tight edges doesn't constitute a feasible solution, then report a relaxer,
//! just like in the hypergraph UF decoder paper: https://arxiv.org/abs/2103.08049
//!
//! This plugin will always be appended to the end of the plugin sequence to make
//! sure there is a feasible MINLP solution.
//!


use crate::decoding_hypergraph::*;
use crate::dual_module::*;
use crate::invalid_subgraph::*;
use crate::matrix::*;
use crate::num_traits::One;
use crate::plugin::*;
use crate::relaxer::*;
use crate::util::*;
use std::collections::BTreeSet;

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak};

#[derive(Debug, Clone, Default)]
pub struct PluginUnionFind {}

impl PluginUnionFind {
    /// check if the cluster is valid (hypergraph union-find decoder)
    pub fn find_single_relaxer(matrix: &mut EchelonMatrix) -> Option<Relaxer> {
        if matrix.get_echelon_info().satisfiable {
            return None; // cannot find any relaxer
        }
        let local_edges: BTreeSet<EdgePtr> = matrix.get_view_edges().iter().map(|e| e.upgrade_force()).collect();
        let invalid_subgraph = InvalidSubgraph::new_complete_ptr(
            &matrix.get_vertices(),
            &local_edges,
        );
        Some(Relaxer::new([(invalid_subgraph, Rational::one())].into()))
    }
}

impl PluginImpl for PluginUnionFind {
    fn find_relaxers(
        &self,
        matrix: &mut EchelonMatrix,
        _positive_dual_nodes: &[DualNodePtr],
    ) -> Vec<Relaxer> {
        if let Some(relaxer) = Self::find_single_relaxer(matrix) {
            vec![relaxer]
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::example_codes::*;
    use crate::primal_module_serial::{tests::*, GrowingStrategy};
    use test_case::test_case;

    #[test_case("single_cluster", GrowingStrategy::SingleCluster)]
    #[test_case("multiple_cluster", GrowingStrategy::MultipleClusters)]
    fn plugin_union_find_basic_1(suffix: &str, growing_strategy: GrowingStrategy) {
        // cargo test plugin_union_find_basic_1 -- --nocapture
        let visualize_filename = format!("plugin_union_find_basic_1_{suffix}.json");
        let defect_vertices = vec![10, 11, 16, 17];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            1,
            vec![PluginUnionFind::entry()],
            growing_strategy,
        );
    }
}
