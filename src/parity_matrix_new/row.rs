use derivative::Derivative;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

pub type BitArrayUnit = usize;
pub const BIT_UNIT_LENGTH: usize = std::mem::size_of::<BitArrayUnit>() * 8;
pub type DualVariableTag = usize;

/// optimize for small clusters where there are no more than 63 edges
#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct ParityRow {
    /// the first BIT_UNIT_LENGTH-1 edges are stored here, and the last bit is used the right hand bit value
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    first: BitArrayUnit,
    /// the other edges
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    others: Vec<BitArrayUnit>,
}

impl ParityRow {
    pub fn new_length(variable_count: usize) -> Self {
        let mut row = ParityRow::new();
        let others_len = variable_count / BIT_UNIT_LENGTH;
        if others_len > 0 {
            row.others = vec![0; others_len];
        }
        row
    }

    pub fn set_left(&mut self, var_index: usize, value: bool) {
        if var_index < BIT_UNIT_LENGTH - 1 {
            // common case
            if value {
                self.first |= 0x01 << var_index;
            } else {
                self.first &= !(0x01 << var_index);
            }
        } else {
            let bias_index = var_index - (BIT_UNIT_LENGTH - 1);
            let others_idx = bias_index / BIT_UNIT_LENGTH;
            let bit_idx = bias_index % BIT_UNIT_LENGTH;
            if value {
                self.others[others_idx] |= 0x01 << bit_idx;
            } else {
                self.others[others_idx] &= !(0x01 << bit_idx);
            }
        }
    }

    pub fn get_left(&self, var_index: usize) -> bool {
        if var_index < BIT_UNIT_LENGTH - 1 {
            // common case
            self.first & (0x01 << var_index) != 0
        } else {
            let bias_index = var_index - (BIT_UNIT_LENGTH - 1);
            let others_idx = bias_index / BIT_UNIT_LENGTH;
            let bit_idx = bias_index % BIT_UNIT_LENGTH;
            self.others[others_idx] & (0x01 << bit_idx) != 0
        }
    }

    pub fn set_right(&mut self, value: bool) {
        if value {
            self.first |= 0x01 << (BIT_UNIT_LENGTH - 1);
        } else {
            self.first &= !(0x01 << (BIT_UNIT_LENGTH - 1));
        }
    }

    pub fn get_right(&self) -> bool {
        self.first & (0x01 << (BIT_UNIT_LENGTH - 1)) != 0
    }

    pub fn add(&mut self, other: &Self) {
        debug_assert_eq!(self.others.len(), other.others.len(), "size must be the same");
        self.first ^= other.first;
        for i in 0..self.others.len() {
            self.others[i] ^= other.others[i];
        }
    }

