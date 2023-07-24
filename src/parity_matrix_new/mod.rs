pub mod echelon_matrix;
pub mod parity_matrix;
pub mod row;
pub mod table;

#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

pub use echelon_matrix::EchelonMatrix;
pub use parity_matrix::ParityMatrix;
pub use row::ParityRow;
pub use table::{VizTable, VizTrait};

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<ParityMatrix>()?;
    m.add_class::<ParityRow>()?;
    Ok(())
}
