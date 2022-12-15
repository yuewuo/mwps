use serde::{Serialize, Deserialize};
use crate::rand_xoshiro::rand_core::RngCore;
use crate::rand_xoshiro;
use crate::num_rational;
use crate::visualize::*;
use crate::num_traits::ToPrimitive;


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
        let code = ErrorPatternReader::from_initializer(self);
        code.sanity_check()
    }
    pub fn total_weight_subgraph(&self, subgraph: &Subgraph) -> Weight {
        let mut weight = 0;
        for &edge_index in subgraph.iter() {
            weight += self.weighted_edges[edge_index].1;
        }
        weight
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

/// the result of MWPS algorithm: a parity subgraph (defined by some edges that, 
/// if are selected, will generate the parity result in the syndrome)
pub struct Subgraph(Vec<EdgeIndex>);

impl Subgraph {
    pub fn new(edges: Vec<EdgeIndex>) -> Self {
        Self(edges)
    }
    pub fn new_empty() -> Self {
        Self(vec![])
    }
}

impl std::ops::Deref for Subgraph {
    type Target = Vec<EdgeIndex>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Subgraph {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Debug for Subgraph {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl MWPSVisualizer for Subgraph {
    fn snapshot(&self, _abbrev: bool) -> serde_json::Value {
        json!({
            "subgraph": self.0,
        })
    }
}

/// the range of the optimal MWPS solution's weight
pub struct WeightRange {
    pub lower: Rational,
    pub upper: Rational,
}

impl WeightRange {
    pub fn new(lower: Rational, upper: Rational) -> Self {
        Self { lower, upper }
    }
    /// a solution is optimal only if the range is a single point
    pub fn is_optimal(&self) -> bool {
        self.lower == self.upper
    }
}

impl MWPSVisualizer for WeightRange {
    fn snapshot(&self, _abbrev: bool) -> serde_json::Value {
        json!({
            "weight_range": {
                "lower": self.lower.to_f64(),
                "upper": self.upper.to_f64(),
                "ln": self.lower.numer().to_i64(),
                "ld": self.lower.denom().to_i64(),
                "un": self.upper.numer().to_i64(),
                "ud": self.upper.denom().to_i64(),
            },
        })
    }
}
