use crate::util::*;
use std::sync::Arc;
use crate::visualize::*;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use num_traits::Signed;
use std::hash::{Hasher, Hash};
use std::collections::hash_map::DefaultHasher;


/// hyper model graph that contains static information regardless of the syndrome
#[derive(Debug, Clone)]
pub struct HyperModelGraph {
    /// initializer
    pub initializer: Arc<SolverInitializer>,
    /// the data structure for each vertex
    pub vertices: Vec<HyperModelGraphVertex>,
}

#[derive(Default, Debug, Clone)]
pub struct HyperModelGraphVertex {
    /// the incident edges
    pub edges: Vec<EdgeIndex>,
}

impl HyperModelGraph {

    pub fn new(initializer: Arc<SolverInitializer>) -> Self {
        let mut vertices: Vec<HyperModelGraphVertex> = vec![HyperModelGraphVertex::default(); initializer.vertex_num];
        for (edge_index, (incident_vertices, _weight)) in initializer.weighted_edges.iter().enumerate() {
            for &vertex_index in incident_vertices.iter() {
                vertices[vertex_index].edges.push(edge_index);
            }
        }
        Self {
            initializer,
            vertices,
        }
    }

}

impl MWPSVisualizer for HyperModelGraph {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        self.initializer.snapshot(abbrev)
    }
}

pub struct HyperDecodingGraph {
    /// model graph
    pub model_graph: Arc<HyperModelGraph>,
    /// syndrome
    pub syndrome_pattern: Arc<SyndromePattern>,
    /// fast check whether a vertex is defect
    pub defect_vertices_hashset: HashSet<VertexIndex>,
    /// fast check whether an edge is erased
    pub erasures_hashset: HashSet<EdgeIndex>,
}

impl HyperDecodingGraph {

    pub fn new(model_graph: Arc<HyperModelGraph>, syndrome_pattern: Arc<SyndromePattern>) -> Self {
        let mut defect_vertices_hashset = HashSet::with_capacity(syndrome_pattern.defect_vertices.len());
        for &vertex_index in syndrome_pattern.defect_vertices.iter() {
            defect_vertices_hashset.insert(vertex_index);
        }
        let mut erasures_hashset = HashSet::with_capacity(syndrome_pattern.erasures.len());
        for &edge_index in syndrome_pattern.erasures.iter() {
            erasures_hashset.insert(edge_index);
        }
        Self {
            model_graph, syndrome_pattern,
            defect_vertices_hashset, erasures_hashset,
        }
    }

}

impl MWPSVisualizer for HyperDecodingGraph {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        let mut value = self.model_graph.initializer.snapshot(abbrev);
        let mut vertices = Vec::<serde_json::Value>::new();
        for vertex_index in 0..self.model_graph.initializer.vertex_num {
            vertices.push(json!({
                if abbrev { "s" } else { "is_defect" }: i32::from(self.defect_vertices_hashset.contains(&vertex_index)),
            }));
        }
        snapshot_combine_values(&mut value, json!({ "vertices": vertices }), abbrev);
        value
    }
}

/// the invalid subgraph is the core of the framework, $S = (V_S, E_S)$
#[derive(Clone, Debug)]
pub struct InvalidSubgraph {
    /// the hash value calculated by other fields
    pub hash: u64,
    /// subset of vertices
    vertices: BTreeSet<VertexIndex>,
    /// subset of edges
    edges: BTreeSet<EdgeIndex>,
    /// the hair of the invalid subgraph, to avoid repeated computation
    hairs: BTreeSet<EdgeIndex>,
}

impl InvalidSubgraph {

    /// construct an invalid subgraph using only $E_S$, and constructing the $V_S$ by $\cup E_S$ 
    pub fn new(edges: BTreeSet<EdgeIndex>, decoding_graph: &HyperDecodingGraph) -> Self {
        let mut vertices = BTreeSet::new();
        for &edge_index in edges.iter() {
            let (incident_vertices, _weight) = &decoding_graph.model_graph.initializer.weighted_edges[edge_index];
            for &vertex_index in incident_vertices.iter() {
                vertices.insert(vertex_index);
            }
        }
        Self::new_complete(vertices, edges, decoding_graph)
    }

