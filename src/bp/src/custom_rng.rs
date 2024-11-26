//! custom_rng.rs
//!
//! This module contains a custom random number generator that can be used to generate random double values between 0 and 1.

use rand::distributions::{Distribution, Uniform};
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::seq::SliceRandom;

/// Generates random double values between 0 and 1 using StdRng (originally Mersenne Twister) random number generator
pub struct CustomRNG {
    gen: StdRng,
}

impl CustomRNG {
    /// Constructs a new RandomNumberGenerator object with an optional seed
    /// If no seed is specified, the generator is seeded using the system clock.
    pub fn new(seed: Option<u64>) -> Self {
        let gen = match seed {
            Some(s) => StdRng::seed_from_u64(s),
            None => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_secs();
                StdRng::seed_from_u64(now)
            }
        };
        CustomRNG { gen }
    }

    /// Generates a new random double value between 0 and 1
    pub fn random_double(&mut self) -> f64 {
        let dis = Uniform::from(0.0..1.0);
        dis.sample(&mut self.gen)
    }

    /// Generates a new random integer value between 0 and max_int
    pub fn random_int(&mut self, max_int: usize) -> usize {
        let dis = Uniform::from(0..=max_int);
        dis.sample(&mut self.gen)
    }
}

/// A templated class for shuffling lists of data.
#[derive(Clone, Debug)]
pub struct RandomListShuffle<T> {
    generator: StdRng,
    _marker: std::marker::PhantomData<T>,
}

impl Default for RandomListShuffle<usize> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> RandomListShuffle<T> {
    /// Default constructor
    pub fn new() -> Self {
        RandomListShuffle {
            generator: StdRng::from_entropy(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Constructor that allows specifying a seed
    pub fn with_seed(seed: Option<u64>) -> Self {
        let generator = match seed {
            Some(s) => StdRng::seed_from_u64(s),
            None => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_secs();
                StdRng::seed_from_u64(now)
            }
        };
        RandomListShuffle {
            generator,
            _marker: std::marker::PhantomData,
        }
    }

    /// Set the seed for the random number generator
    pub fn seed(&mut self, seed: u64) {
        self.generator = StdRng::seed_from_u64(seed);
    }

    /// Shuffle a vector of data
    pub fn shuffle(&mut self, data: &mut Vec<T>) {
        let rng = &mut self.generator;
        let slice = data.as_mut_slice();
        slice.shuffle(rng);
    }
}
