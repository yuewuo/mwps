use super::row::*;
use crate::util::*;
use derivative::Derivative;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// the parity matrix that is necessary to satisfy parity requirement
#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct BasicMatrix {
    /// the vertices already maintained by this parity check
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub vertices: BTreeSet<VertexIndex>,
    /// the edges maintained by this parity check, mapping to the local indices
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub edges: BTreeMap<EdgeIndex, usize>,
    /// variable index map to edge index and whether the edge is fully grown
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub variables: Vec<(EdgeIndex, bool)>,
    /// the constraints
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub constraints: Vec<ParityRow>,
}
