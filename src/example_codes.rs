//! Example Decoding
//! 
//! This module contains several abstract decoding hypergraph and it's randomized simulator utilities.
//! This helps to debug, but it doesn't corresponds to real noise model, nor it's capable of simulating circuit-level noise model.
//! For complex noise model and simulator functionality, please see <https://github.com/yuewuo/QEC-Playground>
//! 
//! Note that these examples are not optimized for cache for simplicity.
//! To maximize code efficiency, user should design how to group vertices such that memory speed is constant for arbitrary large code distance.
//! 

use super::visualize::*;
use super::util::*;
use std::collections::HashMap;
use crate::serde_json;
use crate::rand_xoshiro::rand_core::SeedableRng;
use crate::derivative::Derivative;
use std::fs::File;
use std::io::{self, BufRead};
#[cfg(feature="python_binding")]
use pyo3::prelude::*;


/// Vertex corresponds to a stabilizer measurement bit
#[derive(Derivative, Clone)]
#[derivative(Debug)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct CodeVertex {
    /// position helps to visualize
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub position: VisualizePosition,
    /// neighbor edges helps to set find individual edge
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub neighbor_edges: Vec<EdgeIndex>,
    /// whether it's a defect
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub is_defect: bool,
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl CodeVertex {
    #[cfg(feature = "python_binding")]
    fn __repr__(&self) -> String { format!("{:?}", self) }
}

/// Edge flips the measurement result of two vertices
#[derive(Derivative, Clone)]
#[derivative(Debug)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct CodeEdge {
    /// the two vertices incident to this edge; in quantum LDPC codes this should be only a handful of vertices
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub vertices: Vec<VertexIndex>,
    /// probability of flipping the results of these vertices; do not set p to 0 to remove edge: if desired, create a new code type without those edges
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub p: f64,
    /// probability of having a reported event of error on this edge (aka erasure errors)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub pe: f64,
    /// the integer weight of this edge
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub half_weight: Weight,
    /// whether this edge is erased
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub is_erasure: bool,
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl CodeEdge {
    #[cfg_attr(feature = "python_binding", new)]
    pub fn new(vertices: Vec<VertexIndex>) -> Self {
        Self {
            vertices: vertices,
            p: 0.,
            pe: 0.,
            half_weight: 0,
            is_erasure: false,
        }
    }
    #[cfg(feature = "python_binding")]
    fn __repr__(&self) -> String { format!("{:?}", self) }
}

/// default function for computing (pre-scaled) weight from probability
#[cfg_attr(feature = "python_binding", pyfunction)]
pub fn weight_of_p(p: f64) -> f64 {
    assert!((0. ..=0.5).contains(&p), "p must be a reasonable value between 0 and 50%");
    ((1. - p) / p).ln()
}

pub trait ExampleCode {

    /// get mutable references to vertices and edges
    fn vertices_edges(&mut self) -> (&mut Vec<CodeVertex>, &mut Vec<CodeEdge>);
    fn immutable_vertices_edges(&self) -> (&Vec<CodeVertex>, &Vec<CodeEdge>);

    /// get the number of vertices
    fn vertex_num(&self) -> VertexNum { self.immutable_vertices_edges().0.len() as VertexNum }

    /// generic method that automatically computes integer weights from probabilities,
    /// scales such that the maximum integer weight is 10000 and the minimum is 1
    fn compute_weights(&mut self, max_half_weight: Weight) {
        let (_vertices, edges) = self.vertices_edges();
        let mut max_weight = 0.;
        for edge in edges.iter() {
            let weight = weight_of_p(edge.p);
            if weight > max_weight {
                max_weight = weight;
            }
        }
        assert!(max_weight > 0., "max weight is not expected to be 0.");
        // scale all weights but set the smallest to 1
        for edge in edges.iter_mut() {
            let weight = weight_of_p(edge.p);
            let half_weight: Weight = ((max_half_weight as f64) * weight / max_weight).round() as Weight;
            edge.half_weight = if half_weight == 0 { 1 } else { half_weight };  // weight is required to be even
        }
    }

