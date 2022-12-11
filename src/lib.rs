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

pub mod cli;
pub mod visualize;
pub mod dual_module;
pub mod util;
pub mod example_codes;
pub mod pointers;


#[cfg(feature="python_binding")]
use pyo3::prelude::*;


#[cfg(feature="python_binding")]
#[pymodule]
fn mwps(_py: Python<'_>, _m: &PyModule) -> PyResult<()> {
    panic!("this project is currently a placeholder")
}
