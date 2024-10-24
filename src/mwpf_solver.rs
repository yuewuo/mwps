//! Minimum-Weight Parity Factor Solver
//!
//! This module includes some common usage of primal and dual modules to solve MWPF problems.
//! Note that you can call different primal and dual modules, even interchangeably, by following the examples in this file
//!

use crate::dual_module::*;
// use crate::dual_module_serial::*;
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
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::prelude::*;
use std::io::BufWriter;
use std::sync::Arc;

pub trait PrimalDualSolver {
    fn clear(&mut self);
    fn solve_visualizer(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>);
    fn solve(&mut self, syndrome_pattern: &SyndromePattern) {
        self.solve_visualizer(syndrome_pattern, None)
    }
    fn subgraph_range_visualizer(&mut self, visualizer: Option<&mut Visualizer>) -> (Subgraph, WeightRange);
    fn subgraph_range(&mut self) -> (Subgraph, WeightRange) {
        self.subgraph_range_visualizer(None)
    }
    fn subgraph(&mut self) -> Subgraph {
        self.subgraph_range().0
    }
    fn sum_dual_variables(&self) -> Rational;
    fn generate_profiler_report(&self) -> serde_json::Value;

    fn get_tuning_time(&self) -> Option<f64>;
    fn clear_tuning_time(&mut self);
    fn print_clusters(&self) {
        panic!();
    }
    fn update_weights(&mut self, _new_weights: &mut Vec<f64>) {}
    fn get_model_graph(&self) -> Arc<ModelHyperGraph>;
}

