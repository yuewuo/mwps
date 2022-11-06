#[cfg(feature="python_binding")]
use pyo3::prelude::*;


#[cfg(feature="python_binding")]
#[pymodule]
fn mwps(_py: Python<'_>, _m: &PyModule) -> PyResult<()> {
    panic!("this project is currently a placeholder")
}
