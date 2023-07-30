pub mod basic;
pub mod echelon;
pub mod hair;
pub mod interface;
pub mod row;
pub mod tail;
pub mod tight;
pub mod visualize;

#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

pub use basic::BasicMatrix;
pub use echelon::Echelon;
pub use interface::*;
pub use tail::Tail;
pub use tight::Tight;
pub use visualize::{VizTable, VizTrait};

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<ParityMatrix>()?;
    m.add_class::<ParityRow>()?;
    Ok(())
}
