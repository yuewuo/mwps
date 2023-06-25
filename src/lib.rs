#![cfg_attr(feature = "python_binding", feature(cfg_eval))]

extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate cfg_if;
extern crate chrono;
extern crate clap;
extern crate derivative;
extern crate itertools;
extern crate more_asserts;
extern crate num_rational;
extern crate num_traits;
extern crate parking_lot;
extern crate pbr;
extern crate prettytable;
#[cfg(feature = "python_binding")]
extern crate pyo3;
extern crate rand;
extern crate rand_xoshiro;
extern crate urlencoding;

pub mod cli;
pub mod dual_module;
pub mod dual_module_serial;
pub mod example_codes;
pub mod mwps_solver;
pub mod pointers;
pub mod primal_module;
pub mod primal_module_union_find;
pub mod union_find;
pub mod util;
pub mod visualize;
// pub mod explore;
pub mod framework;
pub mod parity_matrix;
pub mod plugin;
pub mod plugin_independent_single_hair;
pub mod primal_module_serial;
pub mod relaxer_pool;

#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

#[cfg(feature = "python_binding")]
#[pymodule]
fn mwps(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    util::register(py, m)?;
    visualize::register(py, m)?;
    example_codes::register(py, m)?;
    Ok(())
}
