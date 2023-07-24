use super::*;
use crate::util::*;
use derivative::Derivative;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct ColumnInfo {
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub is_dependent: bool,
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub row: usize,
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct EchelonInfo {
    /// (is_dependent, if dependent the only "1" position row)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub columns: Vec<ColumnInfo>,
    /// the number of effective rows
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub effective_rows: usize,
    /// whether it's a satisfiable matrix, only valid when `is_echelon_form` is true
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub satisfiable: bool,
    /// the leading "1" position column
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub rows: Vec<usize>,
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct EchelonMatrix {
    /// matrix itself
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub matrix: ParityMatrix,
    /// information about the matrix when it's formatted into the Echelon form;
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub info: EchelonInfo,
    /// variable indices of the echelon view
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub var_indices: Vec<usize>,
}

impl EchelonMatrix {
    /// use the new EchelonView to access this function
    fn row_echelon_form_reordered(&mut self, edges: &[EdgeIndex]) {
        self.info.satisfiable = false;
        if self.constraints.is_empty() {
            // no parity requirement
            self.info.satisfiable = true;
            return;
        }
        let height = self.constraints.len();
        self.matrix.edge_to_tight_var_indices_load(edges, &mut self.var_indices);
        if self.var_indices.is_empty() {
            // no variable to satisfy any requirement
            self.info.satisfiable = !self.constraints.iter().any(|x| x.get_right()); // if any RHS=1, it cannot be satisfied
            return;
        }
        let width = self.var_indices.len();
        let mut lead = 0;
        for r in 0..height {
            if lead >= width {
                // no more variables
                self.info.satisfiable = r == height || (r..height).all(|row| !self.constraints[row].get_right());
                if self.info.satisfiable {
                    self.info.effective_rows = r;
                } else {
                    // find a row with rhs=1 and swap with r row
                    self.info.effective_rows = r + 1;
                    if !self.constraints[r].get_right() {
                        // make sure display is reasonable: RHS=1 and all LHS=0
                        for row in r + 1..height {
                            if self.constraints[row].get_right() {
                                let (slice_1, slice_2) = self.constraints.split_at_mut(r + 1);
                                std::mem::swap(&mut slice_1[r], &mut slice_2[row - r - 1]);
                                break;
                            }
                        }
                    }
                }
                return;
            }
            let mut i = r;
            while !self.constraints[i].get_left(self.var_indices[lead]) {
                // find first non-zero lead
                i += 1;
                if i == height {
                    i = r;
                    // couldn't find a leading 1 in this column, indicating this variable is an independent variable
                    self.info.columns[self.var_indices[lead]] = ColumnInfo {
                        is_dependent: false,
                        row: r,
                    };
                    lead += 1; // consider the next lead
                    if lead == width {
                        self.info.satisfiable = r == height || (r..height).all(|row| !self.constraints[row].get_right());
                        if self.info.satisfiable {
                            self.info.effective_rows = r;
                        } else {
                            // find a row with rhs=1 and swap with r row
                            self.info.effective_rows = r + 1;
                            if !self.constraints[r].get_right() {
                                // make sure display is reasonable: RHS=1 and all LHS=0
                                for row in r + 1..height {
                                    if self.constraints[row].get_right() {
                                        let (slice_1, slice_2) = self.constraints.split_at_mut(r + 1);
                                        std::mem::swap(&mut slice_1[r], &mut slice_2[row - r - 1]);
                                        break;
                                    }
                                }
                            }
                        }
                        return;
                    }
                }
            }
            if i != r {
                // implies r < i
                let (slice_1, slice_2) = self.constraints.split_at_mut(i);
                std::mem::swap(&mut slice_1[r], &mut slice_2[0]);
            }
            for j in 0..height {
                if j != r && self.constraints[j].get_left(self.var_indices[lead]) {
                    self.xor_row(j, r);
                }
            }
            self.info.rows[r] = self.var_indices[lead];
            self.info.columns[self.var_indices[lead]] = ColumnInfo {
                is_dependent: true,
                row: r,
            };
            self.info.effective_rows = r + 1;
            lead += 1;
        }
        while lead < width {
            self.info.columns[self.var_indices[lead]] = ColumnInfo {
                is_dependent: false,
                row: height - 1,
            };
            lead += 1;
        }
        self.info.satisfiable = true;
    }
}

impl std::ops::Deref for EchelonMatrix {
    type Target = ParityMatrix;
    fn deref(&self) -> &Self::Target {
        &self.matrix
    }
}

impl std::ops::DerefMut for EchelonMatrix {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.matrix
    }
}

// override the deref implementation
impl EchelonMatrix {
    /// need to be called whenever self.matrix.add_variable is called
    fn sync_column(&mut self) {
        let length = self.matrix.edges.len() - self.info.columns.len();
        for _ in 0..length {
            self.info.columns.push(ColumnInfo {
                is_dependent: false,
                row: 0,
            });
        }
    }

    pub fn add_variable(&mut self, edge_index: EdgeIndex) {
        self.matrix.add_variable(edge_index);
        self.sync_column();
    }

    pub fn add_variable_with_tightness(&mut self, edge_index: EdgeIndex, is_tight: bool) {
        self.matrix.add_variable_with_tightness(edge_index, is_tight);
        self.sync_column();
    }

    pub fn add_tight_variable(&mut self, edge_index: EdgeIndex) {
        self.matrix.add_tight_variable(edge_index);
        self.sync_column();
    }

    pub fn add_constraint(&mut self, vertex_index: VertexIndex, incident_edges: &[EdgeIndex], parity: bool) {
        self.matrix.add_constraint(vertex_index, incident_edges, parity);
        self.sync_column();
        self.info.rows.push(0);
        // by default all constraints are taking effect
        self.info.effective_rows = self.matrix.constraints.len();
    }
}

#[test]
fn parity_matrix_echelon_matrix_1() {
    // cargo test --features=colorful parity_matrix_echelon_matrix_1 -- --nocapture
    let mut matrix = EchelonMatrix::new();
    for edge_index in 0..7 {
        matrix.add_tight_variable(edge_index);
    }
    matrix.add_constraint(0, &[0, 1], true);
    println!("{}", matrix.info.columns.len());
    matrix.add_constraint(1, &[0, 2], false);
    matrix.add_constraint(2, &[2, 3, 5], false);
    matrix.add_constraint(3, &[1, 3, 4], false);
    matrix.add_constraint(4, &[4, 6], false);
    matrix.add_constraint(5, &[5, 6], true);
    matrix.printstd();
    assert_eq!(
        matrix.printstd_str(),
        "\
┌─┬─┬─┬─┬─┬─┬─┬─┬───┐
┊ ┊0┊1┊2┊3┊4┊5┊6┊ = ┊
╞═╪═╪═╪═╪═╪═╪═╪═╪═══╡
┊0┊1┊1┊ ┊ ┊ ┊ ┊ ┊ 1 ┊
├─┼─┼─┼─┼─┼─┼─┼─┼───┤
┊1┊1┊ ┊1┊ ┊ ┊ ┊ ┊   ┊
├─┼─┼─┼─┼─┼─┼─┼─┼───┤
┊2┊ ┊ ┊1┊1┊ ┊1┊ ┊   ┊
├─┼─┼─┼─┼─┼─┼─┼─┼───┤
┊3┊ ┊1┊ ┊1┊1┊ ┊ ┊   ┊
├─┼─┼─┼─┼─┼─┼─┼─┼───┤
┊4┊ ┊ ┊ ┊ ┊1┊ ┊1┊   ┊
├─┼─┼─┼─┼─┼─┼─┼─┼───┤
┊5┊ ┊ ┊ ┊ ┊ ┊1┊1┊ 1 ┊
└─┴─┴─┴─┴─┴─┴─┴─┴───┘
"
    );
    let edges = matrix.get_edge_indices();
    matrix.row_echelon_form_reordered(&edges);
    matrix.printstd();
    assert_eq!(
        matrix.printstd_str(),
        "\
┌─┬─┬─┬─┬─┬─┬─┬─┬───┐
┊ ┊0┊1┊2┊3┊4┊5┊6┊ = ┊
╞═╪═╪═╪═╪═╪═╪═╪═╪═══╡
┊0┊1┊ ┊ ┊1┊ ┊ ┊1┊ 1 ┊
├─┼─┼─┼─┼─┼─┼─┼─┼───┤
┊1┊ ┊1┊ ┊1┊ ┊ ┊1┊   ┊
├─┼─┼─┼─┼─┼─┼─┼─┼───┤
┊2┊ ┊ ┊1┊1┊ ┊ ┊1┊ 1 ┊
├─┼─┼─┼─┼─┼─┼─┼─┼───┤
┊3┊ ┊ ┊ ┊ ┊1┊ ┊1┊   ┊
├─┼─┼─┼─┼─┼─┼─┼─┼───┤
┊4┊ ┊ ┊ ┊ ┊ ┊1┊1┊ 1 ┊
├─┼─┼─┼─┼─┼─┼─┼─┼───┤
┊5┊ ┊ ┊ ┊ ┊ ┊ ┊ ┊   ┊
└─┴─┴─┴─┴─┴─┴─┴─┴───┘
"
    );
}
