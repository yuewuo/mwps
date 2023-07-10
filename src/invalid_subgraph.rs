use crate::hyper_decoding_graph::*;
use crate::parity_matrix::*;
use crate::util::*;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// the invalid subgraph is the core of the framework, $S = (V_S, E_S)$
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidSubgraph {
    /// the hash value calculated by other fields
    pub hash_value: u64,
    /// subset of vertices
    pub vertices: BTreeSet<VertexIndex>,
    /// subset of edges
    pub edges: BTreeSet<EdgeIndex>,
    /// the hair of the invalid subgraph, to avoid repeated computation
    pub hairs: BTreeSet<EdgeIndex>,
}

impl Hash for InvalidSubgraph {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash_value.hash(state);
    }
}

impl InvalidSubgraph {
    /// construct an invalid subgraph using only $E_S$, and constructing the $V_S$ by $\cup E_S$
    #[allow(clippy::unnecessary_cast)]
    pub fn new(edges: BTreeSet<EdgeIndex>, decoding_graph: &HyperDecodingGraph) -> Self {
        let mut vertices = BTreeSet::new();
        for &edge_index in edges.iter() {
            let hyperedge =
                &decoding_graph.model_graph.initializer.weighted_edges[edge_index as usize];
            for &vertex_index in hyperedge.vertices.iter() {
                vertices.insert(vertex_index);
            }
        }
        Self::new_complete(vertices, edges, decoding_graph)
    }

    /// complete definition of invalid subgraph $S = (V_S, E_S)$
    #[allow(clippy::unnecessary_cast)]
    pub fn new_complete(
        vertices: BTreeSet<VertexIndex>,
        edges: BTreeSet<EdgeIndex>,
        decoding_graph: &HyperDecodingGraph,
    ) -> Self {
        let mut hairs = BTreeSet::new();
        for &vertex_index in vertices.iter() {
            let vertex = &decoding_graph.model_graph.vertices[vertex_index as usize];
            for &edge_index in vertex.edges.iter() {
                if !edges.contains(&edge_index) {
                    hairs.insert(edge_index);
                }
            }
        }
        let mut invalid_subgraph = Self {
            hash_value: 0,
            vertices,
            edges,
            hairs,
        };
        debug_assert_eq!(invalid_subgraph.sanity_check(decoding_graph), Ok(()));
        invalid_subgraph.update_hash();
        invalid_subgraph
    }

    pub fn update_hash(&mut self) {
        let mut hasher = DefaultHasher::new();
        self.vertices.hash(&mut hasher);
        self.edges.hash(&mut hasher);
        self.hairs.hash(&mut hasher);
        self.hash_value = hasher.finish();
    }

    // check whether this invalid subgraph is indeed invalid, this is costly and should be disabled in release runs
    #[allow(clippy::unnecessary_cast)]
    pub fn sanity_check(&self, decoding_graph: &HyperDecodingGraph) -> Result<(), String> {
        if self.vertices.is_empty() {
            return Err("an invalid subgraph must contain at least one vertex".to_string());
        }
        // check if all vertices are valid
        for &vertex_index in self.vertices.iter() {
            if vertex_index >= decoding_graph.model_graph.initializer.vertex_num {
                return Err(format!(
                    "vertex {vertex_index} is not a vertex in the model graph"
                ));
            }
        }
        // check if every edge is subset of its vertices
        for &edge_index in self.edges.iter() {
            if edge_index as usize >= decoding_graph.model_graph.initializer.weighted_edges.len() {
                return Err(format!(
                    "edge {edge_index} is not an edge in the model graph"
                ));
            }
            let hyperedge =
                &decoding_graph.model_graph.initializer.weighted_edges[edge_index as usize];
            for &vertex_index in hyperedge.vertices.iter() {
                if !self.vertices.contains(&vertex_index) {
                    return Err(format!(
                        "hyperedge {edge_index} connects vertices {:?}, \
                    but vertex {vertex_index} is not in the invalid subgraph vertices {:?}",
                        hyperedge.vertices, self.vertices
                    ));
                }
            }
        }
        // check the edges indeed cannot satisfy the requirement of the vertices
        let mut matrix = ParityMatrix::new_no_phantom();
        for &edge_index in self.edges.iter() {
            matrix.add_tight_variable(edge_index);
        }
        for &vertex_index in self.vertices.iter() {
            matrix.add_parity_check_with_decoding_graph(vertex_index, decoding_graph);
        }
        if matrix.check_is_satisfiable() {
            return Err(format!(
                "it's a valid subgraph because edges {:?} ⊆ {:?} can satisfy the parity requirement from vertices {:?}",
                matrix.get_joint_solution().unwrap(),
                self.edges,
                self.vertices
            ));
        }
        Ok(())
    }

