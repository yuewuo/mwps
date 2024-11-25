//! Minimum-Weight Parity Factor Solver
//!
//! This module includes some common usage of primal and dual modules to solve MWPF problems.
//! Note that you can call different primal and dual modules, even interchangeably, by following the examples in this file
//!

use crate::dual_module::*;
use crate::dual_module_pq::*;
use crate::example_codes::*;
use crate::model_hypergraph::*;
use crate::plugin::*;
use crate::plugin_single_hair::*;
use crate::plugin_union_find::PluginUnionFind;
use crate::primal_module::*;
use crate::primal_module_serial::*;
use crate::util::*;
use crate::visualize::*;
use core::panic;
use hashbrown::HashSet;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::prelude::*;
use std::io::BufWriter;
use std::sync::Arc;

cfg_if::cfg_if! {
    if #[cfg(feature="python_binding")] {
        use crate::invalid_subgraph::*;
        use crate::util_py::*;
        use pyo3::prelude::*;
        use std::collections::BTreeSet;
    }
}

pub trait SolverTrait {
    fn debug_print(&self) {
        unimplemented!();
    }
    fn clear(&mut self);
    fn solve_visualizer(&mut self, syndrome_pattern: SyndromePattern, visualizer: Option<&mut Visualizer>);
    fn solve(&mut self, syndrome_pattern: SyndromePattern) {
        self.solve_visualizer(syndrome_pattern, None)
    }
    fn subgraph_range_visualizer(&mut self, visualizer: Option<&mut Visualizer>) -> (OutputSubgraph, WeightRange);
    fn subgraph_range(&mut self) -> (OutputSubgraph, WeightRange) {
        self.subgraph_range_visualizer(None)
    }
    fn subgraph(&mut self) -> OutputSubgraph {
        self.subgraph_range().0
    }
    fn sum_dual_variables(&self) -> Rational;
    fn generate_profiler_report(&self) -> serde_json::Value;

    fn get_tuning_time(&self) -> Option<f64>;
    fn clear_tuning_time(&mut self);
    fn print_clusters(&self) {
        panic!();
    }
    fn update_weights(&mut self, new_weights: &mut Vec<f64>, mix_ratio: f64);
    fn get_model_graph(&self) -> Arc<ModelHyperGraph>;
}