    /// sanity check to avoid duplicate edges that are hard to debug
    fn sanity_check(&self) -> Result<(), String> {
        let (vertices, edges) = self.immutable_vertices_edges();
        // check the graph is reasonable
        if vertices.is_empty() || edges.is_empty() {
            return Err("empty graph".to_string());
        }
        // check duplicated edges
        let mut existing_edges = HashMap::<Vec<VertexIndex>, EdgeIndex>::with_capacity(edges.len() * 2);
        for (edge_idx, edge) in edges.iter().enumerate() {
            let mut vertices = edge.vertices.clone();
            if vertices.len() < 1 {
                return Err(format!("empty hyperedge {}", edge_idx));
            }
            vertices.sort();
            let length_before_dedup = vertices.len();
            vertices.dedup();
            if vertices.len() != length_before_dedup {
                return Err(format!("edge {} contains duplicate vertices, after dedup it's {:?}", edge_idx, vertices));
            }
            if existing_edges.contains_key(&vertices) {
                let previous_idx = existing_edges[&vertices];
                return Err(format!("duplicate edge {} and {} with incident vertices {:?}", previous_idx, edge_idx, vertices));
            }
            existing_edges.insert(vertices, edge_idx as EdgeIndex);
        }
        // check duplicated referenced edge from each vertex
        for (vertex_idx, vertex) in vertices.iter().enumerate() {
            let mut existing_edges = HashMap::<EdgeIndex, ()>::new();
            if vertex.neighbor_edges.is_empty() {
                return Err(format!("vertex {} do not have any neighbor edges", vertex_idx));
            }
            for edge_idx in vertex.neighbor_edges.iter() {
                if existing_edges.contains_key(edge_idx) {
                    return Err(format!("duplicate referred edge {} from vertex {}", edge_idx, vertex_idx));
                }
                existing_edges.insert(*edge_idx, ());
            }
        }
        Ok(())
    }

    /// set probability of all edges; user can set individual probabilities
    fn set_probability(&mut self, p: f64) {
        let (_vertices, edges) = self.vertices_edges();
        for edge in edges.iter_mut() {
            edge.p = p;
        }
    }

    /// set erasure probability of all edges; user can set individual probabilities
    fn set_erasure_probability(&mut self, pe: f64) {
        let (_vertices, edges) = self.vertices_edges();
        for edge in edges.iter_mut() {
            edge.pe = pe;
        }
    }

    /// automatically create vertices given edges
    fn fill_vertices(&mut self, vertex_num: VertexNum) {
        let (vertices, edges) = self.vertices_edges();
        vertices.clear();
        vertices.reserve(vertex_num as usize);
        for _ in 0..vertex_num {
            vertices.push(CodeVertex {
                position: VisualizePosition::new(0., 0., 0.),
                neighbor_edges: Vec::new(),
                is_defect: false,
            });
        }
        for (edge_idx, edge) in edges.iter().enumerate() {
            for vertex_index in edge.vertices.iter() {
                let vertex = &mut vertices[*vertex_index];
                vertex.neighbor_edges.push(edge_idx as EdgeIndex);
            }
        }
    }

    /// gather all positions of vertices
    fn get_positions(&self) -> Vec<VisualizePosition> {
        let (vertices, _edges) = self.immutable_vertices_edges();
        let mut positions = Vec::with_capacity(vertices.len());
        for vertex in vertices.iter() {
            positions.push(vertex.position.clone());
        }
        positions
    }

    /// generate standard interface to instantiate MWPS solver
    fn get_initializer(&self) -> SolverInitializer {
        let (vertices, edges) = self.immutable_vertices_edges();
        let vertex_num = vertices.len() as VertexIndex;
        let mut weighted_edges = Vec::with_capacity(edges.len());
        for edge in edges.iter() {
            weighted_edges.push((edge.vertices.clone(), edge.half_weight * 2));
        }
        SolverInitializer {
            vertex_num,
            weighted_edges,
        }
    }

    /// set defect vertices (non-trivial measurement result in case of single round of measurement, 
    /// or different result from the previous round in case of multiple rounds of measurement)
    fn set_defect_vertices(&mut self, defect_vertices: &[VertexIndex]) {
        let (vertices, _edges) = self.vertices_edges();
        for vertex in vertices.iter_mut() {
            vertex.is_defect = false;
        }
        for vertex_idx in defect_vertices.iter() {
            let vertex = &mut vertices[*vertex_idx as usize];
            vertex.is_defect = true;
        }
    }

