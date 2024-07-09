const fn num_bits<T>() -> usize {
    std::mem::size_of::<T>() * 8
}

pub fn log(x: usize) -> u32 {
    num_bits::<i32>() as u32 - (x as i32).leading_zeros() - 1
}

#[cfg(test)]
mod log {
    use super::log;

    #[test]
    fn returns_log_of_numbers_greater_than_zero() {
        assert_eq!(log(1), 0);
        assert_eq!(log(2), 1);
        assert_eq!(log(4), 2);
        assert_eq!(log(8), 3);
        assert_eq!(log(16), 4);
        assert_eq!(log(32), 5);
        assert_eq!(log(64), 6);
        assert_eq!(log(128), 7);
        assert_eq!(log(256), 8);
        assert_eq!(log(512), 9);
    }
}
