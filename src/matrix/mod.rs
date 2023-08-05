pub mod basic;
pub mod complete;
pub mod echelon;
pub mod hair;
pub mod interface;
pub mod row;
pub mod tail;
pub mod tight;
pub mod visualize;

pub use basic::BasicMatrix;
pub use complete::CompleteMatrix;
pub use echelon::Echelon;
pub use hair::HairView;
pub use interface::*;
pub use tail::Tail;
pub use tight::Tight;
pub use visualize::{VizTable, VizTrait};