    /// set erasure edges
    fn set_erasures(&mut self, erasures: &[EdgeIndex]) {
        let (_vertices, edges) = self.vertices_edges();
        for edge in edges.iter_mut() {
            edge.is_erasure = false;
        }
        for edge_idx in erasures.iter() {
            let edge = &mut edges[*edge_idx as usize];
            edge.is_erasure = true;
        }
    }

    /// set syndrome
    fn set_syndrome(&mut self, syndrome_pattern: &SyndromePattern) {
        self.set_defect_vertices(&syndrome_pattern.defect_vertices);
        self.set_erasures(&syndrome_pattern.erasures);
    }

    /// get current defect vertices
    fn get_defect_vertices(&self) -> Vec<VertexIndex> {
        let (vertices, _edges) = self.immutable_vertices_edges();
        let mut syndrome = Vec::new();
        for (vertex_idx, vertex) in vertices.iter().enumerate() {
            if vertex.is_defect {
                syndrome.push(vertex_idx as VertexIndex);
            }
        }
        syndrome
    }

    /// get current erasure edges
    fn get_erasures(&self) -> Vec<EdgeIndex> {
        let (_vertices, edges) = self.immutable_vertices_edges();
        let mut erasures = Vec::new();
        for (edge_idx, edge) in edges.iter().enumerate() {
            if edge.is_erasure {
                erasures.push(edge_idx as EdgeIndex);
            }
        }
        erasures
    }

    /// get current syndrome
    fn get_syndrome(&self) -> SyndromePattern {
        SyndromePattern::new(self.get_defect_vertices(), self.get_erasures())
    }

    /// generate random errors based on the edge probabilities and a seed for pseudo number generator
    fn generate_random_errors(&mut self, seed: u64) -> SyndromePattern {
        let mut rng = DeterministicRng::seed_from_u64(seed);
        let (vertices, edges) = self.vertices_edges();
        for vertex in vertices.iter_mut() {
            vertex.is_defect = false;
        }
        for edge in edges.iter_mut() {
            let p = if rng.next_f64() < edge.pe {
                edge.is_erasure = true;
                0.5  // when erasure happens, there are 50% chance of error
            } else {
                edge.is_erasure = false;
                edge.p
            };
            if rng.next_f64() < p {
                for vertex_index in edge.vertices.iter() {
                    let vertex = &mut vertices[*vertex_index];
                    vertex.is_defect = !vertex.is_defect;
                }
            }
        }
        self.get_syndrome()
    }

    fn is_defect(&self, vertex_idx: usize) -> bool {
        let (vertices, _edges) = self.immutable_vertices_edges();
        vertices[vertex_idx].is_defect
    }

}

#[cfg(feature="python_binding")]
use rand::{Rng, thread_rng};

