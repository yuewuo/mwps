use serde::{Serialize, Deserialize};
use crate::rand_xoshiro::rand_core::RngCore;
use crate::rand_xoshiro;
use crate::num_rational;

pub type Weight = i64;
pub type EdgeIndex = usize;
pub type VertexIndex = usize;
pub type NodeIndex = VertexIndex;
pub type DefectIndex = VertexIndex;
pub type VertexNodeIndex = VertexIndex;  // must be same as VertexIndex, NodeIndex, DefectIndex
pub type VertexNum = VertexIndex;
pub type NodeNum = VertexIndex;

pub type Rational = num_rational::BigRational;
// pub type Rational = num_rational::Rational64;

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverInitializer {
    /// the number of vertices
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub vertex_num: VertexNum,
    /// weighted edges, where vertex indices are within the range [0, vertex_num)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub weighted_edges: Vec<(Vec<VertexIndex>, Weight)>,
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl SolverInitializer {
    #[cfg_attr(feature = "python_binding", new)]
    pub fn new(vertex_num: VertexNum, weighted_edges: Vec<(Vec<VertexIndex>, Weight)>) -> SolverInitializer {
        SolverInitializer {
            vertex_num,
            weighted_edges,
        }
    }
    #[cfg(feature = "python_binding")]
    fn __repr__(&self) -> String { format!("{:?}", self) }
    /// sanity check to avoid duplicate edges that are hard to debug
    pub fn sanity_check(&self) -> Result<(), String> {
        use crate::example_codes::*;
        let mut code = ErrorPatternReader::from_initializer(self);
        code.sanity_check()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct SyndromePattern {
    /// the vertices corresponding to defect measurements
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub defect_vertices: Vec<VertexIndex>,
    /// the edges that experience erasures, i.e. known errors
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub erasures: Vec<EdgeIndex>,
}

impl SyndromePattern {
    pub fn new(defect_vertices: Vec<VertexIndex>, erasures: Vec<EdgeIndex>) -> Self {
        Self { defect_vertices, erasures }
    }
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl SyndromePattern {
    #[cfg_attr(feature = "python_binding", new)]
    #[cfg_attr(feature = "python_binding", args(defect_vertices="vec![]", erasures="vec![]"))]
    pub fn py_new(mut defect_vertices: Vec<VertexIndex>, erasures: Vec<EdgeIndex>, syndrome_vertices: Option<Vec<VertexIndex>>) -> Self {
        if let Some(syndrome_vertices) = syndrome_vertices {
            assert!(defect_vertices.is_empty(), "do not pass both `syndrome_vertices` and `defect_vertices` since they're aliasing");
            defect_vertices = syndrome_vertices;
        }
        Self { defect_vertices, erasures }
    }
    #[cfg_attr(feature = "python_binding", staticmethod)]
    pub fn new_vertices(defect_vertices: Vec<VertexIndex>) -> Self {
        Self::new(defect_vertices, vec![])
    }
    #[cfg_attr(feature = "python_binding", staticmethod)]
    pub fn new_empty() -> Self {
        Self::new(vec![], vec![])
    }
    #[cfg(feature = "python_binding")]
    fn __repr__(&self) -> String { format!("{:?}", self) }
}

#[allow(dead_code)]
/// use Xoshiro256StarStar for deterministic random number generator
pub type DeterministicRng = rand_xoshiro::Xoshiro256StarStar;

pub trait F64Rng {
    fn next_f64(&mut self) -> f64;
}

impl F64Rng for DeterministicRng {
    fn next_f64(&mut self) -> f64 {
        f64::from_bits(0x3FF << 52 | self.next_u64() >> 12) - 1.
    }
}