#[cfg(feature = "python_binding")]
macro_rules! bind_trait_to_python {
    ($struct_name:ident) => {
        #[pymethods]
        impl $struct_name {
            #[pyo3(name = "clear")]
            fn py_clear(&mut self) {
                self.clear()
            }
            #[pyo3(name = "solve", signature = (syndrome_pattern, visualizer=None))] // in Python, `solve` and `solve_visualizer` is the same because it can take optional parameter
            fn py_solve(&mut self, syndrome_pattern: SyndromePattern, visualizer: Option<&mut Visualizer>) {
                self.solve_visualizer(syndrome_pattern, visualizer)
            }
            #[pyo3(name = "subgraph_range", signature = (visualizer=None))] // in Python, `subgraph_range` and `subgraph_range_visualizer` is the same
            fn py_subgraph_range(&mut self, visualizer: Option<&mut Visualizer>) -> (PySubgraph, PyWeightRange) {
                let (subgraph, range) = self.subgraph_range_visualizer(visualizer);
                (subgraph.into_iter().collect::<Vec<EdgeIndex>>().into(), range.into())
            }
            #[pyo3(name = "subgraph", signature = (visualizer=None))]
            fn py_subgraph(&mut self, visualizer: Option<&mut Visualizer>) -> Subgraph {
                self.subgraph_range_visualizer(visualizer).0.into_iter().collect()
            }
            #[pyo3(name = "sum_dual_variables")]
            fn py_sum_dual_variables(&self) -> PyRational {
                self.sum_dual_variables().clone().into()
            }
            #[pyo3(name = "load_syndrome", signature = (syndrome_pattern, visualizer=None))]
            pub fn py_load_syndrome(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>) {
                self.0.load_syndrome(syndrome_pattern, visualizer)
            }
            #[pyo3(name = "get_node", signature = (node_index))]
            pub fn py_get_node(&mut self, node_index: NodeIndex) -> Option<PyDualNodePtr> {
                self.0.interface_ptr.get_node(node_index).map(|x| x.into())
            }
            #[pyo3(name = "find_node", signature = (vertices=None, edges=None))]
            pub fn py_find_node(
                &self,
                vertices: Option<&Bound<PyAny>>,
                edges: Option<&Bound<PyAny>>,
            ) -> PyResult<Option<PyDualNodePtr>> {
                let invalid_subgraph = Arc::new(self.py_construct_invalid_subgraph(vertices, edges)?);
                Ok(self.0.interface_ptr.find_node(&invalid_subgraph).map(|x| x.into()))
            }
            #[pyo3(name = "create_node", signature = (vertices=None, edges=None))]
            pub fn py_create_node(
                &mut self,
                vertices: Option<&Bound<PyAny>>,
                edges: Option<&Bound<PyAny>>,
            ) -> PyResult<PyDualNodePtr> {
                let invalid_subgraph = Arc::new(self.py_construct_invalid_subgraph(vertices, edges)?);
                let interface_ptr = self.0.interface_ptr.clone();
                Ok(match self.0.dual_module.mode() {
                    DualModuleMode::Search => interface_ptr.create_node(invalid_subgraph, &mut self.0.dual_module),
                    DualModuleMode::Tune => interface_ptr.create_node_tune(invalid_subgraph, &mut self.0.dual_module),
                }
                .into())
            }
            #[pyo3(name = "grow", signature = (length))]
            fn py_grow(&mut self, length: PyRational) {
                let length: Rational = length.into();
                if let Some(max_valid_grow) = self.0.dual_module.compute_max_valid_grow() {
                    assert!(
                        length <= max_valid_grow,
                        "growth overflow: attempting to grow {} but can only grow {} maximum",
                        length,
                        max_valid_grow
                    );
                };
                self.0.dual_module.grow(length)
            }
            #[pyo3(name = "snapshot", signature = (abbrev=true))]
            fn py_snapshot(&mut self, abbrev: bool) -> PyObject {
                json_to_pyobject(self.0.snapshot(abbrev))
            }
            #[pyo3(name = "dual_report")]
            fn py_dual_report(&mut self) -> PyDualReport {
                self.0.dual_module.report().into()
            }
            #[pyo3(name = "get_edge_nodes")]
            fn py_get_edge_nodes(&self, edge_index: EdgeIndex) -> Vec<PyDualNodePtr> {
                self.0
                    .dual_module
                    .get_edge_nodes(edge_index)
                    .into_iter()
                    .map(|x| x.into())
                    .collect()
            }
            #[pyo3(name = "set_grow_rate")]
            fn py_set_grow_rate(&mut self, dual_node_ptr: PyDualNodePtr, grow_rate: PyRational) {
                self.0.dual_module.set_grow_rate(&dual_node_ptr.0, grow_rate.into())
            }
        }
        impl $struct_name {
            pub fn py_construct_invalid_subgraph(
                &self,
                vertices: Option<&Bound<PyAny>>,
                edges: Option<&Bound<PyAny>>,
            ) -> PyResult<InvalidSubgraph> {
                // edges default to empty set
                let edges = if let Some(edges) = edges {
                    py_into_btree_set(edges)?
                } else {
                    BTreeSet::new()
                };
                // vertices must be superset of the union of all edges
                let interface = self.0.interface_ptr.read_recursive();
                Ok(if let Some(vertices) = vertices {
                    let vertices = py_into_btree_set(vertices)?;
                    InvalidSubgraph::new_complete(vertices, edges, &interface.decoding_graph)
                } else {
                    InvalidSubgraph::new(edges, &interface.decoding_graph)
                })
            }
        }
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolverSerialPluginsConfig {
    #[serde(flatten, default = "hyperion_default_configs::primal")]
    primal: PrimalModuleSerialConfig,
}

pub mod hyperion_default_configs {
    use crate::primal_module_serial::*;

    pub fn primal() -> PrimalModuleSerialConfig {
        serde_json::from_value(json!({})).unwrap()
    }
}

pub struct SolverSerialPlugins {
    dual_module: DualModulePQ,
    primal_module: PrimalModuleSerial,
    interface_ptr: DualModuleInterfacePtr,
    model_graph: Arc<ModelHyperGraph>,
}

impl MWPSVisualizer for SolverSerialPlugins {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        let mut value = self.primal_module.snapshot(abbrev);
        snapshot_combine_values(&mut value, self.dual_module.snapshot(abbrev), abbrev);
        snapshot_combine_values(&mut value, self.interface_ptr.snapshot(abbrev), abbrev);
        value
    }
}

impl SolverSerialPlugins {
    pub fn new(initializer: &SolverInitializer, plugins: Arc<Vec<PluginEntry>>, config: serde_json::Value) -> Self {
        let model_graph = Arc::new(ModelHyperGraph::new(Arc::new(initializer.clone())));
        let mut primal_module = PrimalModuleSerial::new_empty(initializer); // question: why does this need initializer?
        let config: SolverSerialPluginsConfig = serde_json::from_value(config).unwrap();
        primal_module.plugins = plugins;
        primal_module.config = config.primal.clone();

        Self {
            dual_module: DualModulePQ::new_empty(initializer),
            primal_module,
            interface_ptr: DualModuleInterfacePtr::new(model_graph.clone()),
            model_graph,
        }
    }
}

impl SolverSerialPlugins {
    // APIs for step-by-step solving in Python
    pub fn load_syndrome(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>) {
        self.primal_module.solve_step_load_syndrome(
            &self.interface_ptr,
            Arc::new(syndrome_pattern.clone()),
            &mut self.dual_module,
        );
        if let Some(visualizer) = visualizer {
            visualizer
                .snapshot_combined(
                    "syndrome loaded".to_string(),
                    vec![&self.interface_ptr, &self.dual_module, &self.primal_module],
                )
                .unwrap();
        }
    }
}

impl SolverTrait for SolverSerialPlugins {
    fn clear(&mut self) {
        self.primal_module.clear();
        self.dual_module.clear();
        self.interface_ptr.clear();
    }
    fn solve_visualizer(&mut self, mut syndrome_pattern: SyndromePattern, visualizer: Option<&mut Visualizer>) {
        self.dual_module.adjust_weights_for_negative_edges();

        let moved_out_vec = std::mem::take(&mut syndrome_pattern.defect_vertices);
        let mut moved_out_set = moved_out_vec.into_iter().collect::<HashSet<VertexIndex>>();

        for to_flip in self.dual_module.get_flip_vertices().iter() {
            if moved_out_set.contains(to_flip) {
                moved_out_set.remove(to_flip);
            } else {
                moved_out_set.insert(*to_flip);
            }
        }

        syndrome_pattern.defect_vertices = moved_out_set.into_iter().collect();

        let syndrome_pattern = Arc::new(syndrome_pattern.clone());

        if !syndrome_pattern.erasures.is_empty() {
            unimplemented!();
        }
        self.primal_module.solve_visualizer(
            &self.interface_ptr,
            syndrome_pattern.clone(),
            &mut self.dual_module,
            visualizer,
        );
        debug_assert!(
            {
                let subgraph = self.subgraph();
                self.model_graph
                    .matches_subgraph_syndrome(&subgraph, &syndrome_pattern.defect_vertices)
            },
            "the subgraph does not generate the syndrome"
        );
    }
    fn subgraph_range_visualizer(&mut self, visualizer: Option<&mut Visualizer>) -> (OutputSubgraph, WeightRange) {
        let (subgraph, weight_range) = self.primal_module.subgraph_range(&self.interface_ptr, &mut self.dual_module);
        if let Some(visualizer) = visualizer {
            visualizer
                .snapshot_combined(
                    "subgraph".to_string(),
                    vec![&self.interface_ptr, &self.dual_module, &subgraph, &weight_range],
                )
                .unwrap();
        }
        (subgraph, weight_range)
    }
    fn sum_dual_variables(&self) -> Rational {
        self.interface_ptr.sum_dual_variables()
    }
    fn generate_profiler_report(&self) -> serde_json::Value {
        json!({
            // "dual": self.dual_module.generate_profiler_report(),
            "primal": self.primal_module.generate_profiler_report(),
        })
    }
    fn get_tuning_time(&self) -> Option<f64> {
        self.dual_module.get_total_tuning_time()
    }
    fn clear_tuning_time(&mut self) {
        self.dual_module.clear_tuning_time()
    }
    fn print_clusters(&self) {
        self.primal_module.print_clusters();
    }
    fn update_weights(&mut self, new_weights: &mut Vec<f64>, mix_ratio: f64) {
        self.dual_module.update_weights(new_weights, mix_ratio);
    }
    fn get_model_graph(&self) -> Arc<ModelHyperGraph> {
        self.model_graph.clone()
    }
    fn debug_print(&self) {
        self.dual_module.debug_print();
    }
}

macro_rules! bind_solver_trait {
    ($struct_name:ident) => {
        impl SolverTrait for $struct_name {
            fn clear(&mut self) {
                self.0.clear()
            }
            fn solve_visualizer(&mut self, syndrome_pattern: SyndromePattern, visualizer: Option<&mut Visualizer>) {
                self.0.solve_visualizer(syndrome_pattern, visualizer)
            }
            fn subgraph_range_visualizer(&mut self, visualizer: Option<&mut Visualizer>) -> (OutputSubgraph, WeightRange) {
                self.0.subgraph_range_visualizer(visualizer)
            }
            fn sum_dual_variables(&self) -> Rational {
                self.0.sum_dual_variables()
            }
            fn generate_profiler_report(&self) -> serde_json::Value {
                self.0.generate_profiler_report()
            }
            fn get_tuning_time(&self) -> Option<f64> {
                self.0.get_tuning_time()
            }
            fn clear_tuning_time(&mut self) {
                self.0.clear_tuning_time()
            }
            fn print_clusters(&self) {
                self.0.print_clusters()
            }
            fn update_weights(&mut self, new_weights: &mut Vec<f64>, mix_ratio: f64) {
                self.0.update_weights(new_weights, mix_ratio)
            }
            fn get_model_graph(&self) -> Arc<ModelHyperGraph> {
                self.0.model_graph.clone()
            }
            fn debug_print(&self) {
                self.0.debug_print()
            }
        }
    };
}

#[cfg_attr(feature = "python_binding", pyclass)]
pub struct SolverSerialUnionFind(SolverSerialPlugins);

impl SolverSerialUnionFind {
    pub fn new(initializer: &SolverInitializer, config: serde_json::Value) -> Self {
        Self(SolverSerialPlugins::new(initializer, Arc::new(vec![]), config))
    }
}

#[cfg(feature = "python_binding")]
#[pymethods]
impl SolverSerialUnionFind {
    #[new]
    #[pyo3(signature = (initializer, config=None))]
    pub fn new_python(initializer: &SolverInitializer, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        Self::new(initializer, config)
    }
}

bind_solver_trait!(SolverSerialUnionFind);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverSerialUnionFind);

