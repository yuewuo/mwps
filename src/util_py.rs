use crate::dual_module::*;
use crate::util::*;
use pyo3::prelude::*;

#[derive(Debug, Clone)]
#[pyclass(name = "Rational")]
pub struct PyRational(pub Rational);

#[derive(Debug, Clone)]
#[pyclass(name = "DualNodePtr")]
pub struct PyDualNodePtr(pub DualNodePtr);

#[derive(Debug, Clone)]
#[pyclass(name = "OrderedDualNodePtr")]
pub struct PyOrderedDualNodePtr {
    pub index: NodeIndex,
    pub ptr: PyDualNodePtr,
}

#[derive(Debug, Clone)]
#[pyclass(name = "MaxUpdateLength")]
pub enum PyMaxUpdateLength {
    Unbounded(),
    ValidGrow(PyRational),
    Conflicting(EdgeIndex),
    ShrinkProhibited(PyOrderedDualNodePtr),
}

#[derive(Debug, Clone)]
#[pyclass(name = "GroupMaxUpdateLength")]
pub enum PyGroupMaxUpdateLength {
    Unbounded(),
    ValidGrow(PyRational),
    Conflicts(Vec<PyMaxUpdateLength>),
}

#[pyfunction]
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRational>()?;
    m.add_class::<PyDualNodePtr>()?;
    m.add_class::<PyOrderedDualNodePtr>()?;
    m.add_class::<PyMaxUpdateLength>()?;
    m.add_class::<PyGroupMaxUpdateLength>()?;
    m.add_class::<DualModuleMode>()?;
    Ok(())
}
