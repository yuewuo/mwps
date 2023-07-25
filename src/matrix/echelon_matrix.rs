use super::*;
use crate::util::*;
use derivative::Derivative;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

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

impl EchelonMatrix {
    pub fn load_var_indices(&mut self) {
        self.load_var_indices_reordered(&self.get_edge_indices());
    }

    pub fn load_var_indices_reordered(&mut self, edges: &[EdgeIndex]) {
        self.matrix.edge_to_tight_var_indices_load(edges, &mut self.var_indices);
    }

    pub fn row_echelon_form(&mut self) {
        self.row_echelon_form_reordered(&self.get_edge_indices());
    }

    /// use the new EchelonView to access this function
    pub fn row_echelon_form_reordered(&mut self, edges: &[EdgeIndex]) {
        // always load var indices
        self.load_var_indices_reordered(edges);
        if self.constraints.is_empty() {
            // no parity requirement
            self.info.satisfiable = true;
            self.info.effective_rows = 0;
            return;
        }
        self.info.satisfiable = false;
        let height = self.constraints.len();
        if self.var_indices.is_empty() {
            // no variable to satisfy any requirement
            // if any RHS=1, it cannot be satisfied
            for row in 0..height {
                if self.constraints[row].get_right() {
                    self.info.satisfiable = false;
                    self.swap_row(0, row);
                    self.info.effective_rows = 1;
                    return;
                }
            }
            self.info.satisfiable = true;
            self.info.effective_rows = 0;
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
                                self.swap_row(r, row);
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
                                        self.swap_row(r, row);
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
                self.swap_row(r, i);
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

    fn sync_row(&mut self) {
        let length = self.matrix.constraints.len() - self.info.rows.len();
        for _ in 0..length {
            self.info.rows.push(0);
        }
        if length > 0 {
            // by default all constraints are taking effect
            self.info.effective_rows = self.matrix.constraints.len();
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
        self.sync_row();
    }

    pub fn add_parity_checks(&mut self, constraints: &[(Vec<EdgeIndex>, bool)]) {
        self.matrix.add_parity_checks(constraints);
        self.sync_column();
        self.sync_row();
    }
}

impl VizTrait for EchelonMatrix {
    fn viz_table(&self) -> VizTable {
        VizTable::new(self, &self.var_indices)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::rand::{Rng, SeedableRng};

    #[test]
    fn parity_matrix_echelon_matrix_1() {
        // cargo test --features=colorful parity_matrix_echelon_matrix_1 -- --nocapture
        let mut echelon = EchelonMatrix::new();
        for edge_index in 0..7 {
            echelon.add_tight_variable(edge_index);
        }
        echelon.row_echelon_form();
        echelon.printstd();
        echelon.add_constraint(0, &[0, 1], true);
        println!("{}", echelon.info.columns.len());
        echelon.add_constraint(1, &[0, 2], false);
        echelon.add_constraint(2, &[2, 3, 5], false);
        echelon.add_constraint(3, &[1, 3, 4], false);
        echelon.add_constraint(4, &[4, 6], false);
        echelon.add_constraint(5, &[5, 6], true);
        echelon.matrix.printstd();
        assert_eq!(
            echelon.matrix.printstd_str(),
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
        echelon.row_echelon_form();
        echelon.printstd();
        assert_eq!(
            echelon.printstd_str(),
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
        assert_eq!(echelon.info.effective_rows, 5);
        let is_dependent_vec = [true, true, true, false, true, true, false];
        let corresponding_row = [0, 1, 2, usize::MAX, 3, 4, usize::MAX];
        for (i, &is_dependent) in is_dependent_vec.iter().enumerate() {
            assert_eq!(echelon.info.columns[i].is_dependent, is_dependent);
            if is_dependent {
                assert_eq!(echelon.info.columns[i].row, corresponding_row[i]);
            }
        }
        let corresponding_column = [0, 1, 2, 4, 5];
        for (i, &column) in corresponding_column.iter().enumerate() {
            assert_eq!(echelon.info.rows[i], column);
        }
        assert!(echelon.info.satisfiable);
    }

    #[test]
    fn parity_matrix_echelon_matrix_empty_is_satisfiable() {
        // cargo test --features=colorful parity_matrix_echelon_matrix_empty_is_satisfiable -- --nocapture
        let mut echelon = EchelonMatrix::new();
        for edge_index in 0..5 {
            echelon.add_variable_with_tightness(edge_index, false); // no tight edges
        }
        assert_eq!(echelon.get_edge_indices().len(), 5);
        echelon.row_echelon_form();
        echelon.printstd();
        assert_eq!(
            echelon.printstd_str(),
            "\
┌┬───┐
┊┊ = ┊
╞╪═══╡
└┴───┘
"
        );
        assert_eq!(format!("{:?}", echelon), format!("{:?}", echelon.clone()));
        assert!(echelon.info.satisfiable);
    }

    #[test]
    fn parity_matrix_echelon_matrix_no_variable_is_unsatisfiable() {
        // cargo test --features=colorful parity_matrix_echelon_matrix_no_variable_is_unsatisfiable -- --nocapture
        for rhs in [false, true] {
            let mut echelon = EchelonMatrix::new();
            for edge_index in 0..5 {
                echelon.add_variable_with_tightness(edge_index, false); // no tight edges
            }
            echelon.add_constraint(0, &[0, 1, 3], rhs);
            assert_eq!(echelon.get_edge_indices().len(), 5);
            echelon.row_echelon_form();
            echelon.printstd();
            // when rhs=1, it is not satisfiable
            assert!(echelon.info.satisfiable != rhs);
        }
    }

    #[test]
    fn parity_matrix_echelon_matrix_lead_larger_than_width() {
        // cargo test --features=colorful parity_matrix_echelon_matrix_lead_larger_than_width -- --nocapture
        for rhs in [false, true] {
            let mut echelon = EchelonMatrix::new();
            for edge_index in 0..3 {
                echelon.add_tight_variable(edge_index);
            }
            echelon.add_constraint(0, &[0, 1], false);
            echelon.add_constraint(1, &[1, 2], true);
            echelon.add_constraint(2, &[2], true);
            echelon.add_constraint(3, &[1], rhs);
            echelon.row_echelon_form();
            echelon.printstd();
            assert!(echelon.info.satisfiable != rhs);
        }
        // also try to enter the branch where it needs to search more rows
        {
            let mut echelon = EchelonMatrix::new();
            for edge_index in 0..3 {
                echelon.add_tight_variable(edge_index);
            }
            echelon.add_constraint(0, &[0, 1], false);
            echelon.add_constraint(1, &[1, 2], true);
            echelon.add_constraint(2, &[0, 1, 2], false);
            echelon.add_constraint(3, &[0, 1, 2], false);
            echelon.add_constraint(4, &[0, 1, 2], false);
            echelon.add_constraint(5, &[0, 1, 2], false);
            echelon.add_constraint(6, &[2], true);
            echelon.row_echelon_form();
            echelon.printstd();
        }
    }

    #[test]
    fn parity_matrix_echelon_matrix_lead_smaller_than_width() {
        // cargo test --features=colorful parity_matrix_echelon_matrix_lead_smaller_than_width -- --nocapture
        let mut echelon = EchelonMatrix::new();
        for edge_index in 0..5 {
            echelon.add_tight_variable(edge_index);
        }
        echelon.add_constraint(0, &[0, 1], false);
        echelon.add_constraint(1, &[1, 2], true);
        echelon.row_echelon_form();
        echelon.printstd();
        assert_eq!(
            echelon.printstd_str(),
            "\
┌─┬─┬─┬─┬─┬─┬───┐
┊ ┊0┊1┊2┊3┊4┊ = ┊
╞═╪═╪═╪═╪═╪═╪═══╡
┊0┊1┊ ┊1┊ ┊ ┊ 1 ┊
├─┼─┼─┼─┼─┼─┼───┤
┊1┊ ┊1┊1┊ ┊ ┊ 1 ┊
└─┴─┴─┴─┴─┴─┴───┘
"
        );
    }

    fn generate_random_parity_checks(
        rng: &mut DeterministicRng,
        variable_count: usize,
        constraint_count: usize,
    ) -> Vec<(Vec<EdgeIndex>, bool)> {
        let mut parity_checks = vec![];
        for _ in 0..constraint_count {
            let rhs: bool = rng.gen();
            let lhs = (0..variable_count).filter(|_| rng.gen()).collect();
            parity_checks.push((lhs, rhs))
        }
        parity_checks
    }

    struct YetAnotherRowEchelon {
        matrix: Vec<Vec<bool>>,
    }

    impl YetAnotherRowEchelon {
        fn new(echelon: &EchelonMatrix) -> Self {
            let mut matrix = vec![];
            for constraint in echelon.matrix.constraints.iter() {
                let mut row = vec![];
                for &var_index in echelon.var_indices.iter() {
                    row.push(constraint.get_left(var_index));
                }
                row.push(constraint.get_right());
                matrix.push(row);
            }
            Self { matrix }
        }

        // https://rosettacode.org/wiki/Reduced_row_echelon_form#Rust
        fn reduced_row_echelon_form(&mut self) {
            let matrix = &mut self.matrix;
            let mut pivot = 0;
            let row_count = matrix.len();
            if row_count == 0 {
                return;
            }
            let column_count = matrix[0].len();
            'outer: for r in 0..row_count {
                if column_count <= pivot {
                    break;
                }
                let mut i = r;
                while !matrix[i][pivot] {
                    i += 1;
                    if i == row_count {
                        i = r;
                        pivot += 1;
                        if column_count == pivot {
                            break 'outer;
                        }
                    }
                }
                for j in 0..column_count {
                    let temp = matrix[r][j];
                    matrix[r][j] = matrix[i][j];
                    matrix[i][j] = temp;
                }
                for i in 0..row_count {
                    if i != r && matrix[i][pivot] {
                        for k in 0..column_count {
                            matrix[i][k] ^= matrix[r][k];
                        }
                    }
                }
                pivot += 1;
            }
        }

        fn print(&self) {
            for row in self.matrix.iter() {
                for &e in row.iter() {
                    print!("{}", if e { 1 } else { 0 });
                }
                println!();
            }
        }

        fn is_satisfiable(&self) -> bool {
            'outer: for row in self.matrix.iter() {
                if row[row.len() - 1] {
                    for &e in row.iter().take(row.len() - 1) {
                        if e {
                            continue 'outer;
                        }
                    }
                    return false;
                }
            }
            true
        }

        fn effective_rows(&self) -> usize {
            let mut effective_rows = 0;
            for (i, row) in self.matrix.iter().enumerate() {
                for &e in row.iter() {
                    if e {
                        effective_rows = i + 1;
                    }
                }
            }
            effective_rows
        }

        fn assert_eq(&self, echelon: &EchelonMatrix) {
            let satisfiable = self.is_satisfiable();
            assert_eq!(satisfiable, echelon.info.satisfiable);
            if !satisfiable {
                // assert effective_rows is the line where it fails
                return;
            }
            let effective_rows = self.effective_rows();
            assert_eq!(echelon.info.effective_rows, effective_rows);
            for (i, row) in self.matrix.iter().enumerate() {
                for (j, &e) in row.iter().enumerate() {
                    if j < row.len() - 1 {
                        assert_eq!(e, echelon.get_lhs(i, j));
                    } else {
                        assert_eq!(e, echelon.get_rhs(i));
                    }
                }
            }
        }
    }

    #[test]
    fn parity_matrix_echelon_matrix_another_echelon() {
        // cargo test --features=colorful parity_matrix_echelon_matrix_another_echelon -- --nocapture
        let mut echelon = EchelonMatrix::new();
        for edge_index in 0..7 {
            echelon.add_tight_variable(edge_index);
        }
        echelon.add_constraint(0, &[0, 1], true);
        echelon.add_constraint(1, &[0, 2], false);
        echelon.add_constraint(2, &[2, 3, 5], false);
        echelon.add_constraint(3, &[1, 3, 4], false);
        echelon.add_constraint(4, &[4, 6], false);
        echelon.add_constraint(5, &[5, 6], true);
        echelon.load_var_indices();
        echelon.printstd();
        let mut another = YetAnotherRowEchelon::new(&echelon);
        another.print();
        // both go to echelon form
        echelon.row_echelon_form();
        echelon.printstd();
        another.reduced_row_echelon_form();
        another.print();
        another.assert_eq(&echelon);
    }

    #[test]
    fn parity_matrix_echelon_matrix_random_tests() {
        // cargo test --features=colorful parity_matrix_echelon_matrix_random_tests -- --nocapture
        let mut rng = DeterministicRng::seed_from_u64(123);
        let repeat = 50;
        for variable_count in 0..31 {
            for constraint_count in 0..31 {
                for _ in 0..repeat {
                    let mut echelon = EchelonMatrix::new();
                    for edge_index in 0..variable_count {
                        echelon.add_tight_variable(edge_index);
                    }
                    let parity_checks = generate_random_parity_checks(&mut rng, variable_count, constraint_count);
                    echelon.add_parity_checks(&parity_checks);
                    echelon.load_var_indices();
                    // echelon.printstd();
                    let mut another = YetAnotherRowEchelon::new(&echelon);
                    if variable_count > 0 {
                        another.reduced_row_echelon_form();
                        echelon.row_echelon_form();
                        // echelon.printstd();
                        // another.print();
                        another.assert_eq(&echelon);
                    }
                }
            }
        }
    }

    /// panicked at 'index out of bounds: the len is 0 but the index is 0', src/matrix/echelon_matrix.rs:148:13
    #[test]
    fn parity_matrix_echelon_matrix_debug_1() {
        // cargo test --features=colorful parity_matrix_echelon_matrix_debug_1 -- --nocapture
        let mut echelon = EchelonMatrix::new();
        echelon.add_tight_variable(0);
        echelon.add_tight_variable(1);
        let parity_checks = vec![(vec![0], true), (vec![0, 1], true), (vec![], true)];
        echelon.add_parity_checks(&parity_checks);
        echelon.row_echelon_form();
        echelon.printstd();
    }
}
