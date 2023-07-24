pub mod basic_matrix;
pub mod matrix;
pub mod row;

pub use basic_matrix::BasicMatrix;

#[cfg(feature = "python_binding")]
#[pyfunction]
pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<ParityMatrix>()?;
    m.add_class::<ParityRow>()?;
    Ok(())
}