    /// complete definition of invalid subgraph $S = (V_S, E_S)$
    pub fn new_complete(vertices: BTreeSet<VertexIndex>, edges: BTreeSet<EdgeIndex>, decoding_graph: &HyperDecodingGraph) -> Self {
        let mut hairs = BTreeSet::new();
        for &vertex_index in vertices.iter() {
            let vertex = &decoding_graph.model_graph.vertices[vertex_index];
            for &edge_index in vertex.edges.iter() {
                if !edges.contains(&edge_index) {
                    hairs.insert(edge_index);
                }
            }
        }
        let mut result = Self { hash: 0, vertices, edges, hairs, };
        debug_assert_eq!(result.sanity_check(decoding_graph), Ok(()));
        result.update_hash();
        println!("hash: {}", result.hash);
        result
    }

    pub fn update_hash(&mut self) {
        let mut hasher = DefaultHasher::new();
        self.vertices.hash(&mut hasher);
        self.edges.hash(&mut hasher);
        self.hairs.hash(&mut hasher);
        self.hash = hasher.finish();
    }

    // check whether this invalid subgraph is indeed invalid, this is costly and should be disabled in release runs
    pub fn sanity_check(&self, decoding_graph: &HyperDecodingGraph) -> Result<(), String> {
        
        Ok(())
    }

}

pub struct Relaxer {
    /// the direction of invalid subgraphs
    pub direction: Vec<(Arc<InvalidSubgraph>, Rational)>,
    /// the edges that will be untightened after growing along `direction`;
    /// basically all the edges that have negative `overall_growing_rate`
    pub untighten_edges: Vec<(EdgeIndex, Rational)>,
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
        for (edge_index, speed) in edges {
            if speed.is_negative() {
                untighten_edges.push((edge_index, speed));
            }
        }
        Self {
            direction,
            untighten_edges: untighten_edges,
        }
    }

    pub fn sanity_check(&self, decoding_graph: &HyperDecodingGraph) -> Result<(), String> {

        Ok(())
    }

}


#[cfg(test)]
mod tests {
    use super::*;
    use super::super::example_codes::*;

    fn color_code_5_model_graph() -> (Arc<HyperModelGraph>, Arc<SolverInitializer>, Visualizer, CodeCapacityColorCode) {
        let visualize_filename = format!("framework_hyper_model_graph.json");
        let code = CodeCapacityColorCode::new(5, 0.1, 1000);
        let mut visualizer = Visualizer::new(Some(visualize_data_folder() + visualize_filename.as_str()), code.get_positions(), true).unwrap();
        print_visualize_link(visualize_filename.clone());
        visualizer.snapshot_combined(format!("code"), vec![&code]).unwrap();
        let initializer = Arc::new(code.get_initializer());
        // println!("initializer: {initializer:?}");
        let model_graph = HyperModelGraph::new(initializer.clone());
        (Arc::new(model_graph), initializer, visualizer, code)
    }

    #[test]
    fn framework_hyper_model_graph() {  // cargo test framework_hyper_model_graph -- --nocapture
        let (model_graph, initializer, ..) = color_code_5_model_graph();
        println!("model_graph: {model_graph:?}");
        let mut edge_reference_initializer = 0;
        let mut edge_reference_hyper_model_graph = 0;
        for (incident_vertices, _weight) in initializer.weighted_edges.iter() {
            edge_reference_initializer += incident_vertices.len();
        }
        for vertex in model_graph.vertices.iter() {
            edge_reference_hyper_model_graph += vertex.edges.len();
        }
        assert_eq!(edge_reference_initializer, edge_reference_hyper_model_graph);
    }

    #[test]
    fn framework_invalid_subgraph() {  // cargo test framework_invalid_subgraph -- --nocapture
        let (model_graph, _initializer, mut visualizer, ..) = color_code_5_model_graph();
        let syndrome_pattern = Arc::new(SyndromePattern::new_vertices(vec![7, 1]));
        let decoding_graph = Arc::new(HyperDecodingGraph::new(model_graph.clone(), syndrome_pattern.clone()));
        visualizer.snapshot_combined(format!("syndrome"), vec![decoding_graph.as_ref()]).unwrap();
        let invalid_subgraph_1 = InvalidSubgraph::new(vec![13].into_iter().collect(), decoding_graph.as_ref());
        println!("invalid_subgraph_1: {invalid_subgraph_1:?}");
    }

}
