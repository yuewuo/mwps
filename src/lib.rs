#![cfg_attr(
    feature="python_binding",
    feature(cfg_eval)
)]

extern crate serde;
#[macro_use] extern crate serde_json;
#[cfg(feature="python_binding")]
extern crate pyo3;
extern crate chrono;
extern crate urlencoding;
extern crate clap;
extern crate rand_xoshiro;
extern crate parking_lot;
extern crate derivative;
extern crate num_rational;
extern crate num_traits;
extern crate more_asserts;
extern crate pbr;
extern crate rand;
extern crate prettytable;
extern crate itertools;

pub mod cli;
pub mod visualize;
pub mod dual_module;
pub mod util;
pub mod example_codes;
pub mod pointers;
pub mod dual_module_serial;
pub mod primal_module;
pub mod primal_module_union_find;
pub mod union_find;
pub mod mwps_solver;
// pub mod explore;
pub mod primal_module_serial;
pub mod parity_matrix;
pub mod framework;
pub mod plugin;
pub mod plugin_independent_single_hair;
pub mod relaxer_pool;


#[cfg(feature="python_binding")]
use pyo3::prelude::*;


#[cfg(feature="python_binding")]
#[pymodule]
fn mwps(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    util::register(py, m)?;
    visualize::register(py, m)?;
    example_codes::register(py, m)?;
    Ok(())
}
