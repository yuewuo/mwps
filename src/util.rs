use crate::mwpf_solver::*;
#[cfg(not(feature = "float_lp"))]
use crate::num_rational;
use crate::num_traits::ToPrimitive;
use crate::rand_xoshiro;
use crate::rand_xoshiro::rand_core::RngCore;
use crate::visualize::*;
use num_traits::Zero;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
#[cfg(feature = "python_binding")]
use pyo3::types::PyFloat;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::collections::HashMap;
use crate::pointers::{ArcRwLock, WeakRwLock};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::prelude::*;
use std::time::Instant;
use petgraph::Graph;
use petgraph::Undirected;
use std::sync::Arc;

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};

pub type Weight = usize; // only used as input, all internal weight representation will use `Rational`

cfg_if::cfg_if! {
    if #[cfg(feature="r64_weight")] {
        pub type Rational = num_rational::Rational64;
    } else if #[cfg(feature="float_lp")] {
        pub type Rational = crate::ordered_float::OrderedFloat;
    } else  {
        pub type Rational = num_rational::BigRational;
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature="u32_index")] {
        pub type EdgeIndex = u32;
        pub type VertexIndex = u32;
    } else {
        pub type EdgeIndex = usize;
        pub type VertexIndex = usize;
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature="unsafe_pointer")] {
        pub type KnownSafeRefCell<T> = ; // missing implementation
    } else {
        pub type KnownSafeRefCell<T> = std::cell::RefCell<T>;
    }
}