#[cfg_attr(feature = "python_binding", pyclass)]
pub struct SolverSerialSingleHair(SolverSerialPlugins);

impl SolverSerialSingleHair {
    pub fn new(initializer: &SolverInitializer, config: serde_json::Value) -> Self {
        Self(SolverSerialPlugins::new(
            initializer,
            Arc::new(vec![
                PluginUnionFind::entry(), // to allow timeout using union-find as baseline
                PluginSingleHair::entry_with_strategy(RepeatStrategy::Once),
            ]),
            config,
        ))
    }
}

#[cfg(feature = "python_binding")]
#[pymethods]
impl SolverSerialSingleHair {
    #[new]
    #[pyo3(signature = (initializer, config=None))]
    pub fn new_python(initializer: &SolverInitializer, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        Self::new(initializer, config)
    }
}

bind_solver_trait!(SolverSerialSingleHair);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverSerialSingleHair);

#[cfg_attr(feature = "python_binding", pyclass)]
pub struct SolverSerialJointSingleHair(SolverSerialPlugins);

impl SolverSerialJointSingleHair {
    pub fn new(initializer: &SolverInitializer, config: serde_json::Value) -> Self {
        Self(SolverSerialPlugins::new(
            initializer,
            Arc::new(vec![
                PluginUnionFind::entry(), // to allow timeout using union-find as baseline
                PluginSingleHair::entry_with_strategy(RepeatStrategy::Once), // first make all clusters valid single hair
                PluginSingleHair::entry_with_strategy(RepeatStrategy::Multiple {
                    max_repetition: usize::MAX,
                }),
            ]),
            config,
        ))
    }
}

