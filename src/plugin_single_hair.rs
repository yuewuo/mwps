//! single hair looks at every non-zero dual variable and find relaxers that involves only 3 dual variables
//!
//! Generics for plugins, defining the necessary interfaces for a plugin
//!
//! A plugin must implement Clone trait, because it will be cloned multiple times for each cluster
//!

use crate::decoding_hypergraph::*;
use crate::dual_module::*;
use crate::invalid_subgraph::InvalidSubgraph;
use crate::matrix::*;
use crate::plugin::*;
use crate::plugin_union_find::*;
use crate::relaxer::*;
use crate::util::*;
use num_traits::One;
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct PluginSingleHair {}

impl PluginImpl for PluginSingleHair {
    fn find_relaxers(
        &self,
        decoding_graph: &DecodingHyperGraph,
        matrix: &mut EchelonMatrix,
        positive_dual_nodes: &[DualNodePtr],
    ) -> Vec<Relaxer> {
        // single hair requires the matrix to have at least one feasible solution
        if let Some(relaxer) = PluginUnionFind::find_single_relaxer(decoding_graph, matrix) {
            return vec![relaxer];
        }
        // then try to find more relaxers
        let mut relaxers = vec![];
        for dual_node_ptr in positive_dual_nodes.iter() {
            let dual_node = dual_node_ptr.read_recursive();
            let mut hair_view = HairView::new(matrix, dual_node.invalid_subgraph.hairs.iter().cloned());
            debug_assert!(hair_view.get_echelon_satisfiable());
            // hair_view.printstd();
            // optimization: check if there exists a single-hair solution, if not, clear the previous relaxers
            let constrained_rows: Vec<EdgeIndex> = (0..hair_view.rows()).filter(|&row| hair_view.get_rhs(row)).collect();
            let mut has_single_hair_solution = false;
            for column in 0..hair_view.columns() {
                if constrained_rows
                    .iter()
                    .all(|&row| hair_view.get_lhs(row, hair_view.column_to_var_index(column)))
                {
                    has_single_hair_solution = true;
                    break;
                }
            }
            if !has_single_hair_solution {
                relaxers.clear(); // no need for relaxers from other dual nodes
            }
            for &row in constrained_rows.iter() {
                let mut unnecessary_edges = vec![];
                for column in 0..hair_view.columns() {
                    let var_index = hair_view.column_to_var_index(column);
                    if !hair_view.get_lhs(row, var_index) {
                        let edge_index = hair_view.var_to_edge_index(var_index);
                        unnecessary_edges.push(edge_index);
                    }
                }
                if !unnecessary_edges.is_empty() {
                    // we can construct a relaxer here, by growing a new invalid subgraph that
                    // removes those unnecessary edges and shrinking the existing one
                    let mut vertices: BTreeSet<VertexIndex> = hair_view.get_vertices();
                    let mut edges: BTreeSet<EdgeIndex> = BTreeSet::from_iter(hair_view.get_base_view_edges());
                    for &edge_index in dual_node.invalid_subgraph.hairs.iter() {
                        edges.remove(&edge_index);
                    }
                    for &edge_index in unnecessary_edges.iter() {
                        edges.insert(edge_index);
                        vertices.extend(decoding_graph.get_edge_neighbors(edge_index));
                    }
                    let invalid_subgraph = Arc::new(InvalidSubgraph::new_complete(vertices, edges, decoding_graph));
                    let relaxer = Relaxer::new(
                        [
                            (invalid_subgraph, Rational::one()),
                            (dual_node.invalid_subgraph.clone(), -Rational::one()),
                        ]
                        .into(),
                    );
                    relaxers.push(relaxer);
                }
            }
            if !has_single_hair_solution {
                break; // no need to check other dual nodes
            }
        }
        relaxers
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::example_codes::*;
    use crate::primal_module_serial::tests::*;
    use crate::primal_module_serial::*;
    use test_case::test_case;

    #[test_case("single_cluster", GrowingStrategy::SingleCluster)]
    #[test_case("multiple_cluster", GrowingStrategy::MultipleClusters)]
    fn plugin_single_hair_basic_1(suffix: &str, growing_strategy: GrowingStrategy) {
        // cargo test --features=colorful plugin_single_hair_basic_1 -- --nocapture
        let visualize_filename = format!("plugin_single_hair_basic_1_{suffix}.json");
        let defect_vertices = vec![10, 11, 12, 15, 16, 17, 18];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![PluginSingleHair::entry_with_strategy(RepeatStrategy::Once)],
            growing_strategy,
        );
    }

    // error_pattern: [9, 10, 13, 14, 15, 17]
    // defect_vertices: [8, 9, 11, 12, 16, 19, 20, 21]
    #[test]
    fn plugin_single_hair_debug_1() {
        // cargo test --features=colorful plugin_single_hair_debug_1 -- --nocapture
        let visualize_filename = "plugin_single_hair_debug_1.json".to_string();
        let defect_vertices = vec![8, 9, 11, 12, 16, 19, 20, 21];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.01, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            6,
            vec![PluginSingleHair::entry_with_strategy(RepeatStrategy::Once)],
            GrowingStrategy::SingleCluster,
        );
    }

    // error_pattern: [14, 18, 21, 23]
    // defect_vertices: [2, 3, 12, 13, 17, 19, 20]
    #[test]
    fn plugin_single_hair_debug_2() {
        // cargo test --features=colorful plugin_single_hair_debug_2 -- --nocapture
        let visualize_filename = "plugin_single_hair_debug_2.json".to_string();
        let defect_vertices = vec![2, 3, 12, 13, 17, 19, 20];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.1, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![PluginSingleHair::entry_with_strategy(RepeatStrategy::Once)],
            GrowingStrategy::SingleCluster,
        );
    }

    // error_pattern: [9, 12, 22, 24]
    // defect_vertices: [3, 8, 10, 11, 12, 13, 16, 17, 20, 21, 22, 23]
    #[test]
    fn plugin_single_hair_debug_3() {
        // cargo test --features=colorful plugin_single_hair_debug_3 -- --nocapture
        let visualize_filename = "plugin_single_hair_debug_3.json".to_string();
        let defect_vertices = vec![3, 8, 10, 11, 12, 13, 16, 17, 20, 21, 22, 23];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.1, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![PluginSingleHair::entry_with_strategy(RepeatStrategy::Once)],
            GrowingStrategy::SingleCluster,
        );
    }

    // error_pattern: [6, 7, 10]
    // defect_vertices: [5, 7, 11, 14, 15]
    #[test]
    fn plugin_single_hair_debug_4() {
        // cargo test --features=colorful plugin_single_hair_debug_4 -- --nocapture
        let visualize_filename = "plugin_single_hair_debug_4.json".to_string();
        let defect_vertices = vec![5, 7, 11, 14, 15];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.1, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            3,
            vec![PluginSingleHair::entry_with_strategy(RepeatStrategy::Once)],
            GrowingStrategy::SingleCluster,
        );
    }

    // error_pattern: [12, 15, 16]
    // defect_vertices: [10, 11, 14, 17, 20]
    #[test]
    fn plugin_single_hair_debug_5() {
        // cargo test --features=colorful plugin_single_hair_debug_5 -- --nocapture
        let visualize_filename = "plugin_single_hair_debug_5.json".to_string();
        let defect_vertices = vec![10, 11, 14, 17, 20];
        let code = CodeCapacityTailoredCode::new(5, 0., 0.1, 1);
        primal_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            3,
            vec![PluginSingleHair::entry_with_strategy(RepeatStrategy::Once)],
            GrowingStrategy::SingleCluster,
        );
    }
}
