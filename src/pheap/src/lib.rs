//! # Pairing Heap
//! A priority queue implemented with a pairing heap.
//!
//! From [Wikipedia](https://en.wikipedia.org/wiki/Pairing_heap):
//! > A pairing heap is a type of heap data structure with relatively simple implementation and excellent practical amortized performance.
//! > Pairing heaps are heap-ordered multiway tree structures, and can be considered simplified Fibonacci heaps. They are considered a "robust choice" for implementing such algorithms as Prim's MST algorithm.
//!
//! A min-pairing heap supports the following operations:
//! - ```find_min```: finds the minimum element of the heap, which is the root.
//! - ```merge```: combines two heaps together.
//! - ```insert```: adds a new element into the heap.
//! - ```delete_min```: remove the root and reorder its children nodes.
//! - ```decrease_key```: decrease the priority of an element. Standard implementation of a heap data structure does not support searching for a key efficiently (which is the case in this crate). Thus, this operation can take very long time, with an upper bound of ```O(2^(sqrt(log log n)))```.
//!
//! The heap data structure is often used in Dijkstra's algorithm and Prim's algorithm. With [`PairingHeap`],
//! the crate provides a fast implementation of these algorithms . See [`graph`] for more info.
//!
#![warn(
    missing_docs,
    rust_2018_idioms,
    missing_debug_implementations,
    broken_intra_doc_links
)]

mod ph;
pub use ph::PairingHeap;

/// Experimental API for graph analysis.
pub mod graph;

mod tests;