#[cfg(feature="python_binding")]
macro_rules! bind_trait_example_code {
    ($struct_name:ident) => {
        #[pymethods]
        impl $struct_name {
            fn __repr__(&self) -> String { format!("{:?}", self) }
            #[pyo3(name = "vertex_num")]
            fn trait_vertex_num(&self) -> VertexNum { self.vertex_num() }
            #[pyo3(name = "compute_weights")]
            fn trait_compute_weights(&mut self, max_half_weight: Weight) { self.compute_weights(max_half_weight) }
            #[pyo3(name = "sanity_check")]
            fn trait_sanity_check(&self) -> Option<String> { self.sanity_check().err() }
            #[pyo3(name = "set_probability")]
            fn trait_set_probability(&mut self, p: f64) { self.set_probability(p) }
            #[pyo3(name = "set_erasure_probability")]
            fn trait_set_erasure_probability(&mut self, p: f64) { self.set_erasure_probability(p) }
            #[pyo3(name = "fill_vertices")]
            fn trait_fill_vertices(&mut self, vertex_num: VertexNum) { self.fill_vertices(vertex_num) }
            #[pyo3(name = "get_positions")]
            fn trait_get_positions(&self) -> Vec<VisualizePosition> { self.get_positions() }
            #[pyo3(name = "get_initializer")]
            fn trait_get_initializer(&self) -> SolverInitializer { self.get_initializer() }
            #[pyo3(name = "set_defect_vertices")]
            fn trait_set_defect_vertices(&mut self, defect_vertices: Vec<VertexIndex>) { self.set_defect_vertices(&defect_vertices) }
            #[pyo3(name = "set_erasures")]
            fn trait_set_erasures(&mut self, erasures: Vec<EdgeIndex>) { self.set_erasures(&erasures) }
            #[pyo3(name = "set_syndrome")]
            fn trait_set_syndrome(&mut self, syndrome_pattern: &SyndromePattern) { self.set_syndrome(syndrome_pattern) }
            #[pyo3(name = "get_defect_vertices")]
            fn trait_get_defect_vertices(&self) -> Vec<VertexIndex> { self.get_defect_vertices() }
            #[pyo3(name = "get_erasures")]
            fn trait_get_erasures(&self) -> Vec<EdgeIndex> { self.get_erasures() }
            #[pyo3(name = "get_syndrome")]
            fn trait_get_syndrome(&self) -> SyndromePattern { self.get_syndrome() }
            #[pyo3(name = "generate_random_errors")]
            #[args(seed = "thread_rng().gen()")]
            fn trait_generate_random_errors(&mut self, seed: u64) -> SyndromePattern { self.generate_random_errors(seed) }
            #[pyo3(name = "is_virtual")]
            fn trait_is_virtual(&mut self, vertex_idx: usize) -> bool { self.is_virtual(vertex_idx) }
            #[pyo3(name = "is_defect")]
            fn trait_is_defect(&mut self, vertex_idx: usize) -> bool { self.is_defect(vertex_idx) }
            #[pyo3(name = "reorder_vertices")]
            fn trait_reorder_vertices(&mut self, sequential_vertices: Vec<VertexIndex>) { self.reorder_vertices(&sequential_vertices) }
            #[pyo3(name = "snapshot")]
            #[args(abbrev = "true")]
            fn trait_snapshot(&mut self, abbrev: bool) -> PyObject { json_to_pyobject(self.snapshot(abbrev)) }
        }
    };
}

impl<T> MWPSVisualizer for T where T: ExampleCode {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        let (self_vertices, self_edges) = self.immutable_vertices_edges();
        let mut vertices = Vec::<serde_json::Value>::new();
        for vertex in self_vertices.iter() {
            vertices.push(json!({
                if abbrev { "s" } else { "is_defect" }: i32::from(vertex.is_defect),
            }));
        }
        let mut edges = Vec::<serde_json::Value>::new();
        for edge in self_edges.iter() {
            edges.push(json!({
                if abbrev { "w" } else { "weight" }: edge.half_weight * 2,
                if abbrev { "v" } else { "vertices" }: edge.vertices,
            }));
        }
        json!({
            "vertices": vertices,
            "edges": edges,
        })
    }
}

/// perfect quantum repetition code
#[derive(Clone, Debug)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct CodeCapacityRepetitionCode {
    /// vertices in the code
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub vertices: Vec<CodeVertex>,
    /// nearest-neighbor edges in the decoding graph
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub edges: Vec<CodeEdge>,
}

impl ExampleCode for CodeCapacityRepetitionCode {
    fn vertices_edges(&mut self) -> (&mut Vec<CodeVertex>, &mut Vec<CodeEdge>) { (&mut self.vertices, &mut self.edges) }
    fn immutable_vertices_edges(&self) -> (&Vec<CodeVertex>, &Vec<CodeEdge>) { (&self.vertices, &self.edges) }
}

#[cfg(feature="python_binding")]
bind_trait_example_code!{CodeCapacityRepetitionCode}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl CodeCapacityRepetitionCode {

    #[cfg_attr(feature = "python_binding", new)]
    #[cfg_attr(feature = "python_binding", args(max_half_weight = "500"))]
    pub fn new(d: VertexNum, p: f64, max_half_weight: Weight) -> Self {
        let mut code = Self::create_code(d);
        code.set_probability(p);
        code.compute_weights(max_half_weight);
        code
    }

    #[cfg_attr(feature = "python_binding", staticmethod)]
    pub fn create_code(d: VertexNum) -> Self {
        assert!(d >= 3 && d % 2 == 1, "d must be odd integer >= 3");
        let vertex_num = d - 1;
        // create edges
        let mut edges = Vec::new();
        for i in 0..d-2 {
            edges.push(CodeEdge::new(vec![i, i+1]));
        }
        edges.push(CodeEdge::new(vec![0]));  // the left-most edge
        edges.push(CodeEdge::new(vec![d-2]));  // the right-most edge
        let mut code = Self {
            vertices: Vec::new(),
            edges,
        };
        // create vertices
        code.fill_vertices(vertex_num);
        let mut positions = Vec::new();
        for i in 0..d-1 {
            positions.push(VisualizePosition::new(0., i as f64, 0.));
        }
        for (i, position) in positions.into_iter().enumerate() {
            code.vertices[i].position = position;
        }
        code
    }

}

