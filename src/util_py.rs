use crate::dual_module::*;
use crate::matrix::*;
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

type EchelonMatrix = Echelon<Tail<Tight<BasicMatrix>>>;

#[derive(Clone)]
#[pyclass(name = "EchelonMatrix")]
pub struct PyEchelonMatrix(pub EchelonMatrix);
bind_trait_simple_wrapper!(EchelonMatrix, PyEchelonMatrix);

#[pymethods]
impl PyEchelonMatrix {
    #[new]
    fn new() -> Self {
        Self(EchelonMatrix::new())
    }
    fn __repr__(&mut self) -> String {
        self.0.printstd_str()
    }
    fn __str__(&mut self) -> String {
        self.__repr__()
    }
    // MatrixBasic trait functions
    fn add_variable(&mut self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.0.add_variable(edge_index)
    }
    fn add_constraint(
        &mut self,
        vertex_index: VertexIndex,
        incident_edges: Vec<EdgeIndex>,
        parity: bool,
    ) -> Option<Vec<VarIndex>> {
        self.0.add_constraint(vertex_index, &incident_edges, parity)
    }
    fn get_lhs(&self, row: RowIndex, var_index: VarIndex) -> bool {
        self.0.get_lhs(row, var_index)
    }
    fn get_rhs(&self, row: RowIndex) -> bool {
        self.0.get_rhs(row)
    }
    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeIndex {
        self.0.var_to_edge_index(var_index)
    }
    fn edge_to_var_index(&self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.0.edge_to_var_index(edge_index)
    }
    fn exists_edge(&self, edge_index: EdgeIndex) -> bool {
        self.0.exists_edge(edge_index)
    }
    fn get_vertices(&self) -> BTreeSet<VertexIndex> {
        self.0.get_vertices()
    }
    // MatrixView trait functions
    #[getter]
    fn get_columns(&mut self) -> usize {
        self.0.columns()
    }
    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex {
        self.0.column_to_var_index(column)
    }
    fn column_to_edge_index(&self, column: ColumnIndex) -> EdgeIndex {
        self.0.column_to_edge_index(column)
    }
    #[getter]
    fn get_rows(&mut self) -> usize {
        self.0.rows()
    }
    fn get_view_edges(&mut self) -> Vec<EdgeIndex> {
        self.0.get_view_edges()
    }
    fn var_to_column_index(&mut self, var_index: VarIndex) -> Option<ColumnIndex> {
        self.0.var_to_column_index(var_index)
    }
    fn edge_to_column_index(&mut self, edge_index: EdgeIndex) -> Option<ColumnIndex> {
        self.0.edge_to_column_index(edge_index)
    }
    // MatrixTight trait functions
    fn update_edge_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.0.update_edge_tightness(edge_index, is_tight)
    }
    fn is_tight(&self, edge_index: usize) -> bool {
        self.0.is_tight(edge_index)
    }
    fn add_variable_with_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.0.add_variable_with_tightness(edge_index, is_tight)
    }
    fn add_tight_variable(&mut self, edge_index: EdgeIndex) {
        self.0.add_tight_variable(edge_index)
    }
    // MatrixTail trait functions
    fn get_tail_edges(&self) -> BTreeSet<EdgeIndex> {
        self.0.get_tail_edges().clone()
    }
    fn set_tail_edges(&mut self, edges: BTreeSet<EdgeIndex>) {
        self.0.set_tail_edges(edges.into_iter())
    }
    // MatrixEchelon trait functions
    fn get_echelon_info(&mut self) -> EchelonInfo {
        self.0.get_echelon_info().clone()
    }
    fn get_solution(&mut self) -> Option<PySubgraph> {
        self.0.get_solution().map(|x| x.into())
    }
    fn get_solution_local_minimum(&mut self, weight_of: &Bound<PyAny>) -> Option<Subgraph> {
        self.0
            .get_solution_local_minimum(|x| weight_of.call1((x,)).unwrap().extract::<usize>().unwrap())
    }
}

type TailMatrix = Tail<Tight<BasicMatrix>>;

/// TailMatrix is a matrix that allows reordering part of the columns to the end.
#[derive(Clone)]
#[pyclass(name = "TailMatrix")]
pub struct PyTailMatrix(pub TailMatrix);
bind_trait_simple_wrapper!(TailMatrix, PyTailMatrix);

