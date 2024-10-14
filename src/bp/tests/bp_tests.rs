#[cfg(test)]
mod tests {
    use bp_decoder::{bp::*, sparse_matrix_util::print_sparse_matrix};

    #[test]
    fn bp_entry_init() {
        let e = BpEntry::default();
        assert_eq!(e.row_index, -100);
        assert_eq!(e.col_index, -100);
        assert!(e.at_end());
        assert_eq!(e.inner.bit_to_check_msg, 0.0);
        assert_eq!(e.inner.check_to_bit_msg, 0.0);
    }

    #[test]
    fn bp_sparse_init() {
        let n = 3;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..n - 1 {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let expected = "1 1 0\n0 1 1";
        assert_eq!(print_sparse_matrix(&pcm.base, true), expected);
    }

    #[test]
    fn bp_decoder_initialization_test() {
        let n = 3;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        let pcm_n = pcm.base.n;
        for i in 0..n - 1 {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1; n];
        let decoder = BpDecoder::new_3(
            &mut pcm,
            channel_probabilities,
            pcm_n, // maximum iterations
        )
        .unwrap();

        assert_eq!(decoder.pcm.base.m, decoder.check_count);
        assert_eq!(decoder.pcm.base.n, decoder.bit_count);
        assert_eq!(decoder.channel_probabilities, vec![0.1; n]);
        assert_eq!(decoder.maximum_iterations, pcm_n);
        assert_eq!(0.625, decoder.ms_scaling_factor);
        assert!(matches!(decoder.method, BpMethod::ProductSum));
        assert!(matches!(decoder.schedule, BpSchedule::Parallel));
        assert_eq!(decoder.omp_thread_count, 1);
    }

    #[test]
    fn bp_decoder_initialization_with_optional_parameters_test() {
        let n = 4;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..n - 1 {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1, 0.2, 0.3, 0.4];
        let decoder = BpDecoder::new(
            &mut pcm,
            channel_probabilities,
            10, // maximum_iterations
            BpMethod::MinimumSum,
            BpSchedule::Serial,
            0.5,                     // min_sum_scaling_factor
            4,                       // omp_threads
            Some(&vec![1, 3, 0, 2]), // serial_schedule
            -1,                      // random_schedule_seed
            true,                    // random_schedule_at_every_iteration
            BpInputType::Auto,
        )
        .unwrap();
        assert_eq!(decoder.pcm.base.m, decoder.check_count);
        assert_eq!(decoder.pcm.base.n, decoder.bit_count);
        assert_eq!(decoder.channel_probabilities, vec![0.1, 0.2, 0.3, 0.4]);
        assert_eq!(decoder.maximum_iterations, 10);
        assert_eq!(0.5, decoder.ms_scaling_factor);
        assert!(matches!(decoder.method, BpMethod::MinimumSum));
        assert!(matches!(decoder.schedule, BpSchedule::Serial));
        assert_eq!(decoder.omp_thread_count, 4);
        assert_eq!(decoder.serial_schedule_order, vec![1, 3, 0, 2]);
        assert_eq!(decoder.random_schedule_seed, -1);
        assert_eq!(decoder.random_schedule_at_every_iteration, true);
    }

    #[test]
    fn initialise_log_domain_bp_test() {
        let n = 3;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..n - 1 {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1, 0.2, 0.3];
        let mut decoder = BpDecoder::new_3(&mut pcm, channel_probabilities, 100).unwrap();
        decoder.initialise_log_domain_bp();
        for (i, prob) in decoder.channel_probabilities.iter().enumerate() {
            let expected_log_prob = ((1.0 - prob) / prob).ln();
            assert_eq!(decoder.initial_log_prob_ratios[i], expected_log_prob);

            for e in decoder.pcm.base.iterate_column(i) {
                assert_eq!(
                    unsafe { (*e).inner.bit_to_check_msg },
                    decoder.initial_log_prob_ratios[i]
                );
            }
        }
    }

    #[test]
    fn product_sum_parallel() {
        let n = 3;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..(n - 1) {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1; n];
        let mut decoder = BpDecoder::new(
            &mut pcm,
            channel_probabilities,
            n, // Using n as the number of maximum iterations
            BpMethod::ProductSum,
            BpSchedule::Parallel,
            79879879.0,
            1,
            None,
            -1,
            true,
            BpInputType::Auto,
        )
        .unwrap();

        assert_eq!(decoder.pcm.base.m, decoder.check_count);
        assert_eq!(decoder.pcm.base.n, decoder.bit_count);
        assert_eq!(decoder.channel_probabilities, vec![0.1; n]);
        assert_eq!(decoder.maximum_iterations, n);
        assert_eq!(79879879.0, decoder.ms_scaling_factor);
        assert!(matches!(decoder.method, BpMethod::ProductSum));
        assert!(matches!(decoder.schedule, BpSchedule::Parallel));
        assert_eq!(decoder.omp_thread_count, 1);

        let input_vectors = [vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]];
        let expected_resutls = [vec![0, 0, 0], vec![0, 0, 1], vec![1, 0, 0], vec![0, 1, 0]];
        for (index, input_vector) in input_vectors.iter().enumerate() {
            let decoded = decoder.decode(&input_vector);
            assert_eq!(decoded, expected_resutls[index]);
        }
    }

