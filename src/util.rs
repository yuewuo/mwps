use crate::mwpf_solver::*;
#[cfg(not(feature = "float_lp"))]
use crate::num_rational;
use crate::num_traits::ToPrimitive;
use crate::rand_xoshiro;
use crate::rand_xoshiro::rand_core::RngCore;
#[cfg(feature = "python_binding")]
use crate::util_py::*;
use crate::visualize::*;
use num_traits::Zero;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
#[cfg(feature = "python_binding")]
use pyo3::types::{PyDict, PyFloat, PyList};
use serde::{Deserialize, Serialize};

use std::collections::BTreeSet;
use std::fs::File;
use std::io::prelude::*;
use std::time::Instant;

cfg_if::cfg_if! {
    if #[cfg(feature="f64_weight")] {
        pub type Rational = crate::ordered_float::OrderedFloat;
        pub fn numer_of(value: &Rational) -> f64 {
            value.numer().to_f64().unwrap()
        }
        pub fn denom_of(value: &Rational) -> i64 {
            value.denom().to_i64().unwrap()
        }
    } else if #[cfg(feature="rational_weight")] {
        use num_bigint::BigInt;
        pub type Rational = num_rational::BigRational;
        pub fn numer_of(value: &Rational) -> BigInt {
            value.numer().clone()
        }
        pub fn denom_of(value: &Rational) -> BigInt {
            value.denom().clone()
        }
    }
}

pub type Weight = Rational;
pub type EdgeIndex = usize;
pub type VertexIndex = usize;
pub type KnownSafeRefCell<T> = std::cell::RefCell<T>;

pub type NodeIndex = VertexIndex;
pub type DefectIndex = VertexIndex;
pub type VertexNodeIndex = VertexIndex; // must be same as VertexIndex, NodeIndex, DefectIndex
pub type VertexNum = VertexIndex;
pub type NodeNum = VertexIndex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct HyperEdge {
    /// the vertices incident to the hyperedge
    pub vertices: Vec<VertexIndex>,
    /// the weight of the hyperedge
    pub weight: Weight,
}

impl HyperEdge {
    pub fn new(vertices: Vec<VertexIndex>, weight: Weight) -> Self {
        Self { vertices, weight }
    }
}

#[cfg(feature = "python_binding")]
#[pymethods]
impl HyperEdge {
    #[new]
    fn py_new(vertices: &Bound<PyAny>, weight: &Bound<PyAny>) -> PyResult<Self> {
        use crate::util_py::py_into_btree_set;
        let vertices: Vec<VertexIndex> = py_into_btree_set::<VertexIndex>(vertices)?.into_iter().collect();
        Ok(Self::new(vertices, PyRational::from(weight).0))
    }
    fn __repr__(&self) -> String {
        format!("{:?}", self)
    }
    #[getter]
    fn get_vertices(&self) -> Vec<VertexIndex> {
        self.vertices.clone()
    }
    #[setter]
    fn set_vertices(&mut self, vertices: Vec<VertexIndex>) {
        self.vertices = vertices;
    }
    #[getter]
    fn get_weight(&self) -> PyRational {
        self.weight.clone().into()
    }
    #[setter]
    fn set_weight(&mut self, weight: &Bound<PyAny>) {
        self.weight = PyRational::from(weight).0;
    }
}

#[cfg_attr(feature = "python_binding", pyclass(get_all, set_all))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverInitializer {
    /// the number of vertices
    pub vertex_num: VertexNum,
    /// weighted edges, where vertex indices are within the range [0, vertex_num)
    pub weighted_edges: Vec<HyperEdge>,
}

impl SolverInitializer {
    pub fn new(vertex_num: VertexNum, weighted_edges: Vec<HyperEdge>) -> Self {
        Self {
            vertex_num,
            weighted_edges,
        }
    }
}

#[cfg(feature = "python_binding")]
#[pymethods]
impl SolverInitializer {
    #[new]
    fn py_new(vertex_num: VertexNum, weighted_edges: Vec<HyperEdge>) -> Self {
        Self::new(vertex_num, weighted_edges)
    }
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

