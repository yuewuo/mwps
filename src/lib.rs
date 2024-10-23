#![cfg_attr(feature = "python_binding", feature(cfg_eval))]
#![feature(get_mut_unchecked)]

extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate cfg_if;
extern crate chrono;
extern crate clap;
extern crate derivative;
extern crate itertools;
#[macro_use]
extern crate lazy_static;
extern crate more_asserts;
extern crate num_rational;
extern crate num_traits;
extern crate parking_lot;
#[cfg(feature = "cli")]
extern crate pbr;
extern crate prettytable;
#[cfg(feature = "python_binding")]
extern crate pyo3;
extern crate rand;
extern crate rand_xoshiro;
#[cfg(feature = "slp")]
extern crate slp;
extern crate urlencoding;
#[cfg(feature = "wasm_binding")]
extern crate wasm_bindgen;

#[cfg(feature = "cli")]
pub mod cli;
pub mod decoding_hypergraph;
pub mod dual_module;
pub mod dual_module_pq;
pub mod dual_module_serial;
pub mod example_codes;
pub mod invalid_subgraph;
pub mod matrix;
pub mod model_hypergraph;
pub mod mwpf_solver;
pub mod ordered_float;
pub mod plugin;
pub mod plugin_single_hair;
pub mod plugin_union_find;
pub mod pointers;
pub mod primal_module;
pub mod primal_module_serial;
pub mod primal_module_union_find;
pub mod relaxer;
pub mod relaxer_forest;
pub mod relaxer_optimizer;
pub mod union_find;
pub mod util;
pub mod visualize;

pub use bp;

#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

#[cfg(feature = "python_binding")]
#[pymodule]
fn mwpf(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    util::register(py, m)?;
    visualize::register(py, m)?;
    example_codes::register(py, m)?;
    mwpf_solver::register(py, m)?;
    Ok(())
}

#[cfg(feature = "wasm_binding")]
use wasm_bindgen::prelude::*;

#[cfg_attr(feature = "wasm_binding", wasm_bindgen)]
pub fn get_version() -> String {
    use decoding_hypergraph::*;
    use dual_module::*;
    use dual_module_serial::*;
    use example_codes::*;
    use primal_module::*;
    use primal_module_serial::*;
    // TODO: I'm just testing basic functionality
    let defect_vertices = vec![23, 24, 29, 30];
    let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
    // create dual module
    let model_graph = code.get_model_graph();
    let mut dual_module = DualModuleSerial::new_empty(&model_graph.initializer);
    // create primal module
    let mut primal_module = PrimalModuleSerial::new_empty(&model_graph.initializer);
    primal_module.growing_strategy = GrowingStrategy::SingleCluster;
    primal_module.plugins = std::sync::Arc::new(vec![]);
    // try to work on a simple syndrome
    let decoding_graph = DecodingHyperGraph::new_defects(model_graph, defect_vertices.clone());
    let interface_ptr = DualModuleInterfacePtr::new(decoding_graph.model_graph.clone());
    primal_module.solve_visualizer(
        &interface_ptr,
        decoding_graph.syndrome_pattern.clone(),
        &mut dual_module,
        None,
    );
    let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module);
    println!("subgraph: {subgraph:?}");
    // env!("CARGO_PKG_VERSION").to_string()
    format!("subgraph: {subgraph:?}, weight_range: {weight_range:?}")
}
