use crate::dual_module::*;
use crate::num_traits::{Signed, ToPrimitive};
use crate::util::*;
use crate::visualize::*;
use pyo3::basic::CompareOp;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet};
use std::collections::BTreeSet;
use std::hash::DefaultHasher;
use std::hash::{Hash, Hasher};

macro_rules! bind_trait_simple_wrapper {
    ($struct_name:ident, $py_struct_name:ident) => {
        impl From<$struct_name> for $py_struct_name {
            fn from(value: $struct_name) -> Self {
                Self(value)
            }
        }

        impl From<$py_struct_name> for $struct_name {
            fn from(value: $py_struct_name) -> Self {
                value.0
            }
        }
    };
}

#[derive(Clone)]
#[pyclass(name = "Rational")]
pub struct PyRational(pub Rational);
bind_trait_simple_wrapper!(Rational, PyRational);

#[pymethods]
impl PyRational {
    #[new]
    #[pyo3(signature = (numerator, denominator=None))]
    fn __new__(numerator: &Bound<PyAny>, denominator: Option<&Bound<PyAny>>) -> PyResult<Self> {
        cfg_if::cfg_if! {
            if #[cfg(feature="rational_weight")] {
                use num_bigint::BigInt;
                let denominator: BigInt = denominator.map(|x| x.extract::<BigInt>()).transpose()?.unwrap_or_else(|| BigInt::from(1));
                let numerator: BigInt = numerator.extract()?;
                Ok(Self(Rational::new(numerator, denominator)))
            } else {
                let denominator: f64 = denominator.map(|x| x.extract::<f64>()).transpose()?.unwrap_or(1.);
                let numerator: f64 = numerator.extract()?;
                Ok(Self(Rational::new(numerator / denominator)))
            }
        }
    }
    #[getter]
    fn numer(&self) -> PyObject {
        Python::with_gil(|py| self.0.numer().to_object(py))
    }
    #[getter]
    fn denom(&self) -> PyObject {
        Python::with_gil(|py| self.0.denom().to_object(py))
    }
    fn float(&self) -> f64 {
        self.0.to_f64().unwrap()
    }
    fn __richcmp__(&self, other: &Self, op: CompareOp) -> bool {
        op.matches(self.0.cmp(&other.0))
    }
    fn __abs__(&self) -> Self {
        self.0.abs().into()
    }
    fn __mul__(&self, other: &Self) -> Self {
        (self.0.clone() * other.0.clone()).into()
    }
    fn __truediv__(&self, other: &Self) -> Self {
        (self.0.clone() / other.0.clone()).into()
    }
    fn __add__(&self, other: &Self) -> Self {
        (self.0.clone() + other.0.clone()).into()
    }
    fn __sub__(&self, other: &Self) -> Self {
        (self.0.clone() - other.0.clone()).into()
    }
    fn __neg__(&self) -> Self {
        (-self.0.clone()).into()
    }
    fn __pos__(&self) -> Self {
        self.clone()
    }
    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
    fn __str__(&self) -> String {
        cfg_if::cfg_if! {
            if #[cfg(feature="rational_weight")] {
                format!("{}/{}", self.0.numer(), self.0.denom())
            } else {
                format!("{}", self.0.to_f64().unwrap())
            }
        }
    }
    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }
}

impl std::fmt::Debug for PyRational {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__str__())
    }
}

#[derive(Clone)]
#[pyclass(name = "DualNodePtr")]
pub struct PyDualNodePtr(pub DualNodePtr);
bind_trait_simple_wrapper!(DualNodePtr, PyDualNodePtr);

#[pymethods]
impl PyDualNodePtr {
    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
    fn __eq__(&self, other: &PyDualNodePtr) -> bool {
        self.0 == other.0
    }
    fn __str__(&self) -> String {
        format!("Node({})", self.index())
    }
    fn __hash__(&self) -> u64 {
        self.index() as u64
    }
}

impl std::fmt::Debug for PyDualNodePtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.__str__())
    }
}