    pub fn matches_subgraph_syndrome(&self, subgraph: &OutputSubgraph, defect_vertices: &[VertexIndex]) -> bool {
        let subgraph_defect_vertices: Vec<_> = self.get_subgraph_syndrome(subgraph).into_iter().collect();
        let mut defect_vertices = defect_vertices.to_owned();
        defect_vertices.sort();
        if defect_vertices.len() != subgraph_defect_vertices.len() {
            println!(
                "defect vertices: {:?}\nsubgraph_defect_vertices: {:?}",
                defect_vertices, subgraph_defect_vertices
            );
            return false;
        }
        for i in 0..defect_vertices.len() {
            if defect_vertices[i] != subgraph_defect_vertices[i] {
                println!(
                    "defect vertices: {:?}\nsubgraph_defect_vertices: {:?}",
                    defect_vertices, subgraph_defect_vertices
                );
                return false;
            }
        }
        true
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn get_subgraph_total_weight(&self, subgraph: &OutputSubgraph) -> Weight {
        let mut weight = Weight::zero();
        for &edge_index in subgraph.iter() {
            weight += self.weighted_edges[edge_index as usize].weight.clone();
        }
        weight
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn get_subgraph_syndrome(&self, subgraph: &OutputSubgraph) -> BTreeSet<VertexIndex> {
        let mut defect_vertices = BTreeSet::new();
        for &edge_index in subgraph.iter() {
            let HyperEdge { vertices, .. } = &self.weighted_edges[edge_index as usize];
            for &vertex_index in vertices.iter() {
                if defect_vertices.contains(&vertex_index) {
                    defect_vertices.remove(&vertex_index);
                    // println!("duplicate defect vertex: {}", vertex_index);
                } else {
                    defect_vertices.insert(vertex_index);
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
#[cfg_attr(feature = "python_binding", pyclass(get_all, set_all))]
pub struct SyndromePattern {
    /// the vertices corresponding to defect measurements
    pub defect_vertices: Vec<VertexIndex>,
    /// the edges that experience erasures, i.e. known errors
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

impl SyndromePattern {
    pub fn new_vertices(defect_vertices: Vec<VertexIndex>) -> Self {
        Self::new(defect_vertices, vec![])
    }
    pub fn new_empty() -> Self {
        Self::new(vec![], vec![])
    }
}

#[cfg(feature = "python_binding")]
#[pymethods]
impl SyndromePattern {
    #[new]
    #[pyo3(signature = (defect_vertices=None, erasures=None))]
    fn py_new(defect_vertices: Option<&Bound<PyAny>>, erasures: Option<&Bound<PyAny>>) -> PyResult<Self> {
        use crate::util_py::py_into_btree_set;
        let defect_vertices: Vec<VertexIndex> = if let Some(defect_vertices) = defect_vertices {
            py_into_btree_set(defect_vertices)?.into_iter().collect()
        } else {
            vec![]
        };
        let erasures: Vec<EdgeIndex> = if let Some(erasures) = erasures {
            py_into_btree_set(erasures)?.into_iter().collect()
        } else {
            vec![]
        };
        Ok(Self::new(defect_vertices, erasures))
    }
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
pub type Subgraph = Vec<EdgeIndex>;

pub struct OutputSubgraph {
    pub subgraph: Subgraph,
    pub flip_edge_indices: hashbrown::HashSet<EdgeIndex>,
}

impl OutputSubgraph {
    pub fn new(subgraph: Subgraph, flip_edge_indices: hashbrown::HashSet<EdgeIndex>) -> Self {
        Self {
            subgraph,
            flip_edge_indices,
        }
    }

    pub fn iter(&self) -> OutputSubgraphIter {
        OutputSubgraphIter {
            subgraph_iter: self.subgraph.iter(),
            flip_edge_indices: &self.flip_edge_indices,
            remaining_indices: self.flip_edge_indices.clone(),
        }
    }

    // Mutable iterator with updates to `subgraph` during iteration
    pub fn iter_mut(&mut self) -> OutputSubgraphIterMut {
        OutputSubgraphIterMut {
            subgraph: &mut self.subgraph,
            subgraph_iter: 0, // Start iterating from the beginning of `subgraph`
            flip_edge_indices: &mut self.flip_edge_indices,
        }
    }
}

impl From<Subgraph> for OutputSubgraph {
    fn from(value: Subgraph) -> Self {
        Self::new(value, hashbrown::HashSet::new())
    }
}

// consuming iterators
// Implementing `IntoIterator` for `&OutputSubgraph` (for `iter`)
impl<'a> IntoIterator for &'a OutputSubgraph {
    type Item = &'a usize;
    type IntoIter = OutputSubgraphIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        OutputSubgraphIter {
            subgraph_iter: self.subgraph.iter(),
            flip_edge_indices: &self.flip_edge_indices,
            remaining_indices: self.flip_edge_indices.clone(),
        }
    }
}

// Implementing `IntoIterator` for `&mut OutputSubgraph` (for `iter_mut`)
impl<'a> IntoIterator for &'a mut OutputSubgraph {
    type Item = &'a mut usize;
    type IntoIter = OutputSubgraphIterMut<'a>;

    fn into_iter(self) -> Self::IntoIter {
        OutputSubgraphIterMut {
            subgraph: &mut self.subgraph,
            subgraph_iter: 0, // Start at the beginning of `subgraph`
            flip_edge_indices: &mut self.flip_edge_indices,
        }
    }
}

// Implementing `IntoIterator` for `OutputSubgraph` (for `into_iter`)
impl IntoIterator for OutputSubgraph {
    type Item = usize;
    type IntoIter = OutputSubgraphIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        OutputSubgraphIntoIter {
            subgraph_iter: self.subgraph.into_iter(),
            flip_edge_indices: self.flip_edge_indices.clone(),
            remaining_indices: self.flip_edge_indices,
        }
    }
}

pub struct OutputSubgraphIter<'a> {
    subgraph_iter: std::slice::Iter<'a, usize>,
    flip_edge_indices: &'a hashbrown::HashSet<EdgeIndex>,
    remaining_indices: hashbrown::HashSet<EdgeIndex>,
}

impl<'a> Iterator for OutputSubgraphIter<'a> {
    type Item = &'a usize;

    fn next(&mut self) -> Option<Self::Item> {
        // note: optional short circuiting
        if self.flip_edge_indices.is_empty() {
            return self.subgraph_iter.next();
        }

        // Iterate over the `subgraph` elements
        while let Some(index) = self.subgraph_iter.next() {
            if self.flip_edge_indices.contains(index) {
                // Record this index as seen and skip it in output
                self.remaining_indices.remove(index);
                continue;
            } else {
                return Some(index);
            }
        }

        // After finishing subgraph, yield elements from `flip_edge_indices` that were not seen
        if let Some(&remaining_index) = self.remaining_indices.iter().next() {
            self.remaining_indices.remove(&remaining_index);
            return Some(self.flip_edge_indices.get(&remaining_index).unwrap());
        }

        // No more elements to yield
        None
    }
}

// Mutable iterator
pub struct OutputSubgraphIterMut<'a> {
    subgraph: &'a mut Subgraph,
    subgraph_iter: usize, // Index within `subgraph`
    flip_edge_indices: &'a mut hashbrown::HashSet<EdgeIndex>,
}

// note: use of unsafe
impl<'a> Iterator for OutputSubgraphIterMut<'a> {
    type Item = &'a mut usize;

    fn next(&mut self) -> Option<Self::Item> {
        // Iterate over the `subgraph` elements first

        let len = self.subgraph.len();
        while self.subgraph_iter < len {
            let index = self.subgraph_iter;
            self.subgraph_iter += 1;
            let elem = self.subgraph[index];

            // Skip elements in `flip_edge_indices`
            if self.flip_edge_indices.contains(&elem) {
                self.flip_edge_indices.remove(&elem);
                self.subgraph_iter -= 1;
                self.subgraph.remove(self.subgraph_iter);
                continue;
            } else {
                // Using `unsafe` to circumvent borrowing rules safely
                return Some(unsafe { &mut *(&mut self.subgraph[index] as *mut usize) });
            }
        }

        // After `subgraph` elements, add remaining `flip_edge_indices` to `subgraph`
        if let Some(&remaining_index) = self.flip_edge_indices.iter().next() {
            self.flip_edge_indices.remove(&remaining_index);
            self.subgraph.push(remaining_index);
            self.subgraph_iter += 1; // Update to point to the newly added element

            // Using `unsafe` to return a mutable reference to the last element, guaranteed to exist
            return Some(unsafe { &mut *(self.subgraph.last_mut().unwrap() as *mut usize) });
        }

        // No more elements to yield
        None
    }
}

// Consuming iterator
pub struct OutputSubgraphIntoIter {
    subgraph_iter: std::vec::IntoIter<usize>,
    flip_edge_indices: hashbrown::HashSet<EdgeIndex>,
    remaining_indices: hashbrown::HashSet<EdgeIndex>,
}

impl Iterator for OutputSubgraphIntoIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.flip_edge_indices.is_empty() {
            return self.subgraph_iter.next();
        }

        while let Some(index) = self.subgraph_iter.next() {
            if self.flip_edge_indices.contains(&index) {
                self.remaining_indices.remove(&index);
                continue;
            } else {
                return Some(index);
            }
        }

        if let Some(&remaining_index) = self.remaining_indices.iter().next() {
            self.remaining_indices.remove(&remaining_index);
            Some(remaining_index)
        } else {
            None
        }
    }
}

impl MWPSVisualizer for OutputSubgraph {
    fn snapshot(&self, _abbrev: bool) -> serde_json::Value {
        let mut adjusted_subgraph_set = self.subgraph.iter().collect::<hashbrown::HashSet<_>>();
        for to_flip in self.flip_edge_indices.iter() {
            if adjusted_subgraph_set.contains(to_flip) {
                adjusted_subgraph_set.remove(to_flip);
            } else {
                adjusted_subgraph_set.insert(to_flip);
            }
        }
        let adjusted_subgraph = adjusted_subgraph_set.into_iter().collect::<Vec<_>>();
        json!({
            "subgraph": self.subgraph,
            "flip_edge_indices": self.flip_edge_indices.iter().collect::<Vec<_>>(),
            "adjusted_subgraph_for_negative_weight": adjusted_subgraph
        })
    }
}

#[allow(clippy::to_string_in_format_args)]
impl std::fmt::Debug for OutputSubgraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Create adjusted subgraph set
        let mut adjusted_subgraph_set = self.subgraph.iter().copied().collect::<hashbrown::HashSet<_>>();
        for &to_flip in &self.flip_edge_indices {
            if adjusted_subgraph_set.contains(&to_flip) {
                adjusted_subgraph_set.remove(&to_flip);
            } else {
                adjusted_subgraph_set.insert(to_flip);
            }
        }
        let adjusted_subgraph = adjusted_subgraph_set.into_iter().collect::<Vec<_>>();

        // Output debug information in similar format to snapshot
        write!(
            f,
            "{}",
            json!({
                "subgraph": self.subgraph,
                "flip_edge_indices": self.flip_edge_indices.iter().collect::<Vec<_>>(),
                "adjusted_subgraph_for_negative_weight": adjusted_subgraph
            })
            .to_string()
        )
    }
}

impl MWPSVisualizer for Subgraph {
    fn snapshot(&self, _abbrev: bool) -> serde_json::Value {
        json!({
            "subgraph": self,
        })
    }
}

/// the range of the optimal MWPF solution's weight
#[derive(Clone, Debug)]
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
                "ln": numer_of(&self.lower),
                "ld": denom_of(&self.lower),
                "un": numer_of(&self.upper),
                "ud": denom_of(&self.upper),
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