/// code capacity noise model is a single measurement round with perfect stabilizer measurements;
/// e.g. this is the decoding graph of a CSS surface code (standard one, not rotated one) with X-type stabilizers
#[derive(Clone, Debug)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct CodeCapacityPlanarCode {
    /// vertices in the code
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub vertices: Vec<CodeVertex>,
    /// nearest-neighbor edges in the decoding graph
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub edges: Vec<CodeEdge>,
}

impl ExampleCode for CodeCapacityPlanarCode {
    fn vertices_edges(&mut self) -> (&mut Vec<CodeVertex>, &mut Vec<CodeEdge>) { (&mut self.vertices, &mut self.edges) }
    fn immutable_vertices_edges(&self) -> (&Vec<CodeVertex>, &Vec<CodeEdge>) { (&self.vertices, &self.edges) }
}

#[cfg(feature="python_binding")]
bind_trait_example_code!{CodeCapacityPlanarCode}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl CodeCapacityPlanarCode {

    #[cfg_attr(feature = "python_binding", new)]
    #[cfg_attr(feature = "python_binding", args(max_half_weight = "500"))]
    pub fn new(d: VertexNum, p: f64, max_half_weight: Weight) -> Self {
        let mut code = Self::create_code(d);
        code.set_probability(p);
        code.compute_weights(max_half_weight);
        code
    }

    #[cfg_attr(feature = "python_binding", staticmethod)]
    pub fn create_code(d: VertexNum) -> Self {
        assert!(d >= 3 && d % 2 == 1, "d must be odd integer >= 3");
        let row_vertex_num = d-1;
        let vertex_num = row_vertex_num * d;  // `d` rows
        // create edges
        let mut edges = Vec::new();
        for row in 0..d {
            let bias = row * row_vertex_num;
            for i in 0..d-2 {
                edges.push(CodeEdge::new(vec![bias + i, bias + i + 1]));
            }
            edges.push(CodeEdge::new(vec![bias]));  // the left-most edge
            edges.push(CodeEdge::new(vec![bias + d - 2]));  // the right-most edge
            if row + 1 < d {
                for i in 0..d-1 {
                    edges.push(CodeEdge::new(vec![bias + i, bias + i + row_vertex_num]));
                }
            }
        }
        let mut code = Self {
            vertices: Vec::new(),
            edges,
        };
        // create vertices
        code.fill_vertices(vertex_num);
        let mut positions = Vec::new();
        for row in 0..d {
            for i in 0..row_vertex_num {
                positions.push(VisualizePosition::new(row as f64, i as f64, 0.));
            }
        }
        for (i, position) in positions.into_iter().enumerate() {
            code.vertices[i].position = position;
        }
        code
    }

}

/// read from file, including the error patterns;
/// the point is to avoid bad cache performance, because generating random error requires iterating over a large memory space,
/// invalidating all cache. also, this can reduce the time of decoding by prepare the data before hand and could be shared between
/// different partition configurations
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct ErrorPatternReader {
    /// vertices in the code
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub vertices: Vec<CodeVertex>,
    /// nearest-neighbor edges in the decoding graph
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub edges: Vec<CodeEdge>,
    /// pre-generated syndrome patterns
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub syndrome_patterns: Vec<SyndromePattern>,
    /// cursor of current syndrome
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub syndrome_index: usize,
}

impl ExampleCode for ErrorPatternReader {
    fn vertices_edges(&mut self) -> (&mut Vec<CodeVertex>, &mut Vec<CodeEdge>) { (&mut self.vertices, &mut self.edges) }
    fn immutable_vertices_edges(&self) -> (&Vec<CodeVertex>, &Vec<CodeEdge>) { (&self.vertices, &self.edges) }
    fn generate_random_errors(&mut self, _seed: u64) -> SyndromePattern {
        assert!(self.syndrome_index < self.syndrome_patterns.len(), "reading syndrome pattern more than in the file, consider generate the file with more data points");
        let syndrome_pattern = self.syndrome_patterns[self.syndrome_index].clone();
        self.syndrome_index += 1;
        syndrome_pattern
    }
}

