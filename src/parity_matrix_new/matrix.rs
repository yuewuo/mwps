use super::*;
use crate::dual_module::*;
use crate::hyper_decoding_graph::*;
use crate::parity_matrix_visualize::*;
use crate::prettytable::*;
use crate::util::*;
use derivative::Derivative;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct EchelonInfo {
    /// (is_dependent, if dependent the only "1" position row)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub echelon_column_info: Vec<(bool, usize)>,
    /// the number of effective rows
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub echelon_effective_rows: usize,
    /// whether it's a satisfiable matrix, only valid when `is_echelon_form` is true
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub echelon_satisfiable: bool,
    /// the leading "1" position column
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub echelon_row_info: Vec<usize>,
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct ParityMatrix {
    /// matrix itself
    pub matrix: BasicMatrix,
    /// information about the matrix when it's formatted into the Echelon form;
    echelon_info: EchelonInfo,

    /// edges that are affected by any implicit shrink event
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub implicit_shrunk_edges: BTreeSet<EdgeIndex>,
    /// edges that are not visible to outside, e.g. implicitly added to keep the constraints complete;
    /// these edges must be explicitly added to remove from phantom edges
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub phantom_edges: BTreeSet<EdgeIndex>,
    /// whether to keep phantom edges or not, default to True; needed when dynamically changing tight edges
    #[derivative(Default(value = "true"))]
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub keep_phantom_edges: bool,
}

impl std::ops::Deref for ParityMatrix {
    type Target = BasicMatrix;
    fn deref(&self) -> &Self::Target {
        &self.matrix
    }
}

impl std::ops::DerefMut for ParityMatrix {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.matrix
    }
}