#[cfg(feature = "python_binding")]
#[pymethods]
impl SolverSerialJointSingleHair {
    #[new]
    #[pyo3(signature = (initializer, config=None))]
    pub fn new_python(initializer: &SolverInitializer, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        Self::new(initializer, config)
    }
}

bind_solver_trait!(SolverSerialJointSingleHair);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverSerialJointSingleHair);

#[cfg_attr(feature = "python_binding", pyclass)]
pub struct SolverErrorPatternLogger {
    file: BufWriter<File>,
}

impl SolverErrorPatternLogger {
    pub fn new(initializer: &SolverInitializer, code: &dyn ExampleCode, mut config: serde_json::Value) -> Self {
        let mut filename = "tmp/syndrome_patterns.txt".to_string();
        let config = config.as_object_mut().expect("config must be JSON object");
        if let Some(value) = config.remove("filename") {
            filename = value.as_str().expect("filename string").to_string();
        }
        if !config.is_empty() {
            panic!("unknown config keys: {:?}", config.keys().collect::<Vec<&String>>());
        }
        let file = File::create(filename).unwrap();
        let mut file = BufWriter::new(file);
        file.write_all(b"Syndrome Pattern v1.0   <initializer> <positions> <syndrome_pattern>*\n")
            .unwrap();
        serde_json::to_writer(&mut file, &initializer).unwrap(); // large object write to file directly
        file.write_all(b"\n").unwrap();
        serde_json::to_writer(&mut file, &code.get_positions()).unwrap();
        file.write_all(b"\n").unwrap();
        Self { file }
    }
}

