use weak_table::PtrWeakHashSet;

use crate::matrix::*;
use crate::model_hypergraph::*;
use crate::util::*;
use crate::visualize::*;
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak};

#[derive(Debug, Clone)]
pub struct DecodingHyperGraph {
    /// model graph
    pub model_graph: Arc<ModelHyperGraph>,
    /// syndrome
    pub syndrome_pattern: Arc<SyndromePattern>,
    /// fast check whether a vertex is defect
    pub defect_vertices_hashset: HashSet<VertexIndex>,
    /// fast check whether an edge is erased
    pub erasures_hashset: HashSet<EdgeIndex>,
}

impl DecodingHyperGraph {
    pub fn new(model_graph: Arc<ModelHyperGraph>, syndrome_pattern: Arc<SyndromePattern>) -> Self {
        let decoding_graph = Self {
            model_graph,
            syndrome_pattern: syndrome_pattern.clone(),
            defect_vertices_hashset: HashSet::new(),
            erasures_hashset: HashSet::new(),
        };
        // decoding_graph.set_syndrome(syndrome_pattern);
        decoding_graph
    }

    pub fn set_syndrome(&mut self, syndrome_pattern: Arc<SyndromePattern>) {
        self.defect_vertices_hashset.clear();
        self.erasures_hashset.clear();
        // reserve space for the hashset
        if self.defect_vertices_hashset.capacity() < syndrome_pattern.defect_vertices.len() {
            self.defect_vertices_hashset
                .reserve(syndrome_pattern.defect_vertices.len() - self.defect_vertices_hashset.capacity())
        }
        if self.erasures_hashset.capacity() < syndrome_pattern.erasures.len() {
            self.erasures_hashset
                .reserve(syndrome_pattern.erasures.len() - self.erasures_hashset.capacity())
        }
        // add new syndrome
        for &vertex_index in syndrome_pattern.defect_vertices.iter() {
            self.defect_vertices_hashset.insert(vertex_index);
        }
        for &edge_index in syndrome_pattern.erasures.iter() {
            self.erasures_hashset.insert(edge_index);
        }
    }

    pub fn new_defects(model_graph: Arc<ModelHyperGraph>, defect_vertices: Vec<VertexIndex>) -> Self {
        Self::new(model_graph, Arc::new(SyndromePattern::new_vertices(defect_vertices)))
    }

    // pub fn find_valid_subgraph(&self, edges: &BTreeSet<EdgePtr>, vertices: &BTreeSet<VertexPtr>) -> Option<Subgraph> {
    //     let mut matrix = Echelon::<CompleteMatrix>::new();
    //     for edge_index in edges.iter() {
    //         matrix.add_variable(edge_index.downgrade());
    //     }

    //     for vertex_index in vertices.iter() {
    //         // let incident_edges = self.get_vertex_neighbors(vertex_index);
    //         // let parity = self.is_vertex_defect(vertex_index);
    //         let incident_edges = &vertex_index.read_recursive().edges;
    //         let parity = vertex_index.read_recursive().is_defect;
    //         matrix.add_constraint(vertex_index.downgrade(), &incident_edges, parity);
    //     }
    //     matrix.get_solution()
    // }

    // pub fn find_valid_subgraph_auto_vertices(&self, edges: &BTreeSet<EdgePtr>) -> Option<Subgraph> {
    //     let mut vertices: BTreeSet<VertexPtr> = BTreeSet::new();
    //     for edge_ptr in edges.iter() {
    //         // let local_vertices = &edge_ptr.read_recursive().vertices;
    //         let local_vertices = &edge_ptr.get_vertex_neighbors();
    //         for vertex in local_vertices {
    //             vertices.insert(vertex.upgrade_force());
    //         }
    //     }

    //     self.find_valid_subgraph(edges, &vertices)
    // }

    // pub fn is_valid_cluster(&self, edges: &BTreeSet<EdgePtr>, vertices: &BTreeSet<VertexPtr>) -> bool {
    //     self.find_valid_subgraph(edges, vertices).is_some()
    // }

    // pub fn is_valid_cluster_auto_vertices(&self, edges: &BTreeSet<EdgePtr>) -> bool {
    //     self.find_valid_subgraph_auto_vertices(edges).is_some()
    // }

    // pub fn is_vertex_defect(&self, vertex_index: VertexIndex) -> bool {
    //     self.defect_vertices_hashset.contains(&vertex_index)
    // }

    // pub fn get_edge_neighbors(&self, edge_index: EdgeIndex) -> &Vec<VertexIndex> {
    //     self.model_graph.get_edge_neighbors(edge_index)
    // }

    // pub fn get_vertex_neighbors(&self, vertex_index: VertexIndex) -> &Vec<EdgeIndex> {
    //     self.model_graph.get_vertex_neighbors(vertex_index)
    // }

    // pub fn get_edges_neighbors(&self, edges: &BTreeSet<EdgeIndex>) -> BTreeSet<VertexIndex> {
    //     self.model_graph.get_edges_neighbors(edges)
    // }
}

impl MWPSVisualizer for DecodingHyperGraph {
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

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::model_hypergraph::tests::*;

    pub fn color_code_5_decoding_graph(
        defect_vertices: Vec<VertexIndex>,
        visualize_filename: String,
    ) -> (Arc<DecodingHyperGraph>, Visualizer) {
        let (model_graph, mut visualizer) = color_code_5_model_graph(visualize_filename);
        let syndrome_pattern = Arc::new(SyndromePattern::new_vertices(defect_vertices));
        let decoding_graph = Arc::new(DecodingHyperGraph::new(model_graph, syndrome_pattern));
        visualizer
            .snapshot_combined("syndrome".to_string(), vec![decoding_graph.as_ref()])
            .unwrap();
        (decoding_graph, visualizer)
    }

    #[test]
    fn hyper_decoding_graph_basic_1() {
        // cargo test hyper_decoding_graph_basic_1 -- --nocapture
        let visualize_filename = "hyper_decoding_graph_basic_1.json".to_string();
        let defect_vertices = vec![7, 1];
        let (decoding_graph, ..) = color_code_5_decoding_graph(defect_vertices, visualize_filename);
        println!("decoding_graph: {decoding_graph:?}");
    }
}