#[pymethods]
impl PyDualNodePtr {
    #[getter]
    fn index(&self) -> NodeIndex {
        self.0.read_recursive().index
    }
    #[getter]
    fn dual_variable(&self) -> PyRational {
        self.0.read_recursive().get_dual_variable().into()
    }
    #[getter]
    fn grow_rate(&self) -> PyRational {
        self.0.read_recursive().grow_rate.clone().into()
    }
    #[getter]
    fn vertices(&self) -> BTreeSet<VertexIndex> {
        self.0.read_recursive().invalid_subgraph.vertices.clone()
    }
    #[getter]
    fn edges(&self) -> BTreeSet<EdgeIndex> {
        self.0.read_recursive().invalid_subgraph.edges.clone()
    }
    #[getter]
    fn hair(&self) -> BTreeSet<EdgeIndex> {
        self.0.read_recursive().invalid_subgraph.hair.clone()
    }
}

#[derive(Clone, Debug)]
#[pyclass(name = "Obstacle")]
pub enum PyObstacle {
    Conflict { edge_index: EdgeIndex },
    ShrinkToZero { dual_node_ptr: PyDualNodePtr },
}

impl From<Obstacle> for PyObstacle {
    fn from(value: Obstacle) -> Self {
        match value {
            Obstacle::Conflict { edge_index } => Self::Conflict { edge_index },
            Obstacle::ShrinkToZero { dual_node_ptr } => Self::ShrinkToZero {
                dual_node_ptr: dual_node_ptr.ptr.into(),
            },
        }
    }
}

#[pymethods]
impl PyObstacle {
    fn __repr__(&self) -> String {
        format!("{:?}", self)
    }
    fn __str__(&self) -> String {
        self.__repr__()
    }
}

#[derive(Clone, Debug)]
#[pyclass(name = "DualReport")]
pub enum PyDualReport {
    Unbounded(),
    ValidGrow(PyRational),
    Obstacles(Vec<PyObstacle>),
}

impl From<DualReport> for PyDualReport {
    fn from(value: DualReport) -> Self {
        match value {
            DualReport::Unbounded => Self::Unbounded(),
            DualReport::ValidGrow(ratio) => Self::ValidGrow(ratio.into()),
            DualReport::Obstacles(obstacles) => Self::Obstacles(obstacles.into_iter().map(|x| x.into()).collect()),
        }
    }
}

#[pymethods]
impl PyDualReport {
    fn __repr__(&self) -> String {
        format!("{:?}", self)
    }
    fn __str__(&self) -> String {
        self.__repr__()
    }
}

pub fn py_into_btree_set<'py, T: Ord + Clone + FromPyObject<'py>>(value: &Bound<'py, PyAny>) -> PyResult<BTreeSet<T>> {
    let mut result = BTreeSet::<T>::new();
    if value.is_instance_of::<PyList>() {
        let list: &Bound<PyList> = value.downcast()?;
        for element in list.iter() {
            result.insert(element.extract::<T>()?.clone());
        }
    } else if value.is_instance_of::<PySet>() {
        let list: &Bound<PySet> = value.downcast()?;
        for element in list.iter() {
            result.insert(element.extract::<T>()?.clone());
        }
    } else if value.is_instance_of::<PyDict>() {
        let dict: &Bound<PyDict> = value.downcast()?;
        assert!(
            dict.is_empty(),
            "only empty dict is supported; please use set or list instead"
        );
    } else {
        unimplemented!("unsupported python type, should be set, list or (empty)dict")
    }
    Ok(result)
}

#[derive(Clone)]
#[pyclass(name = "Subgraph")]
pub struct PySubgraph(pub Subgraph);
bind_trait_simple_wrapper!(Subgraph, PySubgraph);

#[pymethods]
impl PySubgraph {
    #[new]
    fn new(subgraph: Subgraph) -> Self {
        Self(subgraph)
    }
    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
    fn __str__(&self) -> String {
        self.__repr__()
    }
    #[pyo3(signature = (abbrev=true))]
    fn snapshot(&mut self, abbrev: bool) -> PyObject {
        json_to_pyobject(self.0.snapshot(abbrev))
    }
}

#[pyfunction]
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRational>()?;
    m.add_class::<PyDualNodePtr>()?;
    m.add_class::<PyObstacle>()?;
    m.add_class::<PyDualReport>()?;
    m.add_class::<DualModuleMode>()?;
    m.add_class::<PySubgraph>()?;
    Ok(())
}
