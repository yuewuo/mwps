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
        let mut primal_module = PrimalModuleSerial::new_empty(initializer, &model_graph);
        let config: SolverSerialPluginsConfig = serde_json::from_value(config).unwrap();
        primal_module.growing_strategy = config.growing_strategy;
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



// ////////////////////////////////////////////////////////////////////////////
// ////////////////////////////////////////////////////////////////////////////
// ///////////////////////////Solver Parallel /////////////////////////////////
// ////////////////////////////////////////////////////////////////////////////
// ////////////////////////////////////////////////////////////////////////////


// pub struct SolverParallel {
//     pub dual_module: DualModuleParallel<DualModuleSerial>,
//     pub primal_module: PrimalModuleParallel,
//     pub subgraph_builder: SubGraphBuilder,
// }

// impl SolverParallel {
//     pub fn new(
//         initializer: &SolverInitializer,
//         partition_info: &PartitionInfo,
//         mut primal_dual_config: serde_json::Value,
//     ) -> Self {
//         let primal_dual_config = primal_dual_config.as_object_mut().expect("config must be JSON object");
//         let mut dual_config = DualModuleParallelConfig::default();
//         let mut primal_config = PrimalModuleParallelConfig::default();
//         // remove the key "dual" from the primal_dual_config map and returns Some(value) if the key existed, or None if it did not.
//         // If the key "dual" is found, its associated value is assigned to the variable value.
//         if let Some(value) = primal_dual_config.remove("dual") {
//             dual_config = serde_json::from_value(value).unwrap();
//         }
//         // similarly, do the same to assign primal
//         if let Some(value) = primal_dual_config.remove("primal") {
//             primal_config = serde_json::from_value(value).unwrap();
//         }
//         // after removing the "dual" and "primal", if primal_dual_config is still not empty, panic
//         if !primal_dual_config.is_empty() {
//             panic!(
//                 "unknown primal_dual_config keys: {:?}",
//                 primal_dual_config.keys().collect::<Vec<&String>>()
//             );
//         }

//         // return 
//         Self {
//             dual_module: DualModuleParallel::new_config(initializer, partition_info, dual_config),
//             primal_module: PrimalModuleParallel::new_config(initializer, partition_info, primal_config),
//             subgraph_builder: SubGraphBuilder::new(initializer),
//         }
//     }
// }

// impl PrimalDualSolver for SolverParallel {
//     fn clear(&mut self) {
//         self.dual_module.clear(); // function defined for DualModuleParallel
//         self.primal_module.clear();
//         self.subgraph_builder.clear();
//     }

//     fn solve_visualizer(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>) {
//         // if erasure is not empty, load it 
//         if !syndrome_pattern.erasures.is_empty() {
//             self.subgraph_builder.load_erasures(&syndrome_pattern.erasures);
//         }

//         // return 
//         self.primal_module.parallel_solve_visualizer(syndrome_pattern, &self.dual_module, visualizer);
//     }

//     fn perfect_matching_visualizer(&mut self, visualizer: Option<&mut Visualizer>) -> PerfectMatching {
//         let useless_interface_ptr = DualModuleInterfacePtr::new_empty(); // don't actually use it
//         let perfect_matching = self
//             .primal_module
//             .perfect_matching(&useless_interface_ptr, &mut self.dual_module);
//         if let Some(visualizer) = visualizer {
//             let last_interface_ptr = &self.primal_module.units.last().unwrap().read_recursive().interface_ptr;
//             visualizer
//                 .snapshot_combined(
//                     "perfect matching".to_string(),
//                     vec![last_interface_ptr, &self.dual_module, &perfect_matching],
//                 )
//                 .unwrap();
//         }

//         // return 
//         perfect_matching
//     }

//     // 
//     // fn subgraph_visualizer(&mut self, visualizer: Option<&mut Visualizer>) -> Vec<EdgeIndex> {
//     //     let perfect_matching = self.perfect_matching();
//     //     self.subgraph_builder.load_perfect_matching(&perfect_matching);
//     //     let subgraph = self.subgraph_builder.get_subgraph();
//     //     if let Some(visualizer) = visualizer {
//     //         let last_interface_ptr = &self.primal_module.units.last().unwrap().read_recursive().interface_ptr;
//     //         visualizer
//     //             .snapshot_combined(
//     //                 "perfect matching and subgraph".to_string(),
//     //                 vec![
//     //                     last_interface_ptr,
//     //                     &self.dual_module,
//     //                     &perfect_matching,
//     //                     &VisualizeSubgraph::new(&subgraph),
//     //                 ],
//     //             )
//     //             .unwrap();
//     //     }
//     //     subgraph
//     // }

//     // fn sum_dual_variables(&self) -> Weight {
//     //     let last_unit = self.primal_module.units.last().unwrap().write(); // use the interface in the last unit
//     //     let sum_dual_variables = last_unit.interface_ptr.read_recursive().sum_dual_variables;
//     //     sum_dual_variables
//     // }
//     // fn generate_profiler_report(&self) -> serde_json::Value {
//     //     json!({
//     //         "dual": self.dual_module.generate_profiler_report(),
//     //         "primal": self.primal_module.generate_profiler_report(),
//     //     })
//     // }

// }