pub type NodeIndex = VertexIndex;
pub type DefectIndex = VertexIndex;
pub type VertexNodeIndex = VertexIndex; // must be same as VertexIndex, NodeIndex, DefectIndex
pub type VertexNum = VertexIndex;
pub type NodeNum = VertexIndex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct HyperEdge {
    /// the vertices incident to the hyperedge
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub vertices: Vec<VertexIndex>,
    /// the weight of the hyperedge
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub weight: Weight,
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl HyperEdge {
    #[cfg_attr(feature = "python_binding", new)]
    pub fn new(vertices: Vec<VertexIndex>, weight: Weight) -> Self {
        Self { vertices, weight }
    }

    #[cfg(feature = "python_binding")]
    fn __repr__(&self) -> String {
        format!("{:?}", self)
    }
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverInitializer {
    /// the number of vertices
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub vertex_num: VertexNum,
    /// weighted edges, where vertex indices are within the range [0, vertex_num)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub weighted_edges: Vec<HyperEdge>,
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl SolverInitializer {
    #[cfg_attr(feature = "python_binding", new)]
    pub fn new(vertex_num: VertexNum, weighted_edges: Vec<HyperEdge>) -> Self {
        Self {
            vertex_num,
            weighted_edges,
        }
    }

    #[cfg(feature = "python_binding")]
    fn __repr__(&self) -> String {
        format!("{:?}", self)
    }
}

impl SolverInitializer {
    /// sanity check to avoid duplicate edges that are hard to debug
    pub fn sanity_check(&self) -> Result<(), String> {
        use crate::example_codes::*;
        let code = ErrorPatternReader::from_initializer(self);
        code.sanity_check()
    }

    pub fn matches_subgraph_syndrome(&self, subgraph: &Subgraph, defect_vertices: &[VertexIndex]) -> bool {
        let subgraph_defect_vertices: Vec<_> = self.get_subgraph_syndrome(subgraph).into_iter().collect();
        let subgraph_vertices: std::collections::HashSet<_> = subgraph_defect_vertices.clone().into_iter().map(|v| v.read_recursive().vertex_index).collect();
        let defect_vertices_hash: std::collections::HashSet<_> = defect_vertices.to_vec().into_iter().collect();
        if subgraph_vertices == defect_vertices_hash {
            return true;
        } else {
            println!(
                "defect vertices: {:?}\nsubgraph_defect_vertices: {:?}",
                defect_vertices, subgraph_defect_vertices
            );
            return false;
        }
        // defect_vertices.sort();
        // if defect_vertices.len() != subgraph_defect_vertices.len() {
        //     println!(
        //         "defect vertices: {:?}\nsubgraph_defect_vertices: {:?}",
        //         defect_vertices, subgraph_defect_vertices
        //     );
        //     return false;
        // }
        // for i in 0..defect_vertices.len() {
        //     if defect_vertices[i] != subgraph_defect_vertices[i].upgradable_read().vertex_index {
        //         println!(
        //             "defect vertices: {:?}\nsubgraph_defect_vertices: {:?}",
        //             defect_vertices, subgraph_defect_vertices
        //         );
        //         return false;
        //     }
        // }
        // true
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn get_subgraph_total_weight(&self, subgraph: &Subgraph) -> Rational {
        let mut weight = Rational::zero();
        for edge_weak in subgraph.iter() {
            // weight += self.weighted_edges[edge_index as usize].weight;
            weight += edge_weak.upgrade_force().read_recursive().weight;
        }
        weight
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn get_subgraph_index_total_weight(&self, subgraph_index: &Vec<usize>) -> Rational {
        use crate::num_traits::FromPrimitive;
        let mut weight = Rational::zero();
        for &edge_index in subgraph_index.iter() {
            weight += Rational::from_usize(self.weighted_edges[edge_index as usize].weight).unwrap();
        }
        weight
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn get_subgraph_syndrome(&self, subgraph: &Subgraph) -> BTreeSet<VertexPtr> {
        let mut defect_vertices = BTreeSet::new();
        for edge_weak in subgraph.iter() {
            // let HyperEdge { vertices, .. } = &self.weighted_edges[edge_index as usize];
            let edge_ptr = edge_weak.upgrade_force();
            let edge = edge_ptr.read_recursive();
            let vertices = &edge.vertices;
            for vertex_weak in vertices.iter() {
                let vertex_ptr = vertex_weak.upgrade_force();
                if defect_vertices.contains(&vertex_ptr) {
                    defect_vertices.remove(&vertex_ptr);
                } else {
                    defect_vertices.insert(vertex_ptr);
                }
            }
        }
        defect_vertices
    }
}

impl MWPSVisualizer for SolverInitializer {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        let mut vertices = Vec::<serde_json::Value>::new();
        let mut edges = Vec::<serde_json::Value>::new();
        for _ in 0..self.vertex_num {
            vertices.push(json!({}));
        }
        for HyperEdge { vertices, weight } in self.weighted_edges.iter() {
            edges.push(json!({
                if abbrev { "w" } else { "weight" }: weight,
                if abbrev { "v" } else { "vertices" }: vertices,
            }));
        }
        json!({
            "vertices": vertices,
            "edges": edges,
        })
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
        Self {
            defect_vertices,
            erasures,
        }
    }
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl SyndromePattern {
    #[cfg_attr(feature = "python_binding", new)]
    #[cfg_attr(feature = "python_binding", pyo3(signature = (defect_vertices=vec![], erasures=vec![], syndrome_vertices=None)))]
    pub fn py_new(
        mut defect_vertices: Vec<VertexIndex>,
        erasures: Vec<EdgeIndex>,
        syndrome_vertices: Option<Vec<VertexIndex>>,
    ) -> Self {
        if let Some(syndrome_vertices) = syndrome_vertices {
            assert!(
                defect_vertices.is_empty(),
                "do not pass both `syndrome_vertices` and `defect_vertices` since they're aliasing"
            );
            defect_vertices = syndrome_vertices;
        }
        Self {
            defect_vertices,
            erasures,
        }
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
    fn __repr__(&self) -> String {
        format!("{:?}", self)
    }
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

/// the result of MWPF algorithm: a parity subgraph (defined by some edges that,
/// if are selected, will generate the parity result in the syndrome)
pub type Subgraph = Vec<EdgeWeak>;

impl MWPSVisualizer for Subgraph {
    fn snapshot(&self, _abbrev: bool) -> serde_json::Value {
        let subgraph_by_index: Vec<usize> = self.into_iter().map(|e| e.upgrade_force().read_recursive().edge_index).collect();
        json!({
            "subgraph": subgraph_by_index,
        })
    }
}

// https://stackoverflow.com/questions/76082775/return-a-python-object-defined-in-a-third-party-python-module-e-g-numpy-using
#[cfg(feature = "python_binding")]
pub fn rational_to_pyobject(value: &Rational) -> PyResult<Py<PyAny>> {
    Python::with_gil(|py| {
        if cfg!(feature = "float_lp") {
            PyResult::Ok(PyFloat::new(py, value.to_f64().unwrap()).into())
        } else {
            let frac = py.import("fractions")?;
            let numer = value.numer().clone();
            let denom = value.denom().clone();
            frac.call_method("Fraction", (numer, denom), None).map(Into::into)
        }
    })
}

/// the range of the optimal MWPF solution's weight
#[derive(Clone, Debug)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
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

#[cfg(feature = "python_binding")]
#[pymethods]
impl WeightRange {
    #[getter]
    fn lower(&self) -> PyResult<Py<PyAny>> {
        rational_to_pyobject(&self.lower)
    }

    #[getter]
    fn upper(&self) -> PyResult<Py<PyAny>> {
        rational_to_pyobject(&self.upper)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self)
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

/// record the decoding time of multiple syndrome patterns
pub struct BenchmarkProfiler {
    /// each record corresponds to a different syndrome pattern
    pub records: Vec<BenchmarkProfilerEntry>,
    /// summation of all decoding time
    pub sum_round_time: f64,
    /// syndrome count
    pub sum_syndrome: usize,
    /// error count
    pub sum_error: usize,
    /// noisy measurement round
    pub noisy_measurements: VertexNum,
    /// the file to output the profiler results
    pub benchmark_profiler_output: Option<File>,
}

impl BenchmarkProfiler {
    pub fn new(noisy_measurements: VertexNum, detail_log_file: Option<String>) -> Self {
        let benchmark_profiler_output = detail_log_file.map(|filename| {
            let mut file = File::create(filename).unwrap();
            file.write_all(
                serde_json::to_string(&json!({
                    "noisy_measurements": noisy_measurements,
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
            file.write_all(b"\n").unwrap();
            file
        });
        Self {
            records: vec![],
            sum_round_time: 0.,
            sum_syndrome: 0,
            sum_error: 0,
            noisy_measurements,
            benchmark_profiler_output,
        }
    }
    /// record the beginning of a decoding procedure
    pub fn begin(&mut self, syndrome_pattern: &SyndromePattern, error_pattern: &Vec<usize>) {
        // sanity check last entry, if exists, is complete
        if let Some(last_entry) = self.records.last() {
            assert!(
                last_entry.is_complete(),
                "the last benchmark profiler entry is not complete, make sure to call `begin` and `end` in pairs"
            );
        }
        let entry = BenchmarkProfilerEntry::new(syndrome_pattern, error_pattern);
        self.records.push(entry);
        self.records.last_mut().unwrap().record_begin();
    }
    pub fn event(&mut self, event_name: String) {
        let last_entry = self
            .records
            .last_mut()
            .expect("last entry not exists, call `begin` before `end`");
        last_entry.record_event(event_name);
    }
    /// record the ending of a decoding procedure
    pub fn end(&mut self, solver: Option<&dyn PrimalDualSolver>) {
        let last_entry = self
            .records
            .last_mut()
            .expect("last entry not exists, call `begin` before `end`");
        last_entry.record_end();
        self.sum_round_time += last_entry.round_time.unwrap();
        self.sum_syndrome += last_entry.syndrome_pattern.defect_vertices.len();
        self.sum_error += last_entry.error_pattern.len();
        if let Some(file) = self.benchmark_profiler_output.as_mut() {
            let mut events = serde_json::Map::new();
            for (event_name, time) in last_entry.events.iter() {
                events.insert(event_name.clone(), json!(time));
            }
            let mut value = json!({
                "round_time": last_entry.round_time.unwrap(),
                "defect_num": last_entry.syndrome_pattern.defect_vertices.len(),
                "error_num": last_entry.error_pattern.len(),
                "events": events,
            });
            if let Some(solver) = solver {
                let solver_profile = solver.generate_profiler_report();
                value
                    .as_object_mut()
                    .unwrap()
                    .insert("solver_profile".to_string(), solver_profile);
            }
            file.write_all(serde_json::to_string(&value).unwrap().as_bytes()).unwrap();
            file.write_all(b"\n").unwrap();
        }
    }
    /// print out a brief one-line statistics
    pub fn brief(&self) -> String {
        let total = self.sum_round_time / (self.records.len() as f64);
        let per_round = total / (1. + self.noisy_measurements as f64);
        let per_defect = self.sum_round_time / (self.sum_syndrome as f64);
        format!("total: {total:.3e}, round: {per_round:.3e}, syndrome: {per_defect:.3e},")
    }
}

pub struct BenchmarkProfilerEntry {
    /// the syndrome pattern of this decoding problem
    pub syndrome_pattern: SyndromePattern,
    /// the error pattern
    pub error_pattern: Vec<usize>,
    /// the time of beginning a decoding procedure
    begin_time: Option<Instant>,
    /// record additional events
    pub events: Vec<(String, f64)>,
    /// interval between calling [`Self::record_begin`] to calling [`Self::record_end`]
    pub round_time: Option<f64>,
}

impl BenchmarkProfilerEntry {
    pub fn new(syndrome_pattern: &SyndromePattern, error_pattern: &Vec<usize>) -> Self {
        Self {
            syndrome_pattern: syndrome_pattern.clone(),
            error_pattern: error_pattern.clone(),
            begin_time: None,
            events: vec![],
            round_time: None,
        }
    }
    /// record the beginning of a decoding procedure
    pub fn record_begin(&mut self) {
        assert_eq!(self.begin_time, None, "do not call `record_begin` twice on the same entry");
        self.begin_time = Some(Instant::now());
    }
    /// record the ending of a decoding procedure
    pub fn record_end(&mut self) {
        let begin_time = self
            .begin_time
            .as_ref()
            .expect("make sure to call `record_begin` before calling `record_end`");
        self.round_time = Some(begin_time.elapsed().as_secs_f64());
    }
    pub fn record_event(&mut self, event_name: String) {
        let begin_time = self
            .begin_time
            .as_ref()
            .expect("make sure to call `record_begin` before calling `record_end`");
        self.events.push((event_name, begin_time.elapsed().as_secs_f64()));
    }
    pub fn is_complete(&self) -> bool {
        self.round_time.is_some()
    }
}

#[cfg(feature = "python_binding")]
pub fn json_to_pyobject_locked(value: serde_json::Value, py: Python) -> PyObject {
    match value {
        serde_json::Value::Null => py.None(),
        serde_json::Value::Bool(value) => value.to_object(py),
        serde_json::Value::Number(value) => {
            if value.is_i64() {
                value.as_i64().to_object(py)
            } else {
                value.as_f64().to_object(py)
            }
        }
        serde_json::Value::String(value) => value.to_object(py),
        serde_json::Value::Array(array) => {
            let elements: Vec<PyObject> = array.into_iter().map(|value| json_to_pyobject_locked(value, py)).collect();
            pyo3::types::PyList::new(py, elements).into()
        }
        serde_json::Value::Object(map) => {
            let pydict = pyo3::types::PyDict::new(py);
            for (key, value) in map.into_iter() {
                let pyobject = json_to_pyobject_locked(value, py);
                pydict.set_item(key, pyobject).unwrap();
            }
            pydict.into()
        }
    }
}

#[cfg(feature = "python_binding")]
pub fn json_to_pyobject(value: serde_json::Value) -> PyObject {
    Python::with_gil(|py| json_to_pyobject_locked(value, py))
}

#[cfg(feature = "python_binding")]
pub fn pyobject_to_json_locked(value: PyObject, py: Python) -> serde_json::Value {
    let value: &PyAny = value.as_ref(py);
    if value.is_none() {
        serde_json::Value::Null
    } else if value.is_instance_of::<pyo3::types::PyBool>() {
        json!(value.extract::<bool>().unwrap())
    } else if value.is_instance_of::<pyo3::types::PyInt>() {
        json!(value.extract::<i64>().unwrap())
    } else if value.is_instance_of::<pyo3::types::PyFloat>() {
        json!(value.extract::<f64>().unwrap())
    } else if value.is_instance_of::<pyo3::types::PyString>() {
        json!(value.extract::<String>().unwrap())
    } else if value.is_instance_of::<pyo3::types::PyList>() {
        let elements: Vec<serde_json::Value> = value
            .extract::<Vec<PyObject>>()
            .unwrap()
            .into_iter()
            .map(|object| pyobject_to_json_locked(object, py))
            .collect();
        json!(elements)
    } else if value.is_instance_of::<pyo3::types::PyDict>() {
        let map: &pyo3::types::PyDict = value.downcast().unwrap();
        let mut json_map = serde_json::Map::new();
        for (key, value) in map.iter() {
            json_map.insert(
                key.extract::<String>().unwrap(),
                pyobject_to_json_locked(value.to_object(py), py),
            );
        }
        serde_json::Value::Object(json_map)
    } else {
        unimplemented!("unsupported python type, should be (cascaded) dict, list and basic numerical types")
    }
}

#[cfg(feature = "python_binding")]
pub fn pyobject_to_json(value: PyObject) -> serde_json::Value {
    Python::with_gil(|py| pyobject_to_json_locked(value, py))
}

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<SolverInitializer>()?;
    m.add_class::<SyndromePattern>()?;
    m.add_class::<HyperEdge>()?;
    Ok(())
}


/// for parallel implementation
/// 
/// an efficient representation of partitioned vertices and erasures when they're ordered
#[derive(Debug, Clone, Serialize)]

pub struct PartitionedSyndromePattern<'a> {
    /// the original syndrome pattern to be partitioned
    pub syndrome_pattern: &'a SyndromePattern,
    /// the defect range of this partition: it must be continuous if the defect vertices are ordered
    pub whole_defect_range: DefectRange,
}

impl<'a> PartitionedSyndromePattern<'a> {
    pub fn new(syndrome_pattern: &'a SyndromePattern) -> Self {
        assert!(
            syndrome_pattern.erasures.is_empty(),
            "erasure partition not supported yet;
        even if the edges in the erasure is well ordered, they may not be able to be represented as
        a single range simply because the partition is vertex-based. need more consideration"
        );
        Self {
            syndrome_pattern,
            whole_defect_range: DefectRange::new(0, syndrome_pattern.defect_vertices.len() as DefectIndex),
        }
    }
}

// ////////////////////////////////////////////////////////////////////////////////////////
// ////////////////////////////////////////////////////////////////////////////////////////
// /////////////// We implement the HashSet to specify vertices in set ////////////////////

// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// pub struct IndexSet {
//     // spaced-out individual index
//     pub individual_indices: BTreeSet<VertexNodeIndex>,
//     // indices that can be described using range, we assume that there is only one big range among all vertex indices
//     pub range: [VertexNodeIndex; 2],
// }

// // just to distinguish them in code, essentially nothing different
// pub type VertexSet = IndexSet;
// pub type DefectSet = IndexSet;
// pub type NodeSet = IndexSet;

// impl IndexSet {
//     // initialize a IndexSet that only has a continuous range of indices but no spaced out individual indices
//     fn new_range(start: VertexNodeIndex, end: VertexNodeIndex) -> Self {
//         debug_assert!(end > start, "invalid range [{}, {})", start, end);
//         Self { 
//             individual_indices: BTreeSet::<VertexNodeIndex>::new(),
//             range: [start, end], 
//         }
//     }

//     // initialize a IndexSet that only has spaced out individual indicies
//     fn new_individual_indices(indices: Vec<VertexNodeIndex>) -> Self {
//         let mut new_set = BTreeSet::<VertexNodeIndex>::new();
//         for index in indices {
//             new_set.insert(index);
//         }
//         Self {
//             individual_indices: new_set,
//             range: [0, 0],
//         }
//     }

//     // initialize a IndexSet that has both continuous range of indices and individual spaced out indices
//     pub fn new(start: VertexNodeIndex, end: VertexNodeIndex, indices: Vec<VertexNodeIndex>) -> Self {
//         debug_assert!(end > start, "invalid range [{}, {})", start, end);
//         if start == end && indices.len() == 0{
//             // range is invalid, we check whether indices are empty 
//             // indices are empty too
//             panic!("both the input range and individual indices are invalid");
//         } else if start == end {
//             return Self::new_individual_indices(indices);
//         } else if indices.len() == 0{
//             return Self::new_range(start, end);
//         } else {
//             let mut new_set = BTreeSet::<VertexNodeIndex>::new();
//             for index in indices {
//                 new_set.insert(index);
//             }

//             return Self {
//                 individual_indices: new_set,
//                 range: [start, end],
//             }
//         }
//     }

//     // add more individual index to the already created IndexSet 
//     pub fn add_individual_index(&mut self, index: VertexNodeIndex) {
//         self.individual_indices.insert(index);
//     }

//     pub fn new_range_by_length(start: VertexNodeIndex, length: VertexNodeIndex) -> Self {
//         Self::new_range(start, start + length)
//     }

//     pub fn is_empty(&self) -> bool {
//         self.range[1] == self.range[0] && self.individual_indices.is_empty()
//     }

//     #[allow(clippy::unnecessary_cast)]
//     pub fn len(&self) -> usize {
//         (self.range[1] - self.range[0] + self.individual_indices.len()) as usize
//     }
//     pub fn range_start(&self) -> VertexNodeIndex {
//         self.range[0]
//     }
//     pub fn range_end(&self) -> VertexNodeIndex {
//         self.range[1]
//     }
//     pub fn extend_range_by(&mut self, append_count: VertexNodeIndex) {
//         self.range[1] += append_count;
//     }
//     pub fn bias_by(&mut self, bias: VertexNodeIndex) {
//         self.range[0] += bias;
//         self.range[1] += bias;

//         let set = std::mem::replace(&mut self.individual_indices, BTreeSet::new());
//         self.individual_indices = set.into_iter()
//             .map(|p| p + bias)
//             .collect();
//     }
//     pub fn sanity_check(&self) {
//         assert!(self.range_start() <= self.range_end(), "invalid vertex range {:?}", self);
//     }
//     pub fn contains(&self, vertex_index: VertexNodeIndex) -> bool {
//         (vertex_index >= self.range_start() && vertex_index < self.range_end()) || self.individual_indices.contains(&vertex_index)
//     }
//     // /// fuse two ranges together, returning (the whole range, the interfacing range)
//     // pub fn fuse(&self, other: &Self) -> (Self, Self) {
//     //     self.sanity_check();
//     //     other.sanity_check();
//     //     assert!(self.range[1] <= other.range[0], "only lower range can fuse higher range");
//     //     (
//     //         Self::new(self.range[0], other.range[1]),
//     //         Self::new(self.range[1], other.range[0]),
//     //     )
//     // }
// }



// we leave the code here just in case we need to describe the vertices in continuos range
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct IndexRange {
    pub range: [VertexNodeIndex; 2],
}

// just to distinguish them in code, essentially nothing different
pub type VertexRange = IndexRange;
pub type DefectRange = IndexRange;
pub type NodeRange = IndexRange;
pub type EdgeRange = IndexRange;

impl IndexRange {
    pub fn new(start: VertexNodeIndex, end: VertexNodeIndex) -> Self {
        debug_assert!(end >= start, "invalid range [{}, {})", start, end);
        Self { range: [start, end] }
    }
    pub fn new_length(start: VertexNodeIndex, length: VertexNodeIndex) -> Self {
        Self::new(start, start + length)
    }
    pub fn is_empty(&self) -> bool {
        self.range[1] == self.range[0]
    }
    #[allow(clippy::unnecessary_cast)]
    pub fn len(&self) -> usize {
        (self.range[1] - self.range[0]) as usize
    }
    pub fn start(&self) -> VertexNodeIndex {
        self.range[0]
    }
    pub fn end(&self) -> VertexNodeIndex {
        self.range[1]
    }
    pub fn append_by(&mut self, append_count: VertexNodeIndex) {
        self.range[1] += append_count;
    }
    pub fn bias_by(&mut self, bias: VertexNodeIndex) {
        self.range[0] += bias;
        self.range[1] += bias;
    }
    pub fn sanity_check(&self) {
        assert!(self.start() <= self.end(), "invalid vertex range {:?}", self);
    }
    pub fn contains(&self, vertex_index: VertexNodeIndex) -> bool {
        vertex_index >= self.start() && vertex_index < self.end()
    }
    /// fuse two ranges together, returning (the whole range, the interfacing range)
    pub fn fuse(&self, other: &Self) -> (Self, Self) {
        self.sanity_check();
        other.sanity_check();
        assert!(self.range[1] <= other.range[0], "only lower range can fuse higher range");
        (
            Self::new(self.range[0], other.range[1]),
            Self::new(self.range[1], other.range[0]),
        )
    }
}

impl IndexRange {
    pub fn iter(&self) -> std::ops::Range<VertexNodeIndex> {
        self.range[0]..self.range[1]
    }
    pub fn contains_any(&self, vertex_indices: &[VertexNodeIndex]) -> bool {
        for vertex_index in vertex_indices.iter() {
            if self.contains(*vertex_index) {
                return true;
            }
        }
        false
    }
}


impl Hash for IndexRange {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.range[0].hash(state);
        self.range[1].hash(state);
    }
}

// /// a general partition unit that could contain mirrored vertices
// #[derive(Debug, Clone)]
// pub struct PartitionUnit {
//     /// unit index
//     pub unit_index: usize,
// }

// pub type PartitionUnitPtr = ArcRwLock<PartitionUnit>;
// pub type PartitionUnitWeak = WeakRwLock<PartitionUnit>;

// impl std::fmt::Debug for PartitionUnitPtr {
//     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//         let partition_unit = self.read_recursive();
//         write!(
//             f,
//             "{}",
//             partition_unit.unit_index
//         )
//     }
// }

// impl std::fmt::Debug for PartitionUnitWeak {
//     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//         self.upgrade_force().fmt(f)
//     }
// }

/// user input partition configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionConfig {
    /// the number of vertices
    pub vertex_num: VertexNum,
    /// detailed plan of partitioning serial modules: each serial module possesses a list of vertices, including all interface vertices
    pub partitions: Vec<VertexRange>,
    /// detailed plan of interfacing vertices
    pub fusions: Vec<(usize, usize)>,
    /// undirected acyclic graph (DAG) to keep track of the relationship between different partition units
    pub dag_partition_units: Graph::<(), bool, Undirected>,
    /// defect vertices (global index)
    pub defect_vertices: BTreeSet<usize>,
}

impl PartitionConfig {
    pub fn new(vertex_num: VertexNum) -> Self {
        Self {
            vertex_num,
            partitions: vec![VertexRange::new(0, vertex_num as VertexIndex)],
            fusions: vec![],
            dag_partition_units: Graph::new_undirected(),
            defect_vertices: BTreeSet::new(),
        }
    }

    /// the partition below relies on the fact that the vertices' indices are continuous
    #[allow(clippy::unnecessary_cast)]
    pub fn info(&self) -> PartitionInfo {
        assert!(!self.partitions.is_empty(), "at least one partition must exist");
        let mut owning_ranges = vec![];
        let unit_count = self.partitions.len() + self.fusions.len();
        let partitions_len = self.partitions.len();

        for &partition in self.partitions.iter() {
            partition.sanity_check();
            assert!(
                partition.end() <= self.vertex_num as VertexIndex,
                "invalid vertex index {} in partitions",
                partition.end()
            );
            owning_ranges.push(partition);
        }

        // find boundary vertices
        let mut interface_ranges = vec![];
        let mut unit_index_to_adjacent_indices: HashMap<usize, Vec<usize>> = HashMap::new();
        
        for (boundary_unit_index, (left_index, right_index)) in self.fusions.iter().enumerate() {
            let boundary_unit_index = boundary_unit_index + partitions_len;
            // find the interface_range 
            let (_whole_range, interface_range) = self.partitions[*left_index].fuse(&self.partitions[*right_index]);
            interface_ranges.push(interface_range);
            owning_ranges.push(interface_range);
            if let Some(adjacent_indices) = unit_index_to_adjacent_indices.get_mut(left_index) {
                adjacent_indices.push(boundary_unit_index);
            } else {
                let mut adjacent_indices = vec![];
                adjacent_indices.push(boundary_unit_index);
                unit_index_to_adjacent_indices.insert(*left_index, adjacent_indices.clone());
            }
            
            if let Some(adjacent_indices) = unit_index_to_adjacent_indices.get_mut(right_index) {
                adjacent_indices.push(boundary_unit_index);
            } else {
                let mut adjacent_indices = vec![];
                adjacent_indices.push(boundary_unit_index);
                unit_index_to_adjacent_indices.insert(*right_index, adjacent_indices.clone());
            }
            
            // now we insert the key-value pair for boundary_unit_index and its adjacent 
            if let Some(adjacent_indices) = unit_index_to_adjacent_indices.get_mut(&boundary_unit_index) {
                adjacent_indices.push(*left_index);
                adjacent_indices.push(*right_index);
            } else {
                let mut adjacent_indices = vec![];
                adjacent_indices.push(*left_index);
                adjacent_indices.push(*right_index);
                unit_index_to_adjacent_indices.insert(boundary_unit_index, adjacent_indices.clone());
            }
        }
       
        let mut boundary_vertices: HashMap<usize, Vec<IndexRange>> = HashMap::new();
        for (unit_index, adjacent_unit_indices) in unit_index_to_adjacent_indices.iter() {
            if let Some(adjacent_vertices) = boundary_vertices.get_mut(&unit_index) {
                for adjacent_unit_index in adjacent_unit_indices {
                    adjacent_vertices.push(owning_ranges[*adjacent_unit_index]);
                }
            } else {
                let mut adjacent_vertices = vec![];
                for adjacent_unit_index in adjacent_unit_indices {
                    adjacent_vertices.push(owning_ranges[*adjacent_unit_index]);
                }
                boundary_vertices.insert(*unit_index, adjacent_vertices.clone());
            }
        }

        // construct partition info, assuming partition along the time axis
        let partition_unit_info: Vec<_> = (0..unit_count)
            .map(|i| PartitionUnitInfo {
                // owning_range: if i == self.partitions.len() - 1 {
                //     owning_ranges[i]
                // }else {
                //     IndexRange::new(owning_ranges[i].start(), interface_ranges[i].end())  // owning_ranges[i], 
                // },
                owning_range: owning_ranges[i],
                unit_index: i,
                is_boundary_unit: if i < partitions_len {false} else {true},
                adjacent_parallel_units: unit_index_to_adjacent_indices.get(&i).unwrap().clone(),
                boundary_vertices: boundary_vertices.get(&i).unwrap().clone(),
            })
            .collect();

        // create vertex_to_owning_unit for owning_ranges
        let mut vertex_to_owning_unit = HashMap::new();
        for partition_unit in partition_unit_info.iter() {
            // create vertex_to_owning_unit for owning_ranges
            for vertex_index in partition_unit.owning_range.iter() {
                vertex_to_owning_unit.insert(vertex_index, partition_unit.unit_index);
            }
        }

        PartitionInfo {
            config: self.clone(),
            units: partition_unit_info,
            vertex_to_owning_unit,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    /// the initial configuration that creates this info
    pub config: PartitionConfig,
    /// individual info of each unit
    pub units: Vec<PartitionUnitInfo>,
    /// the mapping from vertices to the owning unit: serial unit (holding real vertices) as well as parallel units (holding interfacing vertices);
    /// used for loading syndrome to the holding units
    pub vertex_to_owning_unit: HashMap<VertexIndex, usize>,
}

// impl PartitionInfo {
    /// split a sequence of syndrome into multiple parts, each corresponds to a unit;
    /// this is a slow method and should only be used when the syndrome pattern is not well-ordered
    // #[allow(clippy::unnecessary_cast)]
    // pub fn partition_syndrome_unordered(&self, syndrome_pattern: &SyndromePattern) -> Vec<SyndromePattern> {
    //     let mut partitioned_syndrome: Vec<_> = (0..self.units.len()).map(|_| SyndromePattern::new_empty()).collect();
    //     for defect_vertex in syndrome_pattern.defect_vertices.iter() {
    //         let unit_index = self.vertex_to_owning_unit.get(defect_vertex);
    //         match unit_index {
    //             Some(unit_index) => partitioned_syndrome[*unit_index].defect_vertices.push(*defect_vertex),
    //             None => // the syndrome is on the boudnary vertices

    //         }
    //     }
    //     // TODO: partition edges
    //     partitioned_syndrome
    // }
// }

// for primal module parallel
impl<'a> PartitionedSyndromePattern<'a> {
    /// partition the syndrome pattern into 2 partitioned syndrome pattern and my whole range
    #[allow(clippy::unnecessary_cast)]
    pub fn partition(&self, partition_unit_info: &PartitionUnitInfo) -> (Self, (Self, Self)) {
        // first binary search the start of owning defect vertices
        let owning_start_index = {
            let mut left_index = self.whole_defect_range.start();
            let mut right_index = self.whole_defect_range.end();
            while left_index != right_index {
                let mid_index = (left_index + right_index) / 2;
                let mid_defect_vertex = self.syndrome_pattern.defect_vertices[mid_index as usize];
                if mid_defect_vertex < partition_unit_info.owning_range.start() {
                    left_index = mid_index + 1;
                } else {
                    right_index = mid_index;
                }
            }
            left_index
        };
        println!("start of owning defect vertice: {owning_start_index:?}");
        // second binary search the end of owning defect vertices
        let owning_end_index = {
            let mut left_index = self.whole_defect_range.start();
            let mut right_index = self.whole_defect_range.end();
            while left_index != right_index {
                let mid_index = (left_index + right_index) / 2;
                let mid_defect_vertex = self.syndrome_pattern.defect_vertices[mid_index as usize];
                if mid_defect_vertex < partition_unit_info.owning_range.end() {
                    left_index = mid_index + 1;
                } else {
                    right_index = mid_index;
                }
            }
            left_index
        };
        println!("end of owning defect vertice: {owning_end_index:?}");

        (
            Self {
                syndrome_pattern: self.syndrome_pattern,
                whole_defect_range: DefectRange::new(owning_start_index, owning_end_index),
            },
            (
                Self {
                    syndrome_pattern: self.syndrome_pattern,
                    whole_defect_range: DefectRange::new(self.whole_defect_range.start(), owning_start_index),
                },
                Self {
                    syndrome_pattern: self.syndrome_pattern,
                    whole_defect_range: DefectRange::new(owning_end_index, self.whole_defect_range.end()),
                },
            ),
        )
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn expand(&self) -> SyndromePattern {
        let mut defect_vertices = Vec::with_capacity(self.whole_defect_range.len());
        for defect_index in self.whole_defect_range.iter() {
            defect_vertices.push(self.syndrome_pattern.defect_vertices[defect_index as usize]);
        }
        SyndromePattern::new(defect_vertices, vec![])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionUnitInfo {
    /// the owning range of units, the vertices exlusive to this unit 
    pub owning_range: VertexRange,
    /// partition unit index
    pub unit_index: usize,
    /// if this unit is boundary unit
    pub is_boundary_unit: bool, 

    pub adjacent_parallel_units: Vec<usize>,
    
    /// the boundary vertices near to this unit
    pub boundary_vertices: Vec<IndexRange>,
    // /// boundary vertices, following the global vertex index
    // /// key: indexrange of the boundary vertices. value: (unit_index, unit_index), the pair of unit_index of the two partition units adjacent to the boundary
    // pub boundary_vertices: Option<HashMap<IndexRange, (usize, usize)>>,
    // /// adjacent PartitionUnits, vector of partition unit_index
    // pub adjacent_partition_units: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct PartitionedSolverInitializer {
    /// unit index
    pub unit_index: usize,
    /// the number of all vertices (including those partitioned into other serial modules)
    pub vertex_num: VertexNum,
    /// the number of all edges (including those partitioned into other serial modules)
    pub edge_num: usize,
    /// vertices exclusively owned by this partition; this part must be a continuous range
    pub owning_range: VertexRange,
    /// weighted edges, where the first vertex index is within the range [vertex_index_bias, vertex_index_bias + vertex_num) and
    /// the second is either in [vertex_index_bias, vertex_index_bias + vertex_num) or inside
    /// the second element in the tuple is the global edge index of the respective hyper_edge
    pub weighted_edges: Vec<(HyperEdge, usize)>,
    // /// (not sure whether we need it, just in case)
    pub boundary_vertices: Vec<IndexRange>,
    /// whether this unit is boundary-unit
    pub is_boundary_unit: bool,
    /// all defect vertices (global index), not just for this unit
    pub defect_vertices: BTreeSet<usize>,
    // /// (not sure whether we need it, just in case)
    // pub adjacent_partition_units: Vec<usize>,
    // /// applicable when all the owning vertices are partitioned (i.e. this belongs to a fusion unit)
    // pub owning_interface: Option<PartitionUnitWeak>,
}

/// perform index transformation
#[allow(clippy::unnecessary_cast)]
pub fn build_old_to_new(reordered_vertices: &Vec<VertexIndex>) -> Vec<Option<VertexIndex>> {
    let mut old_to_new: Vec<Option<VertexIndex>> = (0..reordered_vertices.len()).map(|_| None).collect();
    for (new_index, old_index) in reordered_vertices.iter().enumerate() {
        assert_eq!(old_to_new[*old_index as usize], None, "duplicate vertex found {}", old_index);
        old_to_new[*old_index as usize] = Some(new_index as VertexIndex);
    }
    old_to_new
}

/// translate defect vertices into the current new index given reordered_vertices
#[allow(clippy::unnecessary_cast)]
pub fn translated_defect_to_reordered(
    reordered_vertices: &Vec<VertexIndex>,
    old_defect_vertices: &[VertexIndex],
) -> Vec<VertexIndex> {
    let old_to_new = build_old_to_new(reordered_vertices);
    old_defect_vertices
        .iter()
        .map(|old_index| old_to_new[*old_index as usize].unwrap())
        .collect()
}


#[cfg(test)]
pub mod tests {
    use super::*;

    // #[test]
    // fn util_test() {
    //     let x = VertexSet::new(0, 72, indices)
    // }
}