#[cfg(feature = "python_binding")]
macro_rules! bind_trait_to_python {
    ($struct_name:ident) => {
        #[pymethods]
        impl $struct_name {
            #[pyo3(name = "clear")]
            fn trait_clear(&mut self) {
                self.clear()
            }
            #[pyo3(name = "solve")] // in Python, `solve` and `solve_visualizer` is the same because it can take optional parameter
            fn trait_solve(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>) {
                self.solve_visualizer(syndrome_pattern, visualizer)
            }
            #[pyo3(name = "subgraph_range")] // in Python, `subgraph_range` and `subgraph_range_visualizer` is the same
            fn trait_subgraph_range(&mut self, visualizer: Option<&mut Visualizer>) -> (Subgraph, WeightRange) {
                self.subgraph_range_visualizer(visualizer)
            }
            #[pyo3(name = "subgraph")]
            fn trait_subgraph(&mut self, visualizer: Option<&mut Visualizer>) -> Subgraph {
                self.subgraph_range_visualizer(visualizer).0
            }
            #[pyo3(name = "sum_dual_variables")]
            fn trait_sum_dual_variables(&self) -> PyResult<Py<PyAny>> {
                rational_to_pyobject(&self.sum_dual_variables())
            }
        }
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolverSerialPluginsConfig {
    /// timeout for the whole solving process in millisecond
    #[serde(default = "hyperion_default_configs::primal")]
    primal: PrimalModuleSerialConfig,
    /// growing strategy
    #[serde(default = "hyperion_default_configs::growing_strategy")]
    growing_strategy: GrowingStrategy,
    #[cfg(feature = "cluster_size_limit")]
    /// cluster size limit for the primal module in the tuning phase
    /// this is the threshold for which LP will not be ran on a specific cluster to optimize the solution
    pub tuning_cluster_size_limit: Option<usize>,
}

pub mod hyperion_default_configs {
    use crate::primal_module_serial::*;

    pub fn primal() -> PrimalModuleSerialConfig {
        serde_json::from_value(json!({})).unwrap()
    }

    pub fn growing_strategy() -> GrowingStrategy {
        GrowingStrategy::MultipleClusters
    }
}

pub struct SolverSerialPlugins {
    // dual_module: DualModuleSerial,
    dual_module: DualModulePQ<FutureObstacleQueue<Rational>>,
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
        primal_module.growing_strategy = config.growing_strategy;
        primal_module.plugins = plugins;
        primal_module.config = config.primal.clone();

        #[cfg(feature = "cluster_size_limit")]
        {
            primal_module.cluster_node_limit = config.tuning_cluster_size_limit;
        }

        Self {
            dual_module: DualModulePQ::new_empty(initializer),
            // dual_module: DualModuleSerial::new_empty(initializer),
            primal_module,
            interface_ptr: DualModuleInterfacePtr::new(model_graph.clone()),
            model_graph,
        }
    }
}

impl PrimalDualSolver for SolverSerialPlugins {
    fn clear(&mut self) {
        self.primal_module.clear();
        self.dual_module.clear();
        self.interface_ptr.clear();
    }
    fn solve_visualizer(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>) {
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
    fn subgraph_range_visualizer(&mut self, visualizer: Option<&mut Visualizer>) -> (Subgraph, WeightRange) {
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
    fn update_weights(&mut self, llrs: &mut Vec<f64>) {
        // note: this will be fix bp with incr_lp problem, but the bp results are scaled, such that could be less accurate/slower...

        // should update or not? If updated, then need to reset
        // let mut_model_graph = unsafe { Arc::get_mut_unchecked(&mut self.model_graph) };
        // let mut_initializer = unsafe { Arc::get_mut_unchecked(&mut mut_model_graph.initializer) };

        for (hyper_edge, new_weight) in self.model_graph.initializer.weighted_edges.iter().zip(llrs.iter_mut()) {
            let mut temp = 1. / (1. + new_weight.exp()) * hyper_edge.weight as f64;
            let eps = 1e-14;
            temp = if temp > 1. - eps {
                1. - eps
            } else if temp < eps {
                eps
            } else {
                temp
            };
            *new_weight = -temp.ln();
            // hyper_edge.weight = (*new_weight).round() as usize;
        }

        self.dual_module.update_weights(llrs);
    }
    fn get_model_graph(&self) -> Arc<ModelHyperGraph> {
        self.model_graph.clone()
    }
}

macro_rules! bind_primal_dual_solver_trait {
    ($struct_name:ident) => {
        impl PrimalDualSolver for $struct_name {
            fn clear(&mut self) {
                self.0.clear()
            }
            fn solve_visualizer(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>) {
                self.0.solve_visualizer(syndrome_pattern, visualizer)
            }
            fn subgraph_range_visualizer(&mut self, visualizer: Option<&mut Visualizer>) -> (Subgraph, WeightRange) {
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
            fn update_weights(&mut self, llrs: &mut Vec<f64>) {
                self.0.update_weights(llrs)
            }
            fn get_model_graph(&self) -> Arc<ModelHyperGraph> {
                self.0.model_graph.clone()
            }
        }
    };
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
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
    pub fn new_python(initializer: &SolverInitializer, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        Self::new(initializer, config)
    }
}

bind_primal_dual_solver_trait!(SolverSerialUnionFind);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverSerialUnionFind);

#[cfg_attr(feature = "python_binding", cfg_eval)]
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
    pub fn new_python(initializer: &SolverInitializer, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        Self::new(initializer, config)
    }
}

bind_primal_dual_solver_trait!(SolverSerialSingleHair);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverSerialSingleHair);

#[cfg_attr(feature = "python_binding", cfg_eval)]
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
    pub fn new_python(initializer: &SolverInitializer, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        Self::new(initializer, config)
    }
}

bind_primal_dual_solver_trait!(SolverSerialJointSingleHair);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverSerialJointSingleHair);

#[cfg_attr(feature = "python_binding", cfg_eval)]
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

impl PrimalDualSolver for SolverErrorPatternLogger {
    fn clear(&mut self) {}
    fn solve_visualizer(&mut self, syndrome_pattern: &SyndromePattern, _visualizer: Option<&mut Visualizer>) {
        self.file
            .write_all(
                serde_json::to_string(&serde_json::json!(syndrome_pattern))
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        self.file.write_all(b"\n").unwrap();
    }
    fn subgraph_range_visualizer(&mut self, _visualizer: Option<&mut Visualizer>) -> (Subgraph, WeightRange) {
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
}

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<SolverSerialUnionFind>()?;
    m.add_class::<SolverSerialSingleHair>()?;
    m.add_class::<SolverSerialJointSingleHair>()?;
    m.add_class::<SolverErrorPatternLogger>()?;
    Ok(())
}