    /// summation of all tuning time
    pub sum_tuning_time: f64,
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
            sum_tuning_time: 0.,
        }
    }
    /// record the beginning of a decoding procedure
    pub fn begin(&mut self, syndrome_pattern: &SyndromePattern, error_pattern: &Subgraph) {
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
    pub fn end(&mut self, solver: Option<&dyn SolverTrait>) {
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
                let value_mut = value.as_object_mut().unwrap();
                value_mut.insert("solver_profile".to_string(), solver_profile);
                if let Some(tuning_time) = solver.get_tuning_time() {
                    value_mut.insert("tuning_time".to_string(), tuning_time.into());
                    self.sum_tuning_time += tuning_time;
                }
            }
            file.write_all(serde_json::to_string(&value).unwrap().as_bytes()).unwrap();
            file.write_all(b"\n").unwrap();
        } else if let Some(solver) = solver {
            if let Some(tuning_time) = solver.get_tuning_time() {
                self.sum_tuning_time += tuning_time;
            }
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
    pub error_pattern: Subgraph,
    /// the time of beginning a decoding procedure
    begin_time: Option<Instant>,
    /// record additional events
    pub events: Vec<(String, f64)>,
    /// interval between calling [`Self::record_begin`] to calling [`Self::record_end`]
    pub round_time: Option<f64>,
}

