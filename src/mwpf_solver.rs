//! Minimum-Weight Parity Factor Solver
//!
//! This module includes some common usage of primal and dual modules to solve MWPF problems.
//! Note that you can call different primal and dual modules, even interchangeably, by following the examples in this file
//!

use crate::dual_module::*;
// use crate::dual_module_serial::*;
use crate::dual_module_pq::*;
use crate::dual_module_parallel::*;
use crate::example_codes::*;
use crate::model_hypergraph::*;
use crate::plugin::*;
use crate::plugin_single_hair::*;
use crate::plugin_union_find::PluginUnionFind;
use crate::primal_module::*;
use crate::primal_module_serial::*;
use crate::primal_module_parallel::*;
use crate::util::*;
use crate::visualize::*;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::prelude::*;
use std::io::BufWriter;
use std::sync::Arc;
use crate::pointers::*;

pub trait PrimalDualSolver {
    fn clear(&mut self);
    fn solve_visualizer(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>, seed: u64);
    fn solve(&mut self, syndrome_pattern: &SyndromePattern, seed: u64) {
        self.solve_visualizer(syndrome_pattern, None, seed)
    }
    fn subgraph_range_visualizer(&mut self, visualizer: Option<&mut Visualizer>, seed: u64) -> (Subgraph, WeightRange);
    fn subgraph_range(&mut self, seed: u64) -> (Subgraph, WeightRange) {
        self.subgraph_range_visualizer(None, seed)
    }
    fn subgraph(&mut self, seed: u64) -> Subgraph {
        self.subgraph_range(seed).0
    }
    fn sum_dual_variables(&self) -> Rational;
    fn generate_profiler_report(&self) -> serde_json::Value;
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
                self.solve_visualizer(syndrome_pattern, visualizer, 0)
            }
            #[pyo3(name = "subgraph_range")] // in Python, `subgraph_range` and `subgraph_range_visualizer` is the same
            fn trait_subgraph_range(&mut self, visualizer: Option<&mut Visualizer>) -> (Subgraph, WeightRange) {
                self.subgraph_range_visualizer(visualizer, 0)
            }
            #[pyo3(name = "subgraph")]
            fn trait_subgraph(&mut self, visualizer: Option<&mut Visualizer>) -> Subgraph {
                self.subgraph_range_visualizer(visualizer, 0).0
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
        let mut primal_module = PrimalModuleSerial::new_empty(initializer);
        let config: SolverSerialPluginsConfig = serde_json::from_value(config).unwrap();
        primal_module.growing_strategy = config.growing_strategy;
        primal_module.plugins = plugins;
        primal_module.config = config.primal.clone();
        Self {
            dual_module: DualModulePQ::new_empty(initializer),
            // dual_module: DualModuleSerial::new_empty(initializer),
            primal_module,
            interface_ptr: DualModuleInterfacePtr::new(),
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
    fn solve_visualizer(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>, seed: u64) {
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
                let subgraph = self.subgraph(seed);
                self.model_graph
                    .matches_subgraph_syndrome(&subgraph, &syndrome_pattern.defect_vertices)
            },
            "the subgraph does not generate the syndrome"
        );
    }
    fn subgraph_range_visualizer(&mut self, visualizer: Option<&mut Visualizer>, seed: u64) -> (Subgraph, WeightRange) {
        let (subgraph, weight_range) = self
            .primal_module
            .subgraph_range(&self.interface_ptr, seed);
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
}

