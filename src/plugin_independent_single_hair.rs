//! independent single hair looks at every non-zero dual variable and 
//! 
//! Generics for plugins, defining the necessary interfaces for a plugin
//! 
//! A plugin must implement Clone trait, because it will be cloned multiple times for each cluster
//!

use crate::framework::*;
use crate::parity_matrix::*;