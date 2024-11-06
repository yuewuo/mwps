use crate::mwpf_solver::*;
#[cfg(not(feature = "float_lp"))]
use crate::num_rational;
use crate::num_traits::ToPrimitive;
use crate::rand_xoshiro;
use crate::rand_xoshiro::rand_core::RngCore;
use crate::visualize::*;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
#[cfg(feature = "python_binding")]
use pyo3::types::{PyDict, PyFloat, PyList};
use serde::{Deserialize, Serialize};

use std::collections::BTreeSet;
use std::fs::File;
use std::io::prelude::*;
use std::time::Instant;

pub type Weight = usize; // only used as input, all internal weight representation will use `Rational`

cfg_if::cfg_if! {
    if #[cfg(feature="r64_weight")] {
        pub type Rational = num_rational::Rational64;
        pub fn numer_of(value: &Rational) -> i64 {
            value.numer().to_i64().unwrap()
        }
    } else if #[cfg(feature="float_lp")] {
        pub type Rational = crate::ordered_float::OrderedFloat;
        pub fn numer_of(value: &Rational) -> f64 {
            value.numer().to_f64().unwrap()
        }
    } else  {
        pub type Rational = num_rational::BigRational;
        pub fn numer_of(value: &Rational) -> i64 {
            value.numer().to_i64().unwrap()
        }
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

pub type KnownSafeRefCell<T> = std::cell::RefCell<T>;

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
    pub fn get_subgraph_total_weight(&self, subgraph: &Subgraph) -> Weight {
        let mut weight = 0;
        for &edge_index in subgraph.iter() {
            weight += self.weighted_edges[edge_index as usize].weight;
        }
        weight
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn get_subgraph_syndrome(&self, subgraph: &Subgraph) -> BTreeSet<VertexIndex> {
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
pub type Subgraph = Vec<EdgeIndex>;

impl MWPSVisualizer for Subgraph {
    fn snapshot(&self, _abbrev: bool) -> serde_json::Value {
        json!({
            "subgraph": self,
        })
    }
}

// https://stackoverflow.com/questions/76082775/return-a-python-object-defined-in-a-third-party-python-module-e-g-numpy-using
#[cfg(feature = "python_binding")]
pub fn rational_to_pyobject(value: &Rational) -> PyResult<Py<PyAny>> {
    Python::with_gil(|py| {
        if cfg!(feature = "float_lp") {
            PyResult::Ok(PyFloat::new_bound(py, value.to_f64().unwrap()).into())
        } else {
            let frac = py.import_bound("fractions")?;
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