    pub fn is_left_all_zero(&self) -> bool {
        if self.first & !(0x01usize << (BIT_UNIT_LENGTH - 1)) != 0 {
            return false;
        }
        for &other in self.others.iter() {
            if other != 0 {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use rand::prelude::*;

    struct RowTester {
        verbose: bool,
        variable_count: usize,
        lhs: Vec<bool>,
        rhs: bool,
        // the object to be tested
        row: ParityRow,
    }

    impl RowTester {
        fn set_left(&mut self, var_index: usize, value: bool) {
            self.row.set_left(var_index, value);
            self.lhs[var_index] = value;
            self.assert_equal();
        }
        fn set_right(&mut self, value: bool) {
            self.row.set_right(value);
            self.rhs = value;
            self.assert_equal();
        }
        fn add(&mut self, other: &ParityRow) {
            self.row.add(other);
            for i in 0..self.variable_count {
                self.lhs[i] ^= other.get_left(i);
            }
            self.rhs ^= other.get_right();
            self.assert_equal();
        }
        fn is_left_all_zero(&self) -> bool {
            !self.lhs.iter().any(|x| *x)
        }
    }

    impl RowTester {
        fn new_length(variable_count: usize) -> Self {
            let row = ParityRow::new_length(variable_count);
            assert!(row.is_left_all_zero());
            Self {
                verbose: false,
                variable_count,
                lhs: vec![false; variable_count],
                rhs: false,
                row,
            }
        }
        fn assert_equal(&self) {
            if self.verbose {
                self.print();
            }
            for i in 0..self.variable_count {
                assert_eq!(self.lhs[i], self.row.get_left(i), "different at i={i}");
            }
            assert_eq!(self.rhs, self.row.get_right(), "rhs differ");
            assert_eq!(self.is_left_all_zero(), self.row.is_left_all_zero());
        }
        fn print(&self) {
            for i in 0..self.variable_count {
                print!("{}", if self.lhs[i] { 1 } else { 0 });
            }
            println!("={}", if self.rhs { 1 } else { 0 });
        }
        fn c2b(c: char) -> bool {
            match c {
                '0' => false,
                '1' => true,
                _ => unreachable!(),
            }
        }
        fn load_from_str(lhs: &str, rhs: char) -> Self {
            let mut tester = Self::new_length(lhs.len());
            for (i, c) in lhs.chars().enumerate() {
                tester.set_left(i, Self::c2b(c));
            }
            tester.set_right(Self::c2b(rhs));
            tester
        }
        fn randomize(&mut self) -> &mut Self {
            let mut rng = rand::thread_rng();
            for i in 0..self.variable_count {
                let value = rng.gen();
                self.lhs[i] = value;
                self.row.set_left(i, value);
            }
            let value = rng.gen();
            self.rhs = value;
            self.row.set_right(value);
            self
        }
    }

    #[test]
    fn parity_matrix_row_1() {
        // cargo test parity_matrix_row_1 -- --nocapture
        let mut tester = RowTester::new_length(8);
        println!("{:?}", tester.row.clone());
        tester.verbose = true;
        tester.set_left(0, true);
        assert!(!tester.row.is_left_all_zero());
        tester.set_left(4, true);
        assert!(!tester.row.is_left_all_zero());
        tester.set_left(0, false);
        assert!(!tester.row.is_left_all_zero());
        tester.set_right(true);
        tester.set_right(false);
        tester.set_left(4, false);
        assert!(tester.row.is_left_all_zero());
    }

    #[test]
    fn parity_matrix_row_2() {
        // cargo test parity_matrix_row_2 -- --nocapture
        let mut rng = rand::thread_rng();
        for variable_count in 0..200 {
            let mut tester = RowTester::new_length(variable_count);
            for _ in 0..1000 {
                let value = rng.gen();
                let var_index = rng.gen::<usize>() % (variable_count + 1);
                if var_index < variable_count {
                    tester.set_left(var_index, value);
                } else {
                    tester.set_right(value);
                }
            }
        }
    }

    /// bug found in parity_matrix_row_2
    /// 01110011010001101000001110000011111110111010010010111111010011111=0
    /// 01110011010001101000001110000011111110111010010010111111010011110=0
    /// bug cause: write logic:
    ///     self.others[others_idx] &= (!0x01) << bit_idx;  # wrong!!!
    ///     self.others[others_idx] &= !(0x01 << bit_idx);
    #[test]
    fn parity_matrix_row_3() {
        // cargo test parity_matrix_row_3 -- --nocapture
        let mut tester = RowTester::load_from_str("01110011010001101000001110000011111110111010010010111111010011111", '0');
        tester.verbose = true;
        tester.set_left(tester.variable_count - 1, false);
    }

    #[test]
    fn parity_matrix_row_4() {
        // cargo test parity_matrix_row_4 -- --nocapture
        let mut tester = RowTester::new_length(8);
        tester.randomize();
        tester.verbose = true;
        tester.add(&RowTester::new_length(8).randomize().row);
    }

    #[test]
    fn parity_matrix_row_5() {
        // cargo test parity_matrix_row_5 -- --nocapture
        for variable_count in 0..200 {
            let mut tester = RowTester::new_length(variable_count);
            for _ in 0..500 {
                tester.add(&RowTester::new_length(variable_count).randomize().row);
            }
        }
    }
}
