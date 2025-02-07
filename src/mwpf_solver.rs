//! Minimum-Weight Parity Factor Solver
//!
//! This module includes some common usage of primal and dual modules to solve MWPF problems.
//! Note that you can call different primal and dual modules, even interchangeably, by following the examples in this file
//!

use crate::cluster::*;
use crate::dual_module::*;
use crate::dual_module_pq::*;
use crate::example_codes::*;
use crate::matrix::*;
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
use std::collections::BTreeSet;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufWriter;
use std::sync::Arc;

cfg_if::cfg_if! {
    if #[cfg(feature="python_binding")] {
        use crate::invalid_subgraph::*;
        use crate::util_py::*;
        use pyo3::prelude::*;
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
    fn update_weights(&mut self, new_weights: Vec<Rational>, mix_ratio: f64);
    fn get_model_graph(&self) -> Arc<ModelHyperGraph>;
}

#[cfg(feature = "python_binding")]
macro_rules! bind_trait_to_python {
    ($struct_name:ident) => {
        #[pymethods]
        impl $struct_name {
            #[pyo3(name = "clear")]
            fn py_clear(&mut self, py: Python<'_>) {
                py.allow_threads(move || self.clear())
            }
            #[pyo3(name = "solve", signature = (syndrome_pattern, visualizer=None))] // in Python, `solve` and `solve_visualizer` is the same because it can take optional parameter
            fn py_solve(&mut self, py: Python<'_>, syndrome_pattern: SyndromePattern, visualizer: Option<&mut Visualizer>) {
                py.allow_threads(move || self.solve_visualizer(syndrome_pattern, visualizer));
            }
            #[pyo3(name = "subgraph_range", signature = (visualizer=None))] // in Python, `subgraph_range` and `subgraph_range_visualizer` is the same
            fn py_subgraph_range(
                &mut self,
                py: Python<'_>,
                visualizer: Option<&mut Visualizer>,
            ) -> (PySubgraph, PyWeightRange) {
                let (subgraph, range) = py.allow_threads(move || self.subgraph_range_visualizer(visualizer));
                let mut complete_subgraph = subgraph.into_iter().collect::<Vec<EdgeIndex>>();
                complete_subgraph.sort();
                (complete_subgraph.into(), range.into())
            }
            #[pyo3(name = "subgraph", signature = (visualizer=None))]
            fn py_subgraph(&mut self, py: Python<'_>, visualizer: Option<&mut Visualizer>) -> Subgraph {
                py.allow_threads(move || self.subgraph_range_visualizer(visualizer).0.into_iter().collect())
            }
            #[pyo3(name = "sum_dual_variables")]
            fn py_sum_dual_variables(&self) -> PyRational {
                self.sum_dual_variables().clone().into()
            }
            #[pyo3(name = "load_syndrome", signature = (syndrome_pattern, visualizer=None, skip_initial_duals=false))]
            pub fn py_load_syndrome(
                &mut self,
                py: Python<'_>,
                syndrome_pattern: &SyndromePattern,
                visualizer: Option<&mut Visualizer>,
                skip_initial_duals: bool,
            ) {
                py.allow_threads(move || self.0.load_syndrome(syndrome_pattern, visualizer, skip_initial_duals))
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
            #[pyo3(name = "create_node", signature = (vertices=None, edges=None, find_existing_node=true))]
            pub fn py_create_node(
                &mut self,
                vertices: Option<&Bound<PyAny>>,
                edges: Option<&Bound<PyAny>>,
                find_existing_node: bool,
            ) -> PyResult<PyDualNodePtr> {
                let invalid_subgraph = Arc::new(self.py_construct_invalid_subgraph(vertices, edges)?);
                if find_existing_node {
                    if let Some(node) = self.py_find_node(vertices, edges)? {
                        return Ok(node);
                    }
                }
                let interface_ptr = self.0.interface_ptr.clone();
                Ok(match self.0.dual_module.mode() {
                    DualModuleMode::Search => interface_ptr.create_node(invalid_subgraph, &mut self.0.dual_module),
                    DualModuleMode::Tune => interface_ptr.create_node_tune(invalid_subgraph, &mut self.0.dual_module),
                }
                .into())
            }
            /// create a node without providing any information about the invalid cluster itself, hence no safety checks
            #[pyo3(name = "create_node_hair_unchecked", signature = (hair, vertices=None, edges=None))]
            pub fn py_create_node_hair_unchecked(
                &mut self,
                hair: &Bound<PyAny>,
                vertices: Option<&Bound<PyAny>>,
                edges: Option<&Bound<PyAny>>,
            ) -> PyResult<PyDualNodePtr> {
                let hair = py_into_btree_set(hair)?;
                assert!(!hair.is_empty(), "hair must not be empty");
                let vertices = if let Some(vertices) = vertices {
                    py_into_btree_set(vertices)?
                } else {
                    BTreeSet::new()
                };
                let edges = if let Some(edges) = edges {
                    py_into_btree_set(edges)?
                } else {
                    BTreeSet::new()
                };
                let invalid_subgraph = Arc::new(InvalidSubgraph::new_raw(vertices, edges, hair));
                let interface_ptr = self.0.interface_ptr.clone();
                Ok(match self.0.dual_module.mode() {
                    DualModuleMode::Search => interface_ptr.create_node(invalid_subgraph, &mut self.0.dual_module),
                    DualModuleMode::Tune => interface_ptr.create_node_tune(invalid_subgraph, &mut self.0.dual_module),
                }
                .into())
            }
            #[pyo3(name = "grow", signature = (length))]
            fn py_grow(&mut self, py: Python<'_>, length: PyRational) {
                let length: Rational = length.into();
                py.allow_threads(move || {
                    if let Some(max_valid_grow) = self.0.dual_module.compute_max_valid_grow() {
                        assert!(
                            length <= max_valid_grow,
                            "growth overflow: attempting to grow {} but can only grow {} maximum",
                            length,
                            max_valid_grow
                        );
                    };
                    self.0.dual_module.grow(length);
                });
            }
            #[pyo3(name = "snapshot", signature = (abbrev=true))]
            fn py_snapshot(&mut self, py: Python<'_>, abbrev: bool) -> PyObject {
                let value = py.allow_threads(move || self.0.snapshot(abbrev));
                json_to_pyobject(value)
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
            #[pyo3(name = "stop_all")]
            fn py_stop_all(&mut self) {
                let mut node_index = 0;
                use crate::num_traits::Zero;
                while let Some(node_ptr) = self.0.interface_ptr.get_node(node_index) {
                    self.0.dual_module.set_grow_rate(&node_ptr, Rational::zero());
                    node_index += 1;
                }
            }
            #[pyo3(name = "get_cluster")]
            fn py_get_cluster(&self, vertex_index: VertexIndex) -> PyCluster {
                self.get_cluster(vertex_index).into()
            }
            /// a shortcut for creating a visualizer to display the current state of the solver
            #[pyo3(name = "show")]
            fn py_show(&self, py: Python<'_>, positions: Vec<VisualizePosition>) {
                py.allow_threads(move || {
                    let mut visualizer = Visualizer::new(Some(String::new()), positions, true).unwrap();
                    visualizer.snapshot("show".to_string(), &self.0).unwrap();
                    visualizer.show_py(None, None);
                });
            }
            #[pyo3(name = "get_initializer")]
            fn py_get_initializer(&self) -> SolverInitializer {
                self.0.model_graph.initializer.as_ref().clone()
            }
            // fn __getstate__(&self) -> PyResult<SolverInitializer> {
            //     let state = json!({
            //         "config": self.0.config,
            //         "initializer": self.py_get_initializer(),
            //     });
            // }
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

macro_rules! inherit_solver_plugin_methods {
    ($struct_name:ident) => {
        impl $struct_name {
            pub fn get_cluster(&self, vertex_index: VertexIndex) -> Cluster {
                self.0.get_cluster(vertex_index)
            }
        }
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolverSerialPluginsConfig {
    #[serde(flatten)]
    flatten_primal: PrimalModuleSerialConfig,
    /// legacy config
    primal: Option<PrimalModuleSerialConfig>,
}

pub struct SolverSerialPlugins {
    dual_module: DualModulePQ,
    primal_module: PrimalModuleSerial,
    interface_ptr: DualModuleInterfacePtr,
    model_graph: Arc<ModelHyperGraph>,
    config: SolverSerialPluginsConfig,
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
        primal_module.config = config.primal.as_ref().unwrap_or(&config.flatten_primal).clone();

        Self {
            dual_module: DualModulePQ::new_empty(initializer),
            primal_module,
            interface_ptr: DualModuleInterfacePtr::new(model_graph.clone()),
            model_graph,
            config,
        }
    }

    /// APIs for step-by-step solving in Python
    pub fn load_syndrome(
        &mut self,
        syndrome_pattern: &SyndromePattern,
        visualizer: Option<&mut Visualizer>,
        skip_initial_duals: bool,
    ) {
        if !skip_initial_duals {
            self.interface_ptr
                .load(Arc::new(syndrome_pattern.clone()), &mut self.dual_module);
            self.primal_module.load(&self.interface_ptr, &mut self.dual_module);
        } else {
            self.interface_ptr
                .write()
                .decoding_graph
                .set_syndrome(Arc::new(syndrome_pattern.clone()));
            // also manually set the defect flag in the dual module
            for &vertex_index in syndrome_pattern.defect_vertices.iter() {
                self.dual_module.vertices[vertex_index].write().is_defect = true;
            }
        }
        if let Some(visualizer) = visualizer {
            visualizer
                .snapshot_combined(
                    "syndrome loaded".to_string(),
                    vec![&self.interface_ptr, &self.dual_module, &self.primal_module],
                )
                .unwrap();
        }
    }

    /// get the cluster information of a vertex
    pub fn get_cluster(&self, vertex_index: VertexIndex) -> Cluster {
        let mut cluster = Cluster::new();
        // visit the graph via tight edges
        let mut current_vertices = BTreeSet::new();
        current_vertices.insert(vertex_index);
        while !current_vertices.is_empty() {
            let mut next_vertices = BTreeSet::new();
            for &vertex_index in current_vertices.iter() {
                cluster.add_vertex(vertex_index);
                for &edge_index in self.model_graph.get_vertex_neighbors(vertex_index).iter() {
                    if self.dual_module.is_edge_tight(edge_index) {
                        cluster.add_edge(edge_index);
                        cluster.parity_matrix.add_tight_variable(edge_index);
                        for &next_vertex_index in self.model_graph.get_edge_neighbors(edge_index).iter() {
                            if !cluster.vertices.contains(&next_vertex_index) {
                                next_vertices.insert(next_vertex_index);
                            }
                        }
                    } else {
                        cluster.add_hair(edge_index);
                    }
                }
            }
            current_vertices = next_vertices;
        }
        // add dual variables
        for &edge_index in cluster.edges.iter() {
            for node_ptr in self.dual_module.get_edge_nodes(edge_index).iter() {
                cluster.nodes.insert(node_ptr.clone().into());
            }
        }
        // construct the parity matrix
        let interface = self.interface_ptr.read();
        for &vertex_index in cluster.vertices.iter() {
            let incident_edges = self.model_graph.get_vertex_neighbors(vertex_index);
            let parity = interface.decoding_graph.is_vertex_defect(vertex_index);
            cluster.parity_matrix.add_constraint(vertex_index, incident_edges, parity);
        }
        cluster
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
    fn update_weights(&mut self, new_weights: Vec<Rational>, mix_ratio: f64) {
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
            fn update_weights(&mut self, new_weights: Vec<Rational>, mix_ratio: f64) {
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
    pub fn new_python(py: Python<'_>, initializer: &SolverInitializer, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        py.allow_threads(move || Self::new(initializer, config))
    }
}

bind_solver_trait!(SolverSerialUnionFind);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverSerialUnionFind);
inherit_solver_plugin_methods!(SolverSerialUnionFind);

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
    pub fn new_python(py: Python<'_>, initializer: &SolverInitializer, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        py.allow_threads(move || Self::new(initializer, config))
    }
}

bind_solver_trait!(SolverSerialSingleHair);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverSerialSingleHair);
inherit_solver_plugin_methods!(SolverSerialSingleHair);

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
    pub fn new_python(py: Python<'_>, initializer: &SolverInitializer, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        py.allow_threads(move || Self::new(initializer, config))
    }
}

bind_solver_trait!(SolverSerialJointSingleHair);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverSerialJointSingleHair);
inherit_solver_plugin_methods!(SolverSerialJointSingleHair);

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
    fn update_weights(&mut self, _new_weights: Vec<Rational>, _mix_ratio: f64) {
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