impl BenchmarkProfilerEntry {
    pub fn new(syndrome_pattern: &SyndromePattern, error_pattern: &Subgraph) -> Self {
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
            PyList::new_bound(py, elements).into()
        }
        serde_json::Value::Object(map) => {
            let pydict = PyDict::new_bound(py);
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
pub fn pyobject_to_json_locked(value: &Bound<PyAny>) -> serde_json::Value {
    if value.is_none() {
        serde_json::Value::Null
    } else if value.is_instance_of::<pyo3::types::PyBool>() {
        json!(value.extract::<bool>().unwrap())
    } else if value.is_instance_of::<pyo3::types::PyInt>() {
        json!(value.extract::<i64>().unwrap())
    } else if value.is_instance_of::<PyFloat>() {
        json!(value.extract::<f64>().unwrap())
    } else if value.is_instance_of::<pyo3::types::PyString>() {
        json!(value.extract::<String>().unwrap())
    } else if value.is_instance_of::<PyList>() {
        let elements: Vec<serde_json::Value> = value
            .downcast::<PyList>()
            .unwrap()
            .into_iter()
            .map(|object| pyobject_to_json_locked(&object))
            .collect();
        json!(elements)
    } else if value.is_instance_of::<PyDict>() {
        let map: &Bound<PyDict> = value.downcast().unwrap();
        let mut json_map = serde_json::Map::new();
        for (key, value) in map.iter() {
            json_map.insert(key.extract::<String>().unwrap(), pyobject_to_json_locked(&value));
        }
        serde_json::Value::Object(json_map)
    } else {
        unimplemented!("unsupported python type, should be (cascaded) dict, list and basic numerical types")
    }
}

#[cfg(feature = "python_binding")]
pub fn pyobject_to_json(value: PyObject) -> serde_json::Value {
    Python::with_gil(|py| pyobject_to_json_locked(value.bind(py)))
}

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SolverInitializer>()?;
    m.add_class::<SyndromePattern>()?;
    m.add_class::<HyperEdge>()?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use hashbrown::HashSet;
    use num_bigint::BigInt;
    use std::str::FromStr;

    #[test]
    fn util_py_json_bigint() {
        // cargo test util_py_json_bigint -- --nocapture
        let small_int = BigInt::from(123);
        let big_int = BigInt::from_str("123456789012345678901234567890123").unwrap();
        println!("small_int: {:?}, json: {}", small_int, json!(small_int));
        println!("positive big_int: {:?}, json: {}", big_int, json!(big_int));
        println!("negative big_int: {:?}, json: {}", -big_int.clone(), json!(-big_int));
        let zero_int = BigInt::from(0);
        println!("zero_int: {:?}, json: {}", zero_int, json!(zero_int));
    }

    #[test]
    fn test_iter() {
        let subgraph = vec![1, 2, 3, 4];
        let mut flip_edge_indices = HashSet::new();
        flip_edge_indices.insert(2);
        flip_edge_indices.insert(5);

        let output_subgraph = OutputSubgraph::new(subgraph, flip_edge_indices);

        // Expected behavior: `2` is skipped, and `5` is added at the end.
        let result: Vec<_> = output_subgraph.iter().cloned().collect();
        assert_eq!(result, vec![1, 3, 4, 5]);
    }

    #[test]
    fn test_iter_empty_flip_edge_indices() {
        let subgraph = vec![1, 2, 3];
        let flip_edge_indices = HashSet::new();

        let output_subgraph = OutputSubgraph::new(subgraph.clone(), flip_edge_indices);

        // With empty `flip_edge_indices`, should just return all elements in `subgraph`.
        let result: Vec<_> = output_subgraph.iter().cloned().collect();
        assert_eq!(result, subgraph);
    }

    #[test]
    fn test_iter_all_elements_flipped() {
        let subgraph = vec![1, 2, 3];
        let mut flip_edge_indices = HashSet::new();
        flip_edge_indices.insert(1);
        flip_edge_indices.insert(2);
        flip_edge_indices.insert(3);
        flip_edge_indices.insert(4);

        let output_subgraph = OutputSubgraph::new(subgraph, flip_edge_indices);

        // Expected behavior: all elements in `subgraph` are skipped, and `4` is added at the end.
        let result: Vec<_> = output_subgraph.iter().cloned().collect();
        assert_eq!(result, vec![4]);
    }

    #[test]
    fn test_iter_mut() {
        let subgraph = vec![1, 2, 3, 4];
        let mut flip_edge_indices = HashSet::new();
        flip_edge_indices.insert(2);
        flip_edge_indices.insert(5);

        let mut output_subgraph = OutputSubgraph::new(subgraph, flip_edge_indices);

        // Modify elements during mutable iteration
        for elem in output_subgraph.iter_mut() {
            *elem *= 2;
        }

        // Verify that `2` was skipped and `5` was added and modified at the end
        assert_eq!(output_subgraph.subgraph, vec![2, 6, 8, 10]);
        assert!(output_subgraph.flip_edge_indices.is_empty());
    }

    #[test]
    fn test_iter_mut_no_modifications() {
        let subgraph = vec![10, 20, 30];
        let flip_edge_indices = HashSet::new(); // Empty flip edge indices

        let mut output_subgraph = OutputSubgraph::new(subgraph.clone(), flip_edge_indices);

        // Expected to iterate through all without any modifications to flip_edge_indices
        for elem in output_subgraph.iter_mut() {
            *elem += 1;
        }

        // Verify that all elements were incremented and no `flip_edge_indices` remains
        assert_eq!(output_subgraph.subgraph, vec![11, 21, 31]);
        assert!(output_subgraph.flip_edge_indices.is_empty());
    }

    #[test]
    fn test_into_iter() {
        let subgraph = vec![1, 2, 3, 4];
        let mut flip_edge_indices = HashSet::new();
        flip_edge_indices.insert(2);
        flip_edge_indices.insert(5);

        let output_subgraph = OutputSubgraph::new(subgraph, flip_edge_indices);

        // Consuming iterator, so `output_subgraph` cannot be used afterward
        let result: Vec<_> = output_subgraph.into_iter().collect();

        // Expected behavior: `2` is skipped, and `5` is added at the end.
        assert_eq!(result, vec![1, 3, 4, 5]);
    }

    #[test]
    fn test_into_iter_all_elements_flipped() {
        let subgraph = vec![1, 2, 3];
        let mut flip_edge_indices = HashSet::new();
        flip_edge_indices.insert(1);
        flip_edge_indices.insert(2);
        flip_edge_indices.insert(3);
        flip_edge_indices.insert(4);

        let output_subgraph = OutputSubgraph::new(subgraph, flip_edge_indices);

        // Consuming iterator, expected to yield only `4` at the end since all `subgraph` elements are flipped.
        let result: Vec<_> = output_subgraph.into_iter().collect();
        assert_eq!(result, vec![4]);
    }

    #[test]
    fn test_iter_empty_subgraph() {
        let subgraph = vec![];
        let mut flip_edge_indices = HashSet::new();
        flip_edge_indices.insert(1);
        flip_edge_indices.insert(2);

        let output_subgraph = OutputSubgraph::new(subgraph, flip_edge_indices);

        // With empty `subgraph`, should only yield elements in `flip_edge_indices`
        let mut result: Vec<_> = output_subgraph.iter().cloned().collect();
        result.sort();
        assert_eq!(result, vec![1, 2]); // order here doesn't matter
    }

    #[test]
    fn test_iter_mut_update_subgraph() {
        let subgraph = vec![1, 2, 3, 4];
        let mut flip_edge_indices = HashSet::new();
        flip_edge_indices.insert(2);
        flip_edge_indices.insert(5);

        let mut output_subgraph = OutputSubgraph::new(subgraph, flip_edge_indices);

        // Expected behavior: `2` is skipped, and `5` is added at the end.
        let result: Vec<_> = output_subgraph.iter_mut().map(|x| *x).collect();
        assert_eq!(result, vec![1, 3, 4, 5]);

        assert_eq!(output_subgraph.subgraph, vec![1, 3, 4, 5]);
        assert!(output_subgraph.flip_edge_indices.is_empty());
    }
}