    pub fn generate_matrix(&self, decoding_graph: &HyperDecodingGraph) -> ParityMatrix {
        let mut matrix = ParityMatrix::new();
        for &edge_index in self.hairs.iter() {
            matrix.add_variable(edge_index);
        }
        for &vertex_index in self.vertices.iter() {
            matrix.add_parity_check_with_decoding_graph(vertex_index, decoding_graph);
        }
        matrix
    }
}

// shortcuts for easier code writing at debugging
impl InvalidSubgraph {
    pub fn new_ptr(edges: BTreeSet<EdgeIndex>, decoding_graph: &HyperDecodingGraph) -> Arc<Self> {
        Arc::new(Self::new(edges, decoding_graph))
    }
    pub fn new_vec_ptr(edges: &[EdgeIndex], decoding_graph: &HyperDecodingGraph) -> Arc<Self> {
        Self::new_ptr(edges.iter().cloned().collect(), decoding_graph)
    }
    pub fn new_complete_ptr(
        vertices: BTreeSet<VertexIndex>,
        edges: BTreeSet<EdgeIndex>,
        decoding_graph: &HyperDecodingGraph,
    ) -> Arc<Self> {
        Arc::new(Self::new_complete(vertices, edges, decoding_graph))
    }
    pub fn new_complete_vec_ptr(
        vertices: BTreeSet<VertexIndex>,
        edges: &[EdgeIndex],
        decoding_graph: &HyperDecodingGraph,
    ) -> Arc<Self> {
        Self::new_complete_ptr(
            vertices.iter().cloned().collect(),
            edges.iter().cloned().collect(),
            decoding_graph,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyper_decoding_graph::tests::*;

    #[test]
    fn framework_invalid_subgraph() {
        // cargo test framework_invalid_subgraph -- --nocapture
        let visualize_filename = "framework_invalid_subgraph.json".to_string();
        let (decoding_graph, ..) = color_code_5_decoding_graph(vec![7, 1], visualize_filename);
        let invalid_subgraph_1 =
            InvalidSubgraph::new(vec![13].into_iter().collect(), decoding_graph.as_ref());
        println!("invalid_subgraph_1: {invalid_subgraph_1:?}");
        assert_eq!(
            invalid_subgraph_1.vertices,
            vec![2, 6, 7].into_iter().collect()
        );
        assert_eq!(invalid_subgraph_1.edges, vec![13].into_iter().collect());
        assert_eq!(
            invalid_subgraph_1.hairs,
            vec![5, 6, 9, 10, 11, 12, 14, 15, 16, 17]
                .into_iter()
                .collect()
        );
    }

    #[test]
    #[should_panic]
    fn framework_valid_subgraph() {
        // cargo test framework_valid_subgraph -- --nocapture
        let visualize_filename = "framework_valid_subgraph.json".to_string();
        let (decoding_graph, ..) = color_code_5_decoding_graph(vec![7, 1], visualize_filename);
        let invalid_subgraph =
            InvalidSubgraph::new(vec![6, 10].into_iter().collect(), decoding_graph.as_ref());
        println!("invalid_subgraph: {invalid_subgraph:?}"); // should not print because it panics
    }
}
