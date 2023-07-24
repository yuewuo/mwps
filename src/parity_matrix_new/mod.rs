pub mod basic_matrix;
pub mod matrix;
pub mod row;
pub mod table;

#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

pub use basic_matrix::BasicMatrix;
pub use row::ParityRow;

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<BasicMatrix>()?;
    m.add_class::<ParityRow>()?;
    Ok(())
}