#[pymethods]
impl PyTailMatrix {
    #[new]
    fn new() -> Self {
        Self(TailMatrix::new())
    }
    fn __repr__(&mut self) -> String {
        self.0.printstd_str()
    }
    fn __str__(&mut self) -> String {
        self.__repr__()
    }
    // MatrixBasic trait functions
    fn add_variable(&mut self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.0.add_variable(edge_index)
    }
    fn add_constraint(
        &mut self,
        vertex_index: VertexIndex,
        incident_edges: Vec<EdgeIndex>,
        parity: bool,
    ) -> Option<Vec<VarIndex>> {
        self.0.add_constraint(vertex_index, &incident_edges, parity)
    }
    fn xor_row(&mut self, target: RowIndex, source: RowIndex) {
        self.0.xor_row(target, source)
    }
    fn swap_row(&mut self, a: RowIndex, b: RowIndex) {
        self.0.swap_row(a, b)
    }
    fn get_lhs(&self, row: RowIndex, var_index: VarIndex) -> bool {
        self.0.get_lhs(row, var_index)
    }
    fn get_rhs(&self, row: RowIndex) -> bool {
        self.0.get_rhs(row)
    }
    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeIndex {
        self.0.var_to_edge_index(var_index)
    }
    fn edge_to_var_index(&self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.0.edge_to_var_index(edge_index)
    }
    fn exists_edge(&self, edge_index: EdgeIndex) -> bool {
        self.0.exists_edge(edge_index)
    }
    fn get_vertices(&self) -> BTreeSet<VertexIndex> {
        self.0.get_vertices()
    }
    // MatrixView trait functions
    #[getter]
    fn get_columns(&mut self) -> usize {
        self.0.columns()
    }
    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex {
        self.0.column_to_var_index(column)
    }
    fn column_to_edge_index(&self, column: ColumnIndex) -> EdgeIndex {
        self.0.column_to_edge_index(column)
    }
    #[getter]
    fn get_rows(&mut self) -> usize {
        self.0.rows()
    }
    fn get_view_edges(&mut self) -> Vec<EdgeIndex> {
        self.0.get_view_edges()
    }
    fn var_to_column_index(&mut self, var_index: VarIndex) -> Option<ColumnIndex> {
        self.0.var_to_column_index(var_index)
    }
    fn edge_to_column_index(&mut self, edge_index: EdgeIndex) -> Option<ColumnIndex> {
        self.0.edge_to_column_index(edge_index)
    }
    // MatrixTight trait functions
    fn update_edge_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.0.update_edge_tightness(edge_index, is_tight)
    }
    fn is_tight(&self, edge_index: usize) -> bool {
        self.0.is_tight(edge_index)
    }
    fn add_variable_with_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.0.add_variable_with_tightness(edge_index, is_tight)
    }
    fn add_tight_variable(&mut self, edge_index: EdgeIndex) {
        self.0.add_tight_variable(edge_index)
    }
    // MatrixTail trait functions
    fn get_tail_edges(&self) -> BTreeSet<EdgeIndex> {
        self.0.get_tail_edges().clone()
    }
    fn set_tail_edges(&mut self, edges: BTreeSet<EdgeIndex>) {
        self.0.set_tail_edges(edges.into_iter())
    }
}

type TightMatrix = Tight<BasicMatrix>;

/// TightMatrix is a matrix that hides some of the edges that are not tight while still keeping track of them when doing row operations.
#[derive(Clone)]
#[pyclass(name = "TightMatrix")]
pub struct PyTightMatrix(pub TightMatrix);
bind_trait_simple_wrapper!(TightMatrix, PyTightMatrix);