impl SolverTrait for SolverErrorPatternLogger {
    fn clear(&mut self) {}
    fn solve_visualizer(&mut self, syndrome_pattern: SyndromePattern, _visualizer: Option<&mut Visualizer>) {
        self.file
            .write_all(
                serde_json::to_string(&serde_json::json!(syndrome_pattern))
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        self.file.write_all(b"\n").unwrap();
    }
    fn subgraph_range_visualizer(&mut self, _visualizer: Option<&mut Visualizer>) -> (OutputSubgraph, WeightRange) {
        panic!("error pattern logger do not actually solve the problem, please use Verifier::None by `--verifier none`")
    }
    fn sum_dual_variables(&self) -> Rational {
        panic!("error pattern logger do not actually solve the problem")
    }
    fn generate_profiler_report(&self) -> serde_json::Value {
        json!({})
    }
    fn get_tuning_time(&self) -> Option<f64> {
        None
    }
    fn clear_tuning_time(&mut self) {}
    fn get_model_graph(&self) -> Arc<ModelHyperGraph> {
        panic!("error pattern logger do not actually solve the problem")
    }
    fn update_weights(&mut self, _new_weights: &mut Vec<f64>, _mix_ratio: f64) {
        panic!("error pattern logger do not actually solve the problem")
    }
}

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SolverSerialUnionFind>()?;
    m.add_class::<SolverSerialSingleHair>()?;
    m.add_class::<SolverSerialJointSingleHair>()?;
    m.add_class::<SolverErrorPatternLogger>()?;
    // add Solver as default class
    m.add("Solver", m.getattr("SolverSerialJointSingleHair")?)?;
    Ok(())
}