    #[test]
    fn prod_sum_parallel_repetition_code_5() {
        let n = 5;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..(n - 1) {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1; n];
        let mut decoder = BpDecoder::new(
            &mut pcm,
            channel_probabilities,
            n, // Using n as the number of maximum iterations
            BpMethod::ProductSum,
            BpSchedule::Parallel,
            4324234.0,
            1,
            None,
            -1,
            true,
            BpInputType::Auto,
        )
        .unwrap();

        let input_vectors = [
            vec![0, 0, 0, 0],
            vec![0, 0, 0, 1],
            vec![0, 1, 0, 1],
            vec![1, 0, 1, 0],
            vec![1, 1, 1, 1],
        ];
        let expected_resutls = [
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 1],
            vec![0, 0, 1, 1, 0],
            vec![0, 1, 1, 0, 0],
            vec![0, 1, 0, 1, 0],
        ];
        for (index, input_vector) in input_vectors.iter().enumerate() {
            let decoded = decoder.decode(&input_vector);
            assert_eq!(decoded, expected_resutls[index]);
        }

        // let decoding = decoder.decode(&input_vector);
        // assert_eq!(decoding, vec![0, 1, 0, 1, 0]);
    }

    #[test]
    fn min_sum_repetition_code_5() {
        let n = 5;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..(n - 1) {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1; n];
        let mut decoder = BpDecoder::new(
            &mut pcm,
            channel_probabilities,
            n,
            BpMethod::MinimumSum,
            BpSchedule::Parallel,
            1.0,
            1,
            None,
            -1,
            true,
            BpInputType::Auto,
        )
        .unwrap();

        let input_vectors: Vec<Vec<u8>> = vec![
            vec![0, 0, 0, 0],
            vec![0, 0, 0, 1],
            vec![0, 1, 0, 1],
            vec![1, 0, 1, 0],
            vec![1, 1, 1, 1],
        ];

        let expected_resutls = [
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 1],
            vec![0, 0, 1, 1, 0],
            vec![0, 1, 1, 0, 0],
            vec![0, 1, 0, 1, 0],
        ];
        // let decoding = decoder.decode(&input_vector);
        for (index, input_vector) in input_vectors.iter().enumerate() {
            let decoded = decoder.decode(input_vector);
            // assert_eq!(decoded, vec![1, 0, 1, 0, 1]); // bad
            assert_eq!(decoded, expected_resutls[index]);
            // println!("decoding: {:?}", decoded);
        }
        // assert_eq!(decoding, vec![1, 0, 1, 0, 1]);
    }

    #[test]
    fn prod_sum_serial_repetition_code_5() {
        let n = 5;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..(n - 1) {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1; n];
        let mut decoder = BpDecoder::new(
            &mut pcm,
            channel_probabilities,
            n, // Using n as the number of maximum iterations
            BpMethod::ProductSum,
            BpSchedule::Serial,
            4324234.0,
            1,
            None,
            0,
            false,
            BpInputType::Auto,
        )
        .unwrap();

        let input_vectors: Vec<Vec<u8>> = vec![
            vec![0, 0, 0, 0],
            vec![0, 0, 0, 1],
            vec![0, 1, 0, 1],
            vec![1, 0, 1, 0],
            vec![1, 1, 1, 1],
        ];
        let expected_resutls = [
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 1],
            vec![0, 0, 1, 1, 0],
            vec![0, 1, 1, 0, 0],
            vec![0, 1, 0, 1, 0],
        ];
        for (index, input_vector) in input_vectors.iter().enumerate() {
            let decoded = decoder.decode(input_vector);
            assert_eq!(decoded, expected_resutls[index]);
        }
    }

    #[test]
    fn min_sum_serial_repetition_code_5() {
        let n = 5;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..(n - 1) {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1; n];
        let mut decoder = BpDecoder::new(
            &mut pcm,
            channel_probabilities,
            n,
            BpMethod::MinimumSum,
            BpSchedule::Serial,
            1.0,
            1,
            None,
            0,
            false,
            BpInputType::Auto,
        )
        .unwrap();

        let input_vectors: Vec<Vec<u8>> = vec![
            vec![0, 0, 0, 0],
            vec![0, 0, 0, 1],
            vec![0, 1, 0, 1],
            vec![1, 0, 1, 0],
            vec![1, 1, 1, 1],
        ];
        let expected_resutls = [
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 1],
            vec![0, 0, 1, 1, 0],
            vec![0, 1, 1, 0, 0],
            vec![0, 1, 0, 1, 0],
        ];
        for (index, input_vector) in input_vectors.iter().enumerate() {
            let decoded = decoder.decode(input_vector);
            assert_eq!(decoded, expected_resutls[index]);
        }
    }

    #[test]
    fn min_sum_parallel() {
        let n = 3;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..(n - 1) {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1; n];
        let mut decoder = BpDecoder::new(
            &mut pcm,
            channel_probabilities,
            n,
            BpMethod::MinimumSum,
            BpSchedule::Parallel,
            0.625,
            1,
            None,
            -1,
            true,
            BpInputType::Auto,
        )
        .unwrap();

        let input_vectors = [vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]];
        let expected_results = [vec![0, 0, 0], vec![0, 0, 1], vec![1, 0, 0], vec![0, 1, 0]];

        for (index, input_vector) in input_vectors.iter().enumerate() {
            let decoded = decoder.decode(input_vector);
            assert_eq!(decoded, expected_results[index]);
        }
    }

    #[test]
    fn min_sum_single_scan() {
        let n = 3;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..(n - 1) {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1; n];
        let mut decoder = BpDecoder::new(
            &mut pcm,
            channel_probabilities,
            n,
            BpMethod::MinimumSum,
            BpSchedule::Parallel,
            0.625,
            1,
            None,
            -1,
            true,
            BpInputType::Auto,
        )
        .unwrap();

        decoder.initialise_log_domain_bp();
        let input_vectors = [vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]];
        let expected_results = [vec![0, 0, 0], vec![0, 0, 1], vec![1, 0, 0], vec![0, 1, 0]];

        for (index, input_vector) in input_vectors.iter().enumerate() {
            let decoded = decoder.bp_decode_single_scan(input_vector);
            assert_eq!(decoded, expected_results[index]);
        }
    }

    #[test]
    fn min_sum_relative_schedule() {
        let n = 3;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..(n - 1) {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1; n];
        let mut decoder = BpDecoder::new(
            &mut pcm,
            channel_probabilities,
            100,
            BpMethod::MinimumSum,
            BpSchedule::SerialRelative,
            0.625,
            1,
            None,
            -1,
            true,
            BpInputType::Auto,
        )
        .unwrap();

        let input_vectors = [vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]];
        let expected_results = [vec![0, 0, 0], vec![0, 0, 1], vec![1, 0, 0], vec![0, 1, 0]];
        for (index, input_vector) in input_vectors.iter().enumerate() {
            let decoded = decoder.bp_decode_serial(input_vector);
            assert_eq!(decoded, expected_results[index]);
        }
    }

    #[test]
    fn random_schedule_seed() {
        {
            let n = 4;
            let mut pcm = BpSparse::new(n - 1, n, 0);
            for i in 0..(n - 1) {
                pcm.insert_entry(i, i);
                pcm.insert_entry(i, (i + 1) % n);
            }
            let channel_probabilities = vec![0.1, 0.2, 0.3, 0.4];
            let serial_schedule = vec![2, 3, 1, 0];

            let decoder = BpDecoder::new(
                &mut pcm,
                channel_probabilities,
                100,
                BpMethod::MinimumSum,
                BpSchedule::Serial,
                0.5,
                1,
                Some(&serial_schedule),
                -1,
                true,
                BpInputType::Auto,
            )
            .unwrap();

            // Test if decoder serial schedule is initialized correctly with the seed
            assert_eq!(decoder.serial_schedule_order, vec![2, 3, 1, 0]);
            assert_eq!(decoder.random_schedule_seed, -1);
        }
        {
            let n = 4;
            let mut pcm = BpSparse::new(n - 1, n, 0);
            for i in 0..(n - 1) {
                pcm.insert_entry(i, i);
                pcm.insert_entry(i, (i + 1) % n);
            }
            let channel_probabilities = vec![0.1, 0.2, 0.3, 0.4];
            let expected_serial_schedule = vec![0, 1, 2, 3];

            let decoder = BpDecoder::new(
                &mut pcm,
                channel_probabilities,
                100,
                BpMethod::MinimumSum,
                BpSchedule::Serial,
                0.625,
                1,
                None,
                0,
                true,
                BpInputType::Auto,
            )
            .unwrap();

            // Test if decoder serial schedule is initialized correctly with the seed
            assert_eq!(decoder.serial_schedule_order, expected_serial_schedule);
            assert_eq!(decoder.random_schedule_seed, 0);
        }
        {
            let n = 4;
            let mut pcm = BpSparse::new(n - 1, n, 0);
            for i in 0..(n - 1) {
                pcm.insert_entry(i, i);
                pcm.insert_entry(i, (i + 1) % n);
            }
            let channel_probabilities = vec![0.1, 0.2, 0.3, 0.4];
            // let expected_serial_schedule = vec![2, 3, 1, 0];

            let decoder = BpDecoder::new(
                &mut pcm,
                channel_probabilities,
                100,
                BpMethod::MinimumSum,
                BpSchedule::Serial,
                0.625,
                1,
                None,
                4,
                true,
                BpInputType::Auto,
            )
            .unwrap();

            assert_eq!(decoder.random_schedule_seed, 4);
        }
    }

    #[test]
    fn relative_serial_schedule_order() {
        {
            let n = 3;
            let mut pcm = BpSparse::new(n - 1, n, 0);
            for i in 0..(n - 1) {
                pcm.insert_entry(i, i);
                pcm.insert_entry(i, (i + 1) % n);
            }
            let channel_probabilities = vec![0.2, 0.3, 0.1]; // Set such that bit index 2 has the highest likelihood

            let mut decoder = BpDecoder::new(
                &mut pcm,
                channel_probabilities,
                1,
                BpMethod::MinimumSum,
                BpSchedule::SerialRelative,
                0.625,
                1,
                None,
                -1,
                true,
                BpInputType::Auto,
            )
            .unwrap();

            // decoder.initialise_log_domain_bp();
            decoder.decode(&vec![0, 0]); // Running decode to potentially trigger schedule update

            // Ensure that the bits are ordered correctly after scheduling (mock or actual scheduling logic)
            assert_eq!(decoder.serial_schedule_order, vec![2, 0, 1]); // Expect bit index 2 to be first given its probabilities
            assert_eq!(decoder.random_schedule_seed, -1);
        }

        {
            let n = 3;
            let mut pcm = BpSparse::new(n - 1, n, 0);
            for i in 0..(n - 1) {
                pcm.insert_entry(i, i);
                pcm.insert_entry(i, (i + 1) % n);
            }
            let channel_probabilities = vec![0.3, 0.2, 0.1]; // Set such that bit index 2 has the highest likelihood

            let mut decoder = BpDecoder::new(
                &mut pcm,
                channel_probabilities,
                1,
                BpMethod::MinimumSum,
                BpSchedule::SerialRelative,
                0.625,
                1,
                Some(&vec![0, 1, 2]),
                -1,
                true,
                BpInputType::Auto,
            )
            .unwrap();

            // decoder.initialise_log_domain_bp();
            decoder.decode(&vec![0, 0]); // Running decode to potentially trigger schedule update

            // Ensure that the bits are ordered correctly after scheduling (mock or actual scheduling logic)
            assert_eq!(decoder.serial_schedule_order, vec![2, 1, 0]); // Expect bit index 2 to be first given its probabilities
            assert_eq!(decoder.random_schedule_seed, -1);
        }

        {
            let n = 3;
            let mut pcm = BpSparse::new(n - 1, n, 0);
            for i in 0..(n - 1) {
                pcm.insert_entry(i, i);
                pcm.insert_entry(i, (i + 1) % n);
            }
            let channel_probabilities = vec![0.1, 0.01, 0.01]; // Set such that bit index 2 has the highest likelihood

            let mut decoder = BpDecoder::new(
                &mut pcm,
                channel_probabilities,
                2,
                BpMethod::MinimumSum,
                BpSchedule::SerialRelative,
                0.625,
                1,
                Some(&vec![0, 1, 2]),
                -1,
                true,
                BpInputType::Auto,
            )
            .unwrap();

            // decoder.initialise_log_domain_bp();
            decoder.decode(&vec![1, 0]); // Running decode to potentially trigger schedule update

            // Ensure that the bits are ordered correctly after scheduling (mock or actual scheduling logic)
            assert_eq!(decoder.serial_schedule_order, vec![1, 2, 0]); // Expect bit index 2 to be first given its probabilities
            assert_eq!(decoder.random_schedule_seed, -1);
        }
    }

    #[test]
    fn prod_sum_serial_rep_code_5() {
        let n = 5;
        let mut pcm = BpSparse::new(n - 1, n, 0);
        for i in 0..(n - 1) {
            pcm.insert_entry(i, i);
            pcm.insert_entry(i, (i + 1) % n);
        }
        let channel_probabilities = vec![0.1; n];

        let mut decoder = BpDecoder::new(
            &mut pcm,
            channel_probabilities,
            n,
            BpMethod::ProductSum,
            BpSchedule::Serial,
            4324234.0,
            1,
            None,
            -1,
            true,
            BpInputType::Auto,
        )
        .unwrap();

        let input_vectors = [
            vec![0, 0, 0, 0, 1],
            vec![0, 1, 1, 0, 0],
            vec![1, 0, 0, 1, 1],
        ]; // Received vector
        let expcted_results = [
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0],
            vec![1, 1, 1, 1, 1],
        ];

        for (index, input_vector) in input_vectors.iter().enumerate() {
            let decoded = decoder.decode(input_vector);
            assert_eq!(decoded, expcted_results[index]);
        }
    }
}
