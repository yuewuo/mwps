//! Minimum-Weight Parity Subgraph Solver
//!
//! This module includes some common usage of primal and dual modules to solve MWPS problems.
//! Note that you can call different primal and dual modules, even interchangeably, by following the examples in this file
//!

use crate::dual_module::*;
use crate::dual_module_serial::*;
use crate::primal_module::*;
use crate::primal_module_union_find::*;
use crate::util::*;
use crate::visualize::*;
// use crate::primal_module_serial::*;
use crate::example_codes::*;
use crate::framework::*;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
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

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct SolverUnionFind {
    dual_module: DualModuleSerial,
    primal_module: PrimalModuleUnionFind,
    interface_ptr: DualModuleInterfacePtr,
    model_graph: Arc<HyperModelGraph>,
}

impl MWPSVisualizer for SolverUnionFind {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        let mut value = self.primal_module.snapshot(abbrev);
        snapshot_combine_values(&mut value, self.dual_module.snapshot(abbrev), abbrev);
        snapshot_combine_values(&mut value, self.interface_ptr.snapshot(abbrev), abbrev);
        value
    }
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl SolverUnionFind {
    #[cfg_attr(feature = "python_binding", new)]
    pub fn new(initializer: &SolverInitializer) -> Self {
        let model_graph = Arc::new(HyperModelGraph::new(Arc::new(initializer.clone())));
        Self::new_model_graph(model_graph)
    }
}

impl SolverUnionFind {
    pub fn new_model_graph(model_graph: Arc<HyperModelGraph>) -> Self {
        Self {
            dual_module: DualModuleSerial::new_empty(model_graph.initializer.as_ref()),
            primal_module: PrimalModuleUnionFind::new_empty(model_graph.initializer.as_ref()),
            interface_ptr: DualModuleInterfacePtr::new(model_graph.clone()),
            model_graph,
        }
    }
}

impl PrimalDualSolver for SolverUnionFind {
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

// #[cfg_attr(feature = "python_binding", cfg_eval)]
// #[cfg_attr(feature = "python_binding", pyclass)]
// pub struct SolverSerial {
//     dual_module: DualModuleSerial,
//     primal_module: PrimalModuleSerial,
//     interface_ptr: DualModuleInterfacePtr,
//     initializer: SolverInitializer,
// }

// impl MWPSVisualizer for SolverSerial {
//     fn snapshot(&self, abbrev: bool) -> serde_json::Value {
//         let mut value = self.primal_module.snapshot(abbrev);
//         snapshot_combine_values(&mut value, self.dual_module.snapshot(abbrev), abbrev);
//         snapshot_combine_values(&mut value, self.interface_ptr.snapshot(abbrev), abbrev);
//         value
//     }
// }

// #[cfg_attr(feature = "python_binding", cfg_eval)]
// #[cfg_attr(feature = "python_binding", pymethods)]
// impl SolverSerial {
//     #[cfg_attr(feature = "python_binding", new)]
//     pub fn new(initializer: &SolverInitializer) -> Self {
//         Self {
//             dual_module: DualModuleSerial::new_empty(initializer),
//             primal_module: PrimalModuleSerial::new_empty(initializer),
//             interface_ptr: DualModuleInterfacePtr::new_empty(),
//             initializer: initializer.clone(),
//         }
//     }
// }

// impl PrimalDualSolver for SolverSerial {
//     fn clear(&mut self) {
//         self.primal_module.clear();
//         self.dual_module.clear();
//         self.interface_ptr.clear();
//     }
//     fn solve_visualizer(&mut self, syndrome_pattern: &SyndromePattern, visualizer: Option<&mut Visualizer>) {
//         if !syndrome_pattern.erasures.is_empty() {
//             unimplemented!();
//         }
//         self.primal_module.solve_visualizer(&self.interface_ptr, syndrome_pattern, &mut self.dual_module, visualizer);
//         debug_assert!({
//             let subgraph = self.subgraph();
//             self.initializer.matches_subgraph_syndrome(&subgraph, &syndrome_pattern.defect_vertices)
//         }, "the subgraph does not generate the syndrome");
//     }
//     fn subgraph_range_visualizer(&mut self, visualizer: Option<&mut Visualizer>) -> (Subgraph, WeightRange) {
//         let (subgraph, weight_range) = self.primal_module.subgraph_range(&self.interface_ptr, &mut self.dual_module, &self.initializer);
//         if let Some(visualizer) = visualizer {
//             visualizer.snapshot_combined("subgraph".to_string(), vec![&self.interface_ptr, &self.dual_module, &subgraph, &weight_range]).unwrap();
//         }
//         (subgraph, weight_range)
//     }
//     fn sum_dual_variables(&self) -> Rational { self.interface_ptr.sum_dual_variables() }
//     fn generate_profiler_report(&self) -> serde_json::Value {
//         json!({
//             // "dual": self.dual_module.generate_profiler_report(),
//             "primal": self.primal_module.generate_profiler_report(),
//         })
//     }
// }

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
