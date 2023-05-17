use crate::util::*;
use std::sync::Arc;
use crate::visualize::*;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use num_traits::{Signed, Zero};
use std::hash::{Hasher, Hash};
use std::collections::hash_map::DefaultHasher;
use crate::parity_matrix::*;


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

    pub fn get_edge_neighbors(&self, edge_index: EdgeIndex) -> &Vec<VertexIndex> {
        &self.initializer.weighted_edges[edge_index].0
    }

    pub fn get_vertex_neighbors(&self, vertex_index: VertexIndex) -> &Vec<EdgeIndex> {
        &self.vertices[vertex_index].edges
    }

    pub fn get_edges_neighbors(&self, edges: &BTreeSet<EdgeIndex>) -> BTreeSet<VertexIndex> {
        let mut vertices = BTreeSet::new();
        for &edge_index in edges.iter() {
            vertices.extend(self.get_edge_neighbors(edge_index));
        }
        vertices
    }

    pub fn matches_subgraph_syndrome(&self, subgraph: &Subgraph, defect_vertices: &Vec<VertexIndex>) -> bool {
        self.initializer.matches_subgraph_syndrome(subgraph, defect_vertices)
    }

}

impl MWPSVisualizer for HyperModelGraph {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        self.initializer.snapshot(abbrev)
    }
}

#[derive(Debug, Clone)]
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
        let mut decoding_graph = Self {
            model_graph, syndrome_pattern: syndrome_pattern.clone(),
            defect_vertices_hashset: HashSet::new(), erasures_hashset: HashSet::new(),
        };
        decoding_graph.set_syndrome(syndrome_pattern);
        decoding_graph
    }

    pub fn set_syndrome(&mut self, syndrome_pattern: Arc<SyndromePattern>) {
        self.defect_vertices_hashset.clear();
        self.erasures_hashset.clear();
        // reserve space for the hashset
        if self.defect_vertices_hashset.capacity() < syndrome_pattern.defect_vertices.len() {
            self.defect_vertices_hashset.reserve(syndrome_pattern.defect_vertices.len() - self.defect_vertices_hashset.capacity())
        }
        if self.erasures_hashset.capacity() < syndrome_pattern.erasures.len() {
            self.erasures_hashset.reserve(syndrome_pattern.erasures.len() - self.erasures_hashset.capacity())
        }
        // add new syndrome
        for &vertex_index in syndrome_pattern.defect_vertices.iter() {
            self.defect_vertices_hashset.insert(vertex_index);
        }
        for &edge_index in syndrome_pattern.erasures.iter() {
            self.erasures_hashset.insert(edge_index);
        }
    }

    pub fn new_defects(model_graph: Arc<HyperModelGraph>, defect_vertices: Vec<VertexIndex>) -> Self {
        Self::new(model_graph, Arc::new(SyndromePattern::new_vertices(defect_vertices)))
    }

    pub fn find_valid_subgraph(&self, edges: &BTreeSet<EdgeIndex>, vertices: &BTreeSet<VertexIndex>) -> Option<Subgraph> {
        let mut matrix = ParityMatrix::new_no_phantom();
        for &edge_index in edges.iter() {
            matrix.add_tight_variable(edge_index);
        }
        for &vertex_index in vertices.iter() {
            matrix.add_parity_check_with_decoding_graph(vertex_index, self);
        }
        matrix.get_joint_solution()
    }

    pub fn find_valid_subgraph_auto_vertices(&self, edges: &BTreeSet<EdgeIndex>) -> Option<Subgraph> {
        self.find_valid_subgraph(edges, &self.get_edges_neighbors(edges))
    }

    pub fn is_valid_cluster(&self, edges: &BTreeSet<EdgeIndex>, vertices: &BTreeSet<VertexIndex>) -> bool {
        self.find_valid_subgraph(edges, vertices).is_some()
    }

    pub fn is_valid_cluster_auto_vertices(&self, edges: &BTreeSet<EdgeIndex>) -> bool {
        self.find_valid_subgraph_auto_vertices(edges).is_some()
    }

    pub fn is_vertex_defect(&self, vertex_index: VertexIndex) -> bool {
        self.defect_vertices_hashset.contains(&vertex_index)
    }

    pub fn get_edge_neighbors(&self, edge_index: EdgeIndex) -> &Vec<VertexIndex> {
        self.model_graph.get_edge_neighbors(edge_index)
    }

    pub fn get_vertex_neighbors(&self, vertex_index: VertexIndex) -> &Vec<EdgeIndex> {
        self.model_graph.get_vertex_neighbors(vertex_index)
    }

    pub fn get_edges_neighbors(&self, edges: &BTreeSet<EdgeIndex>) -> BTreeSet<VertexIndex> {
        self.model_graph.get_edges_neighbors(edges)
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
    pub vertices: BTreeSet<VertexIndex>,
    /// subset of edges
    pub edges: BTreeSet<EdgeIndex>,
    /// the hair of the invalid subgraph, to avoid repeated computation
    pub hairs: BTreeSet<EdgeIndex>,
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
        let mut invalid_subgraph = Self { hash: 0, vertices, edges, hairs, };
        debug_assert_eq!(invalid_subgraph.sanity_check(decoding_graph), Ok(()));
        invalid_subgraph.update_hash();
        invalid_subgraph
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
        if self.vertices.is_empty() {
            return Err("an invalid subgraph must contain at least one vertex".to_string());
        }
        // check if all vertices are valid
        for &vertex_index in self.vertices.iter() {
            if vertex_index >= decoding_graph.model_graph.initializer.vertex_num {
                return Err(format!("vertex {vertex_index} is not a vertex in the model graph"))
            }
        }
        // check if every edge is subset of its vertices
        for &edge_index in self.edges.iter() {
            if edge_index >= decoding_graph.model_graph.initializer.weighted_edges.len() {
                return Err(format!("edge {edge_index} is not an edge in the model graph"))
            }
            let (vertices, _weight) = &decoding_graph.model_graph.initializer.weighted_edges[edge_index];
            for &vertex_index in vertices.iter() {
                if !self.vertices.contains(&vertex_index) {
                    return Err(format!("hyperedge {edge_index} connects vertices {vertices:?}, but vertex {vertex_index} is not in the invalid subgraph vertices {:?}", self.vertices))
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
            return Err(format!("it's a valid subgraph because edges {:?} ⊆ {:?} can satisfy the parity requirement from vertices {:?}", matrix.get_joint_solution().unwrap(), self.edges, self.vertices))
        }
        Ok(())
    }

}

#[derive(Clone, Debug)]
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
        let relaxer = Self {
            direction,
            untighten_edges: untighten_edges,
        };
        debug_assert_eq!(relaxer.sanity_check(), Ok(()));
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
            return Err(format!("a valid relaxer must either increase overall ΔyS or untighten some edges"))
        }
        Ok(())
    }

}


#[cfg(test)]
mod tests {
    use super::*;
    use super::super::example_codes::*;

    fn color_code_5_model_graph() -> (Arc<HyperModelGraph>, Visualizer) {
        let visualize_filename = format!("framework_hyper_model_graph.json");
        let code = CodeCapacityColorCode::new(5, 0.1, 1000);
        let mut visualizer = Visualizer::new(Some(visualize_data_folder() + visualize_filename.as_str()), code.get_positions(), true).unwrap();
        print_visualize_link(visualize_filename.clone());
        visualizer.snapshot_combined(format!("code"), vec![&code]).unwrap();
        let model_graph = code.get_model_graph();
        (model_graph, visualizer)
    }

    #[test]
    fn framework_hyper_model_graph() {  // cargo test framework_hyper_model_graph -- --nocapture
        let (model_graph, ..) = color_code_5_model_graph();
        println!("model_graph: {model_graph:?}");
        let mut edge_reference_initializer = 0;
        let mut edge_reference_hyper_model_graph = 0;
        for (incident_vertices, _weight) in model_graph.initializer.weighted_edges.iter() {
            edge_reference_initializer += incident_vertices.len();
        }
        for vertex in model_graph.vertices.iter() {
            edge_reference_hyper_model_graph += vertex.edges.len();
        }
        assert_eq!(edge_reference_initializer, edge_reference_hyper_model_graph);
    }

    fn color_code_5_decoding_graph(defect_vertices: Vec<VertexIndex>) -> (Arc<HyperDecodingGraph>, Visualizer) {
        let (model_graph, mut visualizer) = color_code_5_model_graph();
        let syndrome_pattern = Arc::new(SyndromePattern::new_vertices(defect_vertices));
        let decoding_graph = Arc::new(HyperDecodingGraph::new(model_graph, syndrome_pattern));
        visualizer.snapshot_combined(format!("syndrome"), vec![decoding_graph.as_ref()]).unwrap();
        (decoding_graph, visualizer)
    }

    #[test]
    fn framework_invalid_subgraph() {  // cargo test framework_invalid_subgraph -- --nocapture
        let (decoding_graph, ..) = color_code_5_decoding_graph(vec![7, 1]);
        let invalid_subgraph_1 = InvalidSubgraph::new(vec![13].into_iter().collect(), decoding_graph.as_ref());
        println!("invalid_subgraph_1: {invalid_subgraph_1:?}");
        assert_eq!(invalid_subgraph_1.vertices, vec![2, 6, 7].into_iter().collect());
        assert_eq!(invalid_subgraph_1.edges, vec![13].into_iter().collect());
        assert_eq!(invalid_subgraph_1.hairs, vec![5, 6, 9, 10, 11, 12, 14, 15, 16, 17].into_iter().collect());
    }

    #[test]
    #[should_panic]
    fn framework_valid_subgraph() {  // cargo test framework_valid_subgraph -- --nocapture
        let (decoding_graph, ..) = color_code_5_decoding_graph(vec![7, 1]);
        let invalid_subgraph = InvalidSubgraph::new(vec![6, 10].into_iter().collect(), decoding_graph.as_ref());
        println!("invalid_subgraph: {invalid_subgraph:?}");  // should not print because it panics
    }

    #[test]
    fn framework_good_relaxer() {  // cargo test framework_good_relaxer -- --nocapture
        let (decoding_graph, ..) = color_code_5_decoding_graph(vec![7, 1]);
        let invalid_subgraph = Arc::new(InvalidSubgraph::new_complete(vec![7].into_iter().collect(), BTreeSet::new(), decoding_graph.as_ref()));
        use num_traits::One;
        let relaxer = Relaxer::new(vec![(invalid_subgraph, Rational::one())]);
        println!("relaxer: {relaxer:?}");
        assert!(relaxer.untighten_edges.is_empty());
    }

    #[test]
    #[should_panic]
    fn framework_bad_relaxer() {  // cargo test framework_bad_relaxer -- --nocapture
        let (decoding_graph, ..) = color_code_5_decoding_graph(vec![7, 1]);
        let invalid_subgraph = Arc::new(InvalidSubgraph::new_complete(vec![7].into_iter().collect(), BTreeSet::new(), decoding_graph.as_ref()));
        let relaxer: Relaxer = Relaxer::new(vec![(invalid_subgraph, Rational::zero())]);
        println!("relaxer: {relaxer:?}");  // should not print because it panics
    }

}