#[pymethods]
impl PyTightMatrix {
    #[new]
    fn new() -> Self {
        Self(TightMatrix::new())
    }
    fn __repr__(&mut self) -> String {
        self.0.printstd_str()
    }
    fn __str__(&mut self) -> String {
        self.__repr__()
    }
    // MatrixBasic trait functions
    fn add_variable(&mut self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.0.add_variable(edge_index)
    }
    fn add_constraint(
        &mut self,
        vertex_index: VertexIndex,
        incident_edges: Vec<EdgeIndex>,
        parity: bool,
    ) -> Option<Vec<VarIndex>> {
        self.0.add_constraint(vertex_index, &incident_edges, parity)
    }
    fn xor_row(&mut self, target: RowIndex, source: RowIndex) {
        self.0.xor_row(target, source)
    }
    fn swap_row(&mut self, a: RowIndex, b: RowIndex) {
        self.0.swap_row(a, b)
    }
    fn get_lhs(&self, row: RowIndex, var_index: VarIndex) -> bool {
        self.0.get_lhs(row, var_index)
    }
    fn get_rhs(&self, row: RowIndex) -> bool {
        self.0.get_rhs(row)
    }
    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeIndex {
        self.0.var_to_edge_index(var_index)
    }
    fn edge_to_var_index(&self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.0.edge_to_var_index(edge_index)
    }
    fn exists_edge(&self, edge_index: EdgeIndex) -> bool {
        self.0.exists_edge(edge_index)
    }
    fn get_vertices(&self) -> BTreeSet<VertexIndex> {
        self.0.get_vertices()
    }
    // MatrixView trait functions
    #[getter]
    fn get_columns(&mut self) -> usize {
        self.0.columns()
    }
    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex {
        self.0.column_to_var_index(column)
    }
    fn column_to_edge_index(&self, column: ColumnIndex) -> EdgeIndex {
        self.0.column_to_edge_index(column)
    }
    #[getter]
    fn get_rows(&mut self) -> usize {
        self.0.rows()
    }
    fn get_view_edges(&mut self) -> Vec<EdgeIndex> {
        self.0.get_view_edges()
    }
    fn var_to_column_index(&mut self, var_index: VarIndex) -> Option<ColumnIndex> {
        self.0.var_to_column_index(var_index)
    }
    fn edge_to_column_index(&mut self, edge_index: EdgeIndex) -> Option<ColumnIndex> {
        self.0.edge_to_column_index(edge_index)
    }
    // MatrixTight trait functions
    fn update_edge_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.0.update_edge_tightness(edge_index, is_tight)
    }
    fn is_tight(&self, edge_index: usize) -> bool {
        self.0.is_tight(edge_index)
    }
    fn add_variable_with_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.0.add_variable_with_tightness(edge_index, is_tight)
    }
    fn add_tight_variable(&mut self, edge_index: EdgeIndex) {
        self.0.add_tight_variable(edge_index)
    }
}

/// BasicMatrix is a matrix that provides the basic functionality
#[derive(Clone)]
#[pyclass(name = "BasicMatrix")]
pub struct PyBasicMatrix(pub BasicMatrix);
bind_trait_simple_wrapper!(BasicMatrix, PyBasicMatrix);

#[pymethods]
impl PyBasicMatrix {
    #[new]
    fn new() -> Self {
        Self(BasicMatrix::new())
    }
    fn __repr__(&mut self) -> String {
        self.0.printstd_str()
    }
    fn __str__(&mut self) -> String {
        self.__repr__()
    }
    // MatrixBasic trait functions
    fn add_variable(&mut self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.0.add_variable(edge_index)
    }
    fn add_constraint(
        &mut self,
        vertex_index: VertexIndex,
        incident_edges: Vec<EdgeIndex>,
        parity: bool,
    ) -> Option<Vec<VarIndex>> {
        self.0.add_constraint(vertex_index, &incident_edges, parity)
    }
    fn xor_row(&mut self, target: RowIndex, source: RowIndex) {
        self.0.xor_row(target, source)
    }
    fn swap_row(&mut self, a: RowIndex, b: RowIndex) {
        self.0.swap_row(a, b)
    }
    fn get_lhs(&self, row: RowIndex, var_index: VarIndex) -> bool {
        self.0.get_lhs(row, var_index)
    }
    fn get_rhs(&self, row: RowIndex) -> bool {
        self.0.get_rhs(row)
    }
    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeIndex {
        self.0.var_to_edge_index(var_index)
    }
    fn edge_to_var_index(&self, edge_index: EdgeIndex) -> Option<VarIndex> {
        self.0.edge_to_var_index(edge_index)
    }
    fn exists_edge(&self, edge_index: EdgeIndex) -> bool {
        self.0.exists_edge(edge_index)
    }
    fn get_vertices(&self) -> BTreeSet<VertexIndex> {
        self.0.get_vertices()
    }
    // MatrixView trait functions
    #[getter]
    fn get_columns(&mut self) -> usize {
        self.0.columns()
    }
    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex {
        self.0.column_to_var_index(column)
    }
    fn column_to_edge_index(&self, column: ColumnIndex) -> EdgeIndex {
        self.0.column_to_edge_index(column)
    }
    #[getter]
    fn get_rows(&mut self) -> usize {
        self.0.rows()
    }
    fn get_view_edges(&mut self) -> Vec<EdgeIndex> {
        self.0.get_view_edges()
    }
    fn var_to_column_index(&mut self, var_index: VarIndex) -> Option<ColumnIndex> {
        self.0.var_to_column_index(var_index)
    }
    fn edge_to_column_index(&mut self, edge_index: EdgeIndex) -> Option<ColumnIndex> {
        self.0.edge_to_column_index(edge_index)
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
    m.add_class::<PyEchelonMatrix>()?;
    m.add_class::<PyTailMatrix>()?;
    m.add_class::<PyTightMatrix>()?;
    m.add_class::<PyBasicMatrix>()?;
    m.add_class::<EchelonInfo>()?;
    m.add_class::<ColumnInfo>()?;
    m.add_class::<RowInfo>()?;
    Ok(())
}
