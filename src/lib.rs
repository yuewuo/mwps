#![cfg_attr(feature = "python_binding", feature(cfg_eval))]

extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate cfg_if;
extern crate chrono;
extern crate clap;
extern crate derivative;
extern crate itertools;
#[macro_use]
extern crate lazy_static;
extern crate more_asserts;
extern crate num_rational;
extern crate num_traits;
extern crate parking_lot;
#[cfg(feature = "cli")]
extern crate pbr;
extern crate prettytable;
#[cfg(feature = "python_binding")]
extern crate pyo3;
extern crate rand;
extern crate rand_xoshiro;
#[cfg(feature = "slp")]
extern crate slp;
extern crate urlencoding;
#[cfg(feature = "wasm_binding")]
extern crate wasm_bindgen;

#[cfg(feature = "cli")]
pub mod cli;
pub mod decoding_hypergraph;
pub mod dual_module;
pub mod dual_module_pq;
pub mod dual_module_serial;
pub mod example_codes;
pub mod invalid_subgraph;
pub mod matrix;
pub mod model_hypergraph;
pub mod mwpf_solver;
pub mod plugin;
pub mod plugin_single_hair;
pub mod plugin_union_find;
pub mod pointers;
pub mod primal_module;
pub mod primal_module_serial;
pub mod primal_module_union_find;
pub mod relaxer;
pub mod relaxer_forest;
pub mod relaxer_optimizer;
pub mod union_find;
pub mod util;
pub mod visualize;

#[cfg(feature = "python_binding")]
use pyo3::prelude::*;

#[cfg(feature = "python_binding")]
#[pymodule]
fn mwpf(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    util::register(py, m)?;
    visualize::register(py, m)?;
    example_codes::register(py, m)?;
    mwpf_solver::register(py, m)?;
    Ok(())
}

#[cfg(feature = "wasm_binding")]
use wasm_bindgen::prelude::*;

#[cfg_attr(feature = "wasm_binding", wasm_bindgen)]
pub fn get_version() -> String {
    use decoding_hypergraph::*;
    use dual_module::*;
    use dual_module_serial::*;
    use example_codes::*;
    use primal_module::*;
    use primal_module_serial::*;
    // TODO: I'm just testing basic functionality
    let defect_vertices = vec![23, 24, 29, 30];
    let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
    // create dual module
    let model_graph = code.get_model_graph();
    let mut dual_module = DualModuleSerial::new_empty(&model_graph.initializer);
    // create primal module
    let mut primal_module = PrimalModuleSerial::new_empty(&model_graph.initializer);
    primal_module.growing_strategy = GrowingStrategy::SingleCluster;
    primal_module.plugins = std::sync::Arc::new(vec![]);
    // try to work on a simple syndrome
    let decoding_graph = DecodingHyperGraph::new_defects(model_graph, defect_vertices.clone());
    let interface_ptr = DualModuleInterfacePtr::new(decoding_graph.model_graph.clone());
    primal_module.solve_visualizer(
        &interface_ptr,
        decoding_graph.syndrome_pattern.clone(),
        &mut dual_module,
        None,
    );
    let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module, 0);
    println!("subgraph: {subgraph:?}");
    // env!("CARGO_PKG_VERSION").to_string()
    format!("subgraph: {subgraph:?}, weight_range: {weight_range:?}")
}

#[cfg(not(feature = "f32_weight"))]
type BaseFloat = f64;
#[cfg(feature = "f32_weight")]
type BaseFloat = f32; // there's actually no point in using this, as HIGHs don't support f32

pub mod ordered_float {
    use crate::BaseFloat;
    use num_traits::Zero;

    const EPSILON: BaseFloat = 1e-4; // note: it would be interesting to play around with this.

    #[derive(Debug, Clone, Copy)]
    pub struct OrderedFloat(BaseFloat);

    impl OrderedFloat {
        pub fn new(value: BaseFloat) -> Self {
            Self(value)
        }
        pub fn numer(&self) -> BaseFloat {
            self.0
        }
        pub fn denom(&self) -> BaseFloat {
            1.0
        }
        pub fn set_zero(&mut self) {
            self.0 = 0.0;
        }

        pub fn recip(&self) -> Self {
            Self::new(1.0 / self.0)
        }
        pub fn new_raw(numer: i32, denom: i32) -> Self {
            Self::new(numer as BaseFloat / denom as BaseFloat)
        }
    }

    // Implement num_traits
    impl num_traits::Zero for OrderedFloat {
        fn zero() -> Self {
            Self::new(0.0)
        }
        fn is_zero(&self) -> bool {
            self.0.abs() < EPSILON
        }
    }
    impl num_traits::One for OrderedFloat {
        fn one() -> Self {
            Self::new(1.0)
        }
        fn is_one(&self) -> bool {
            (self.0 - 1.0).abs() < EPSILON
        }
    }
    impl num_traits::Signed for OrderedFloat {
        fn is_negative(&self) -> bool {
            !self.is_zero() && self.0 < 0.0
        }
        fn is_positive(&self) -> bool {
            !self.is_zero() && self.0 > 0.0
        }
        fn abs(&self) -> Self {
            Self::new(self.0.abs())
        }
        fn abs_sub(&self, other: &Self) -> Self {
            (self - other).max(OrderedFloat::zero())
        }
        fn signum(&self) -> Self {
            Self::new(self.0.signum())
        }
    }
    impl num_traits::Num for OrderedFloat {
        type FromStrRadixErr = num_traits::ParseFloatError;
        fn from_str_radix(str: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
            match BaseFloat::from_str_radix(str, radix) {
                Ok(value) => Ok(Self::new(value)),
                Err(err) => Err(err),
            }
        }
    }
    impl num_traits::FromPrimitive for OrderedFloat {
        fn from_i64(n: i64) -> Option<Self> {
            Some(Self::new(n as BaseFloat))
        }
        fn from_u64(n: u64) -> Option<Self> {
            Some(Self::new(n as BaseFloat))
        }
        fn from_f64(n: f64) -> Option<Self> {
            Some(Self::new(n))
        }
        fn from_usize(n: usize) -> Option<Self> {
            Some(Self::new(n as BaseFloat))
        }
    }
    impl num_traits::ToPrimitive for OrderedFloat {
        fn to_i64(&self) -> Option<i64> {
            Some(self.0 as i64)
        }
        fn to_u64(&self) -> Option<u64> {
            Some(self.0 as u64)
        }
        #[allow(clippy::unnecessary_cast)]
        fn to_f64(&self) -> Option<f64> {
            Some(self.0 as f64)
        }
    }