impl ErrorPatternReader {

    pub fn new(mut config: serde_json::Value) -> Self {
        let mut filename = "tmp/syndrome_patterns.txt".to_string();
        let config = config.as_object_mut().expect("config must be JSON object");
        if let Some(value) = config.remove("filename") {
            filename = value.as_str().expect("filename string").to_string();
        }
        if !config.is_empty() { panic!("unknown config keys: {:?}", config.keys().collect::<Vec<&String>>()); }
        let file = File::open(filename).unwrap();
        let mut syndrome_patterns = vec![];
        let mut initializer: Option<SolverInitializer> = None;
        let mut positions: Option<Vec<VisualizePosition>> = None;
        for (line_index, line) in io::BufReader::new(file).lines().enumerate() {
            if let Ok(value) = line {
                match line_index {
                    0 => {
                        assert!(value.starts_with("Syndrome Pattern v1.0 "), "incompatible file version");
                    },
                    1 => {
                        initializer = Some(serde_json::from_str(&value).unwrap());
                    },
                    2 => {
                        positions = Some(serde_json::from_str(&value).unwrap());
                    },
                    _ => {
                        let syndrome_pattern: SyndromePattern = serde_json::from_str(&value).unwrap();
                        syndrome_patterns.push(syndrome_pattern);
                    }
                }
            }
        }
        let initializer = initializer.expect("initializer not present in file");
        let positions = positions.expect("positions not present in file");
        assert_eq!(positions.len(), initializer.vertex_num as usize);
        let mut code = Self {
            vertices: Vec::with_capacity(initializer.vertex_num as usize),
            edges: Vec::with_capacity(initializer.weighted_edges.len()),
            syndrome_patterns,
            syndrome_index: 0,
        };
        for (vertices, weight) in initializer.weighted_edges.iter() {
            assert!(weight % 2 == 0, "weight must be even number");
            code.edges.push(CodeEdge {
                vertices: vertices.clone(),
                p: 0.,  // doesn't matter
                pe: 0.,  // doesn't matter
                half_weight: weight / 2,
                is_erasure: false,  // doesn't matter
            });
        }
        // automatically create the vertices and nearest-neighbor connection
        code.fill_vertices(initializer.vertex_num);
        // set virtual vertices and positions
        for (vertex_index, position) in positions.into_iter().enumerate() {
            code.vertices[vertex_index].position = position;
        }
        code
    }

}

#[cfg(feature="python_binding")]
#[pyfunction]
pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<CodeVertex>()?;
    m.add_class::<CodeEdge>()?;
    m.add_function(wrap_pyfunction!(weight_of_p, m)?)?;
    m.add_class::<CodeCapacityRepetitionCode>()?;
    m.add_class::<CodeCapacityPlanarCode>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visualize_code(code: &mut impl ExampleCode, visualize_filename: String) {
        print_visualize_link(visualize_filename.clone());
        let mut visualizer = Visualizer::new(Some(visualize_data_folder() + visualize_filename.as_str()), code.get_positions(), true).unwrap();
        visualizer.snapshot(format!("code"), code).unwrap();
        for round in 0..3 {
            code.generate_random_errors(round);
            visualizer.snapshot(format!("syndrome {}", round + 1), code).unwrap();
        }
    }

    #[test]
    fn example_code_capacity_repetition_code() {  // cargo test example_code_capacity_repetition_code -- --nocapture
        let mut code = CodeCapacityRepetitionCode::new(7, 0.2, 500);
        code.sanity_check().unwrap();
        visualize_code(&mut code, format!("example_code_capacity_repetition_code.json"));
    }

    #[test]
    fn example_code_capacity_planar_code() {  // cargo test example_code_capacity_planar_code -- --nocapture
        let mut code = CodeCapacityPlanarCode::new(7, 0.1, 500);
        code.sanity_check().unwrap();
        visualize_code(&mut code, format!("example_code_capacity_planar_code.json"));
    }

}