macro_rules! bind_primal_dual_solver_trait {
    ($struct_name:ident) => {
        impl PrimalDualSolver for $struct_name {
            fn clear(&mut self) {
                self.0.clear()
            }
            fn solve_visualizer(
                &mut self,
                syndrome_pattern: &SyndromePattern,
                visualizer: Option<&mut Visualizer>,
                seed: u64,
            ) {
                self.0.solve_visualizer(syndrome_pattern, visualizer, seed)
            }
            fn subgraph_range_visualizer(
                &mut self,
                visualizer: Option<&mut Visualizer>,
                seed: u64,
            ) -> (Subgraph, WeightRange) {
                self.0.subgraph_range_visualizer(visualizer, seed)
            }
            fn sum_dual_variables(&self) -> Rational {
                self.0.sum_dual_variables()
            }
            fn generate_profiler_report(&self) -> serde_json::Value {
                self.0.generate_profiler_report()
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
    fn solve_visualizer(&mut self, syndrome_pattern: &SyndromePattern, _visualizer: Option<&mut Visualizer>, _seed: u64) {
        self.file
            .write_all(
                serde_json::to_string(&serde_json::json!(syndrome_pattern))
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        self.file.write_all(b"\n").unwrap();
    }
    fn subgraph_range_visualizer(&mut self, _visualizer: Option<&mut Visualizer>, _seed: u64) -> (Subgraph, WeightRange) {
        panic!("error pattern logger do not actually solve the problem, please use Verifier::None by `--verifier none`")
    }
    fn sum_dual_variables(&self) -> Rational {
        panic!("error pattern logger do not actually solve the problem")
    }
    fn generate_profiler_report(&self) -> serde_json::Value {
        json!({})
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolverParallelConfig {
    /// timeout for the whole solving process in millisecond
    #[serde(default = "parallel_hyperion_default_configs::primal")]
    primal: PrimalModuleParallelConfig,
    #[serde(default = "parallel_hyperion_default_configs::dual")]
    dual: DualModuleParallelConfig,
    /// growing strategy
    #[serde(default = "parallel_hyperion_default_configs::growing_strategy")]
    growing_strategy: GrowingStrategy,
}

pub mod parallel_hyperion_default_configs {
    use crate::primal_module_serial::GrowingStrategy;
    use crate::primal_module_parallel::*;
    use crate::dual_module_parallel::*;

    pub fn primal() -> PrimalModuleParallelConfig {
        serde_json::from_value(json!({})).unwrap()
    }

    pub fn dual() -> DualModuleParallelConfig {
        serde_json::from_value(json!({})).unwrap()
    }

    pub fn growing_strategy() -> GrowingStrategy {
        GrowingStrategy::MultipleClusters
    }
}


#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverParallel);

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct SolverParallel {
    dual_module: DualModuleParallel<DualModulePQ<FutureObstacleQueue<Rational>>, FutureObstacleQueue<Rational>>, 
    primal_module: PrimalModuleParallel,
    model_graph: Arc<ModelHyperGraph>,

    // /// timeout for the whole solving process in millisecond
    // #[serde(default = "hyperion_default_configs::primal")]
    // primal: PrimalModuleSerialConfig,
    // /// growing strategy
    // #[serde(default = "hyperion_default_configs::growing_strategy")]
    // growing_strategy: GrowingStrategy,
}

impl SolverParallel {
    pub fn new(initializer: &SolverInitializer, partition_info: &PartitionInfo, plugins: Arc<Vec<PluginEntry>>, config: serde_json::Value,) -> Self {
        let model_graph = Arc::new(ModelHyperGraph::new(Arc::new(initializer.clone())));
        let config: SolverParallelConfig = serde_json::from_value(config).unwrap();
        let primal_module = PrimalModuleParallel::new_config(&model_graph.initializer, &partition_info, config.primal.clone(), config.growing_strategy.clone(), plugins.clone());
        let dual_module: DualModuleParallel<DualModulePQ<FutureObstacleQueue<Rational>>, FutureObstacleQueue<Rational>> =
            DualModuleParallel::new_config(&initializer, &partition_info, config.dual.clone());
        
        Self {
            dual_module,
            primal_module,
            // interface_ptr: DualModuleInterfacePtr::new(),
            model_graph,
        }
    }
}

impl PrimalDualSolver for SolverParallel {
    fn clear(&mut self) {
        self.dual_module.clear();
        self.primal_module.clear();
    }
    fn solve_visualizer(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>, seed: u64) {
        let syndrome_pattern = Arc::new(syndrome_pattern.clone());
        if !syndrome_pattern.erasures.is_empty() {
            unimplemented!();
        }
        self.primal_module.parallel_solve_visualizer(
            syndrome_pattern.clone(),
            &mut self.dual_module,
            visualizer,
        );
        debug_assert!(
            {
                let subgraph = self.subgraph(seed);
                self.model_graph
                    .matches_subgraph_syndrome(&subgraph, &syndrome_pattern.defect_vertices)
            },
            "the subgraph does not generate the syndrome"
        );
    }
    fn subgraph_range_visualizer(&mut self, visualizer: Option<&mut Visualizer>, seed: u64) -> (Subgraph, WeightRange) {
        let useless_interface_ptr = DualModuleInterfacePtr::new();
        let (subgraph, weight_range) = self
            .primal_module
            .subgraph_range(&useless_interface_ptr, seed);
        if let Some(visualizer) = visualizer {
            let last_interface_ptr = &self.primal_module.units.last().unwrap().read_recursive().interface_ptr;
            visualizer
                .snapshot_combined(
                    "subgraph".to_string(),
                    vec![last_interface_ptr, &self.dual_module, &subgraph, &weight_range],
                )
                .unwrap();
        }
        (subgraph, weight_range)
    }
    fn sum_dual_variables(&self) -> Rational {
        panic!("this function is not finalized yet for parallel implementation")
    }
    fn generate_profiler_report(&self) -> serde_json::Value {
        json!({
            // "dual": self.dual_module.generate_profiler_report(),
            "primal": self.primal_module.generate_profiler_report(),
        })
    }
}

// bind_primal_dual_solver_trait!(SolverParallel); need to define a specific one for this

// macro_rules! bind_primal_dual_solver_trait {
//     ($struct_name:ident) => {
//         impl PrimalDualSolver for $struct_name {
//             fn clear(&mut self) {
//                 self.0.clear()
//             }
//             fn solve_visualizer(
//                 &mut self,
//                 syndrome_pattern: &SyndromePattern,
//                 visualizer: Option<&mut Visualizer>,
//                 seed: u64,
//             ) {
//                 self.0.solve_visualizer(syndrome_pattern, visualizer, seed)
//             }
//             fn subgraph_range_visualizer(
//                 &mut self,
//                 visualizer: Option<&mut Visualizer>,
//                 seed: u64,
//             ) -> (Subgraph, WeightRange) {
//                 self.0.subgraph_range_visualizer(visualizer, seed)
//             }
//             fn sum_dual_variables(&self) -> Rational {
//                 self.0.sum_dual_variables()
//             }
//             fn generate_profiler_report(&self) -> serde_json::Value {
//                 self.0.generate_profiler_report()
//             }
//         }
//     };
// }



#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct SolverParallelUnionFind(SolverParallel);

impl SolverParallelUnionFind {
    pub fn new(initializer: &SolverInitializer, partition_info: &PartitionInfo, config: serde_json::Value) -> Self {
        Self(SolverParallel::new(initializer, partition_info, Arc::new(vec![]), config))
    }
}

#[cfg(feature = "python_binding")]
#[pymethods]
impl SolverParallelUnionFind {
    #[new]
    pub fn new_python(initializer: &SolverInitializer, partition_info: &PartitionInfo, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        Self::new(initializer, partition_info, config)
    }
}

bind_primal_dual_solver_trait!(SolverParallelUnionFind);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverParallelUnionFind);

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct SolverParallelSingleHair(SolverParallel);

impl SolverParallelSingleHair {
    pub fn new(initializer: &SolverInitializer, partition_info: &PartitionInfo, config: serde_json::Value) -> Self {
        Self(SolverParallel::new(
            initializer,
            partition_info,
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
impl SolverParallelSingleHair {
    #[new]
    pub fn new_python(initializer: &SolverInitializer, partition_info: &PartitionInfo, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        Self::new(initializer, partition_info, config)
    }
}

bind_primal_dual_solver_trait!(SolverParallelSingleHair);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverParallelSingleHair);


#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct SolverParallelJointSingleHair(SolverParallel);

impl SolverParallelJointSingleHair {
    pub fn new(initializer: &SolverInitializer, partition_info: &PartitionInfo, config: serde_json::Value) -> Self {
        Self(SolverParallel::new(
            initializer,
            partition_info,
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
impl SolverParallelJointSingleHair {
    #[new]
    pub fn new_python(initializer: &SolverInitializer, partition_info: &PartitionInfo, config: Option<PyObject>) -> Self {
        let config = config.map(|x| pyobject_to_json(x)).unwrap_or(json!({}));
        Self::new(initializer, partition_info, config)
    }
}

bind_primal_dual_solver_trait!(SolverParallelJointSingleHair);

#[cfg(feature = "python_binding")]
bind_trait_to_python!(SolverParallelJointSingleHair);

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<SolverSerialUnionFind>()?;
    m.add_class::<SolverSerialSingleHair>()?;
    m.add_class::<SolverSerialJointSingleHair>()?;
    m.add_class::<SolverErrorPatternLogger>()?;
    m.add_class::<SolverParallelUnionFind>()?;
    m.add_class::<SolverParallelSingleHair>()?;
    m.add_class::<SolverParallelJointSingleHair>()?;
    Ok(())
}