    // Implement std ops
    impl std::ops::Rem for OrderedFloat {
        type Output = Self;
        fn rem(self, other: Self) -> Self {
            Self::new(self.0 % other.0)
        }
    }
    impl std::ops::Neg for OrderedFloat {
        type Output = Self;
        fn neg(self) -> Self {
            Self::new(-self.0)
        }
    }
    impl std::ops::Neg for &OrderedFloat {
        type Output = OrderedFloat;
        fn neg(self) -> OrderedFloat {
            OrderedFloat::new(-self.0)
        }
    }

    // Implement add, sub, mul, div operations, with assign operations, references, by macros
    macro_rules! impl_ops {
        ($trait:ident, $method:ident) => {
            impl std::ops::$trait for OrderedFloat {
                type Output = Self;
                fn $method(self, other: Self) -> Self {
                    Self::new(self.0.$method(other.0))
                }
            }
            impl std::ops::$trait<&OrderedFloat> for OrderedFloat {
                type Output = Self;
                fn $method(self, other: &Self) -> Self {
                    Self::new(self.0.$method(other.0))
                }
            }
            impl std::ops::$trait<OrderedFloat> for &OrderedFloat {
                type Output = OrderedFloat;
                fn $method(self, other: OrderedFloat) -> OrderedFloat {
                    OrderedFloat::new(self.0.$method(other.0))
                }
            }
            impl std::ops::$trait<&OrderedFloat> for &OrderedFloat {
                type Output = OrderedFloat;
                fn $method(self, other: &OrderedFloat) -> OrderedFloat {
                    OrderedFloat::new(self.0.$method(other.0))
                }
            }
        };
    }
    impl_ops!(Add, add);
    impl_ops!(Sub, sub);
    impl_ops!(Mul, mul);
    impl_ops!(Div, div);

    // Implement assign operations
    macro_rules! impl_assign_ops {
        ($trait:ident, $method:ident, $op:tt) => {
            #[allow(clippy::assign_op_pattern)]
            impl std::ops::$trait for OrderedFloat {
                fn $method(&mut self, other: Self) {
                    *self = *self $op other;
                }
            }
            impl std::ops::$trait<&OrderedFloat> for OrderedFloat {
                fn $method(&mut self, other: &Self) {
                    *self = *self $op other;
                }
            }
            // impl std::ops::$trait<&f32> for OrderedFloat {
            //     fn $method(&mut self, other: &f32) {
            //         self.0 = self.0 $op *other as BaseFloat;
            //     }
            // }
            // impl std::ops::$trait<&f64> for OrderedFloat {
            //     fn $method(&mut self, other: &f64) {
            //         self.0 = self.0 $op *other as BaseFloat;
            //     }
            // }
        };
    }
    impl_assign_ops!(AddAssign, add_assign, +);
    impl_assign_ops!(SubAssign, sub_assign, -);
    impl_assign_ops!(MulAssign, mul_assign, *);
    impl_assign_ops!(DivAssign, div_assign, /);

    // Implement other std traits
    impl std::str::FromStr for OrderedFloat {
        type Err = std::num::ParseFloatError;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            Ok(Self::new(f64::from_str(s)?))
        }
    }
    impl std::hash::Hash for OrderedFloat {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.to_bits().hash(state);
        }
    }
    impl std::fmt::Display for OrderedFloat {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    // Implement Eq
    impl Eq for OrderedFloat {}

    // Implement PartialEq
    impl PartialEq for OrderedFloat {
        fn eq(&self, other: &Self) -> bool {
            (self.0 - other.0).abs() < EPSILON
        }
    }
    impl PartialEq<f64> for OrderedFloat {
        fn eq(&self, other: &f64) -> bool {
            (self.0 - other).abs() < EPSILON
        }
    }
    impl PartialEq<OrderedFloat> for f64 {
        fn eq(&self, other: &OrderedFloat) -> bool {
            (*self - other.0).abs() < EPSILON
        }
    }

    // Implement PartialOrd
    impl PartialOrd for OrderedFloat {
        #[allow(clippy::non_canonical_partial_ord_impl)]
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            if (self.0 - other.0).abs() < EPSILON {
                Some(std::cmp::Ordering::Equal)
            } else {
                self.0.partial_cmp(&other.0)
            }
        }
    }

    // Implement Ord
    impl Ord for OrderedFloat {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.partial_cmp(other).unwrap()
        }
    }

    // Implement From<f64> for OrderedFloat
    impl From<BaseFloat> for OrderedFloat {
        fn from(value: BaseFloat) -> Self {
            OrderedFloat::new(value)
        }
    }

    // Implement Default
    impl Default for OrderedFloat {
        fn default() -> Self {
            Self::new(0.0)
        }
    }
}
