

/// convert a modular 2 matrix into its Echelon form for Gaussian elimination:
/// https://en.wikipedia.org/wiki/Row_echelon_form
pub fn modular_2_row_echelon_form(matrix: &mut Vec::<Vec<u8>>) {
    if matrix.is_empty() { return }
    let height = matrix.len();
    if matrix[0].is_empty() { return }
    let width = matrix[0].len();
    let mut lead = 0;
    for r in 0..height {
        if lead >= width {
            return
        }
        let mut i = r;
        while matrix[i][lead] == 0 {  // find first non-zero lead
            i = i + 1;
            if i == height {
                i = r;
                lead = lead + 1;  // consider the next lead
                if lead == width {
                    return
                }
            }
        }
        if i != r {  // implies r < i
            let (slice_1, slice_2) = matrix.split_at_mut(i);
            std::mem::swap(&mut slice_1[r], &mut slice_2[0]);
        }
        for j in 0..height {
            if j != r && matrix[j][lead] != 0 {
                for k in lead..width {
                    matrix[j][k] = (matrix[j][k] + matrix[r][k]) % 2;
                }
            }
        }
        lead = lead + 1;
    }
}

pub fn assert_matrix_valid_shape<T>(matrix: &Vec::<Vec<T>>) -> Result<(usize, usize), String> {
    let height = matrix.len();
    if height == 0 {
        return Ok((0, 0))
    }
    let width = matrix[0].len();
    for i in 1..height {
        if matrix[i].len() != width {
            return Err(format!("matrix row not equally sized: first row has {} elements while {}-th row has {} elements", width, i, matrix[i].len()))
        }
    }
    Ok((height, width))
}

pub fn eprint_matrix<T: std::fmt::Display>(matrix: &Vec::<Vec<T>>) {
    let height = matrix.len();
    if height == 0 {
        eprintln!("vec![]");
        return
    }
    let width = matrix[0].len();
    eprintln!("vec![");
    for i in 0..height {
        eprint!("   vec![");
        for j in 0..width {
            if j > 0 {
                eprint!(", ");
            }
            eprint!("{}", matrix[i][j]);
        }
        eprintln!("]");
    }
    eprintln!("]");
}

pub fn assert_matrix_equal<T: Eq + std::fmt::Display>(matrix_1: &Vec::<Vec<T>>, matrix_2: &Vec::<Vec<T>>) {
    let (height_1, width_1) = assert_matrix_valid_shape(matrix_1).unwrap();
    let (height_2, width_2) = assert_matrix_valid_shape(matrix_2).unwrap();
    assert_eq!(height_1, height_2, "height of matrix not equal");
    assert_eq!(width_1, width_2, "height of matrix not equal");
    let mut fist_unequal_item = None;
    for i in 0..height_1 {
        let row_1 = &matrix_1[i];
        let row_2 = &matrix_2[i];
        for j in 0..width_1 {
            if row_1[j] != row_2[j] {
                fist_unequal_item = Some((i, j));
                break;
            }
        }
        if fist_unequal_item.is_some() {
            break
        }
    }
    match fist_unequal_item {
        Some((i, j)) => {
            eprint!("matrix 1: ");
            eprint_matrix(matrix_1);
            eprint!("matrix 2: ");
            eprint_matrix(matrix_2);
            panic!("found unequal value at ({i}, {j})")
        }, None => { }
    }
}


#[cfg(test)]
mod tests {
    use super::*;


    // to generate more tests, run python command
    // [1 if random.random() < 0.5 else 0 for _ in range(9)]
    #[test]
    fn modular_2_row_echelon_form_1() {  // cargo test modular_2_row_echelon_form_1 -- --nocapture
        let mut matrix = vec![
            vec![0, 1, 0, 1, 1, 0, 0, 1, 0],
            vec![1, 1, 0, 1, 1, 0, 0, 1, 1],
            vec![0, 1, 0, 0, 0, 1, 0, 1, 0],
            vec![1, 1, 0, 1, 0, 0, 1, 1, 1],
        ];
        modular_2_row_echelon_form(&mut matrix);
        let expected_matrix = vec![
            vec![1, 0, 0, 0, 0, 0, 0, 0, 1],
            vec![0, 1, 0, 0, 0, 1, 0, 1, 0],
            vec![0, 0, 0, 1, 0, 1, 1, 0, 0],
            vec![0, 0, 0, 0, 1, 0, 1, 0, 0],
        ];
        assert_matrix_equal(&matrix, &expected_matrix);
    }

}
