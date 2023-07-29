pub mod basic_matrix;
pub mod echelon;
pub mod matrix_interface;
pub mod row;
pub mod tail;
pub mod tight;
pub mod viz_table;

#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

pub use basic_matrix::BasicMatrix;
pub use echelon::Echelon;
pub use matrix_interface::*;
pub use tail::Tail;
pub use tight::Tight;
pub use viz_table::{VizTable, VizTrait};

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<ParityMatrix>()?;
    m.add_class::<ParityRow>()?;
    Ok(())
}
