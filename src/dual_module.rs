//! Dual Module
//! 
//! Generics for dual modules
//!

use crate::util::*;
use crate::pointers::*;
use std::collections::BTreeSet;
use crate::derivative::Derivative;
use crate::num_traits::{Zero, One, ToPrimitive};
use crate::visualize::*;


pub struct DualNode {
    /// the index of this dual node, helps to locate internal details of this dual node
    pub index: NodeIndex,
    /// specified by users, these internal edges must not satisfy the parity requirement to be a valid dual variable
    pub internal_edges: BTreeSet<EdgeIndex>,
    /// all the vertices that are incident to the internal edges; if empty it will calculate from `internal_edges`
    pub internal_vertices: BTreeSet<VertexIndex>,
    /// calculated automatically: the hair edges (some vertices inside some others outside)
    pub hair_edges: BTreeSet<EdgeIndex>,
    /// current dual variable's value
    pub dual_variable: Rational,
    /// the strategy to grow the dual variables
    pub grow_rate: Rational,
}

pub type DualNodePtr = ArcRwLock<DualNode>;
pub type DualNodeWeak = WeakRwLock<DualNode>;

impl std::fmt::Debug for DualNodePtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let dual_node = self.read_recursive();  // reading index is consistent
        write!(f, "{}", dual_node.index)
    }
}

impl std::fmt::Debug for DualNodeWeak {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.upgrade_force().fmt(f)
    }
}

impl Ord for DualNodePtr {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.read_recursive().index.cmp(&other.read_recursive().index)
    }
}

impl PartialOrd for DualNodePtr {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// an array of dual nodes
/// dual nodes, once created, will never be deconstructed until the next run
#[derive(Derivative)]
#[derivative(Debug)]
pub struct DualModuleInterface {
    /// all the dual node that can be used to control a concrete dual module implementation
    pub nodes: Vec<DualNodePtr>,
}

pub type DualModuleInterfacePtr = ArcRwLock<DualModuleInterface>;
pub type DualModuleInterfaceWeak = WeakRwLock<DualModuleInterface>;

impl std::fmt::Debug for DualModuleInterfacePtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let interface = self.read_recursive();
        write!(f, "{}", interface.nodes.len())
    }
}

impl std::fmt::Debug for DualModuleInterfaceWeak {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.upgrade_force().fmt(f)
    }
}

/// gives the maximum absolute length to grow, if not possible, give the reason;
/// note that strong reference is stored in `MaxUpdateLength` so dropping these temporary messages are necessary to avoid memory leakage
#[derive(Derivative, PartialEq, Eq, Clone)]
#[derivative(Debug)]
pub enum MaxUpdateLength {
    /// unbounded
    Unbounded,
    /// non-zero maximum update length
    ValidGrow(Rational),
    /// conflicting growth, violating the slackness constraint
    Conflicting(EdgeIndex),
    /// hitting 0 dual variable while shrinking, only happens when `grow_rate` < 0
    ShrinkProhibited(DualNodePtr),
}

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub enum GroupMaxUpdateLength {
    /// unbounded
    Unbounded,
    /// non-zero maximum update length
    ValidGrow(Rational),
    /// conflicting reasons and pending VertexShrinkStop events (empty in a single serial dual module)
    Conflicts(Vec<MaxUpdateLength>),
}

/// common trait that must be implemented for each implementation of dual module
pub trait DualModuleImpl {

    /// create a new dual module with empty syndrome
    fn new_empty(initializer: &SolverInitializer) -> Self;

    /// clear all growth and existing dual nodes, prepared for the next decoding
    fn clear(&mut self);

    /// add corresponding dual node, note that the `internal_vertices` and `hair_edges` are not set
    fn add_dual_node(&mut self, dual_node_ptr: &DualNodePtr);

    /// update grow rate
    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational);

    /// An optional function that helps to break down the implementation of [`DualModuleImpl::compute_maximum_update_length`]
    /// check the maximum length to grow (shrink) specific dual node, if length is 0, give the reason of why it cannot further grow (shrink).
    /// if `simultaneous_update` is true, also check for the peer node according to [`DualNode::grow_state`].
    fn compute_maximum_update_length_dual_node(&mut self, _dual_node_ptr: &DualNodePtr, _simultaneous_update: bool) -> MaxUpdateLength {
        panic!("the dual module implementation doesn't support this function, please use another dual module")
    }

    /// check the maximum length to grow (shrink) for all nodes, return a list of conflicting reason and a single number indicating the maximum rate to grow:
    /// this number will be 0 if any conflicting reason presents
    fn compute_maximum_update_length(&mut self) -> GroupMaxUpdateLength;

    /// An optional function that can manipulate individual dual node, not necessarily supported by all implementations
    fn grow_dual_node(&mut self, _dual_node_ptr: &DualNodePtr, _length: Rational) {
        panic!("the dual module implementation doesn't support this function, please use another dual module")
    }

    /// grow a specific length globally, length must be positive.
    /// note that reversing the process is possible, but not recommended: to do that, reverse the state of each dual node, Grow->Shrink, Shrink->Grow
    fn grow(&mut self, length: Rational);

    fn find_valid_subgraph(&self, internal_edges: &BTreeSet<EdgeIndex>, internal_vertices: &BTreeSet<VertexIndex>) -> Option<Subgraph>;

    fn find_valid_subgraph_auto_vertices(&self, internal_edges: &BTreeSet<EdgeIndex>) -> Option<Subgraph> {
        self.find_valid_subgraph(internal_edges, &self.get_edges_neighbors(internal_edges))
    }

    fn is_valid_cluster(&self, internal_edges: &BTreeSet<EdgeIndex>, internal_vertices: &BTreeSet<VertexIndex>) -> bool {
        self.find_valid_subgraph(internal_edges, internal_vertices).is_some()
    }

    fn is_valid_cluster_auto_vertices(&self, internal_edges: &BTreeSet<EdgeIndex>) -> bool {
        self.find_valid_subgraph_auto_vertices(internal_edges).is_some()
    }

    fn get_edge_nodes(&self, edge_index: EdgeIndex) -> Vec<DualNodePtr>;

    fn is_edge_tight(&self, edge_index: EdgeIndex) -> bool;

    fn get_edge_neighbors(&self, edge_index: EdgeIndex) -> Vec<VertexIndex>;

    fn is_vertex_defect(&self, vertex_index: VertexIndex) -> bool;

    /// return if the vertex is defect and all the edges that connects to it
    fn get_vertex_neighbors(&self, vertex_index: VertexIndex) -> Vec<EdgeIndex>;

    fn get_edges_neighbors(&self, edges: &BTreeSet<EdgeIndex>) -> BTreeSet<VertexIndex> {
        let mut vertices = BTreeSet::new();
        for &edge_index in edges.iter() {
            vertices.extend(self.get_edge_neighbors(edge_index));
        }
        vertices
    }

}

impl MaxUpdateLength {

    pub fn new() -> Self {
        Self::Unbounded
    }

    pub fn merge(&mut self, max_update_length: MaxUpdateLength) {
        match self {
            Self::Unbounded => {
                *self = max_update_length;
            },
            Self::ValidGrow(current_length) => {
                match max_update_length {
                    MaxUpdateLength::Unbounded => { },  // do nothing
                    MaxUpdateLength::ValidGrow(length) => {
                        *self = Self::ValidGrow(std::cmp::min(current_length.clone(), length))
                    }
                    _ => { *self = max_update_length },
                }
            },
            _ => { }  // do nothing if it's already a conflict
        }
    }

}

impl GroupMaxUpdateLength {

    pub fn new() -> Self {
        Self::Unbounded
    }

    pub fn add(&mut self, max_update_length: MaxUpdateLength) {
        match self {
            Self::Unbounded => {
                match max_update_length {
                    MaxUpdateLength::Unbounded => { },  // do nothing
                    MaxUpdateLength::ValidGrow(length) => { *self = Self::ValidGrow(length) },
                    _ => { *self = Self::Conflicts(vec![max_update_length]) },
                }
            },
            Self::ValidGrow(current_length) => {
                match max_update_length {
                    MaxUpdateLength::Unbounded => { },  // do nothing
                    MaxUpdateLength::ValidGrow(length) => {
                        *self = Self::ValidGrow(std::cmp::min(current_length.clone(), length))
                    }
                    _ => { *self = Self::Conflicts(vec![max_update_length]) },
                }
            },
            Self::Conflicts(conflicts) => {
                match max_update_length {
                    MaxUpdateLength::Unbounded => { },  // do nothing
                    MaxUpdateLength::ValidGrow(_) => { },  // do nothing
                    _ => {
                        conflicts.push(max_update_length);
                    },
                }
            },
        }
    }

    pub fn is_unbounded(&self) -> bool {
        matches!(self, Self::Unbounded)
    }

    pub fn get_valid_growth(&self) -> Option<Rational> {
        match self {
            Self::Unbounded => {
                panic!("please call GroupMaxUpdateLength::is_unbounded to check if it's unbounded");
            }
            Self::ValidGrow(length) => {
                Some(length.clone())
            },
            _ => { None }
        }
    }

    pub fn pop(&mut self) -> Option<MaxUpdateLength> {
        match self {
            Self::Unbounded | Self::ValidGrow(_) => {
                panic!("please call GroupMaxUpdateLength::get_valid_growth to check if this group is none_zero_growth");
            },
            Self::Conflicts(conflicts) => {
                conflicts.pop()
            }
        }
    }

    pub fn peek(&self) -> Option<&MaxUpdateLength> {
        match self {
            Self::Unbounded | Self::ValidGrow(_) => {
                panic!("please call GroupMaxUpdateLength::get_valid_growth to check if this group is none_zero_growth");
            },
            Self::Conflicts(conflicts) => {
                conflicts.last()
            }
        }
    }

}

impl DualModuleInterfacePtr {

    /// create an empty interface
    pub fn new_empty() -> Self {
        Self::new_value(DualModuleInterface {
            nodes: Vec::new(),
        })
    }

    /// a dual module interface MUST be created given a concrete implementation of the dual module
    pub fn new_load(syndrome_pattern: &SyndromePattern, dual_module_impl: &mut impl DualModuleImpl) -> Self {
        let interface_ptr = Self::new_empty();
        interface_ptr.load(syndrome_pattern, dual_module_impl);
        interface_ptr
    }

    pub fn load(&self, syndrome_pattern: &SyndromePattern, dual_module_impl: &mut impl DualModuleImpl) {
        for vertex_idx in syndrome_pattern.defect_vertices.iter() {
            self.create_defect_node(*vertex_idx, dual_module_impl);
        }
    }

    pub fn sum_dual_variables(&self) -> Rational {
        let interface = self.read_recursive();
        let mut sum = Rational::zero();
        for dual_node_ptr in interface.nodes.iter() {
            let dual_node = dual_node_ptr.read_recursive();
            sum += dual_node.dual_variable.clone();
        }
        sum
    }

    pub fn clear(&self) {
        let mut interface = self.write();
        interface.nodes.clear();
    }

    pub fn get_node(&self, node_index: NodeIndex) -> Option<DualNodePtr> {
        let interface = self.read_recursive();
        interface.nodes.get(node_index).cloned()
    }

    pub fn create_defect_node(&self, vertex_idx: VertexIndex, dual_module: &mut impl DualModuleImpl) -> DualNodePtr {
        let mut interface = self.write();
        let mut internal_vertices = BTreeSet::new();
        internal_vertices.insert(vertex_idx);
        let node_ptr = DualNodePtr::new_value(DualNode {
            index: interface.nodes.len(),
            internal_edges: BTreeSet::new(),  // empty
            internal_vertices,
            hair_edges: BTreeSet::new(),  // to be filled by concrete implementation
            dual_variable: Rational::zero(),
            grow_rate: Rational::one(),
        });
        let cloned_node_ptr = node_ptr.clone();
        interface.nodes.push(node_ptr);
        drop(interface);
        dual_module.add_dual_node(&cloned_node_ptr);
        cloned_node_ptr
    }

    pub fn create_cluster_node(&self, internal_edges: BTreeSet<EdgeIndex>, internal_vertices: BTreeSet<EdgeIndex>, dual_module: &mut impl DualModuleImpl) -> DualNodePtr {
        let mut interface = self.write();
        let node_ptr = DualNodePtr::new_value(DualNode {
            index: interface.nodes.len(),
            internal_edges: internal_edges,
            internal_vertices: internal_vertices,
            hair_edges: BTreeSet::new(),  // to be filled by concrete implementation
            dual_variable: Rational::zero(),
            grow_rate: Rational::one(),
        });
        let cloned_node_ptr = node_ptr.clone();
        interface.nodes.push(node_ptr);
        drop(interface);
        dual_module.add_dual_node(&cloned_node_ptr);
        cloned_node_ptr
    }

    pub fn create_cluster_node_auto_vertices(&self, internal_edges: BTreeSet<EdgeIndex>, dual_module: &mut impl DualModuleImpl) -> DualNodePtr {
        let internal_vertices = dual_module.get_edges_neighbors(&internal_edges);
        self.create_cluster_node(internal_edges, internal_vertices, dual_module)
    }

}

impl MWPSVisualizer for DualModuleInterfacePtr {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        let interface = self.read_recursive();
        let mut dual_nodes = Vec::<serde_json::Value>::new();
        for dual_node_ptr in interface.nodes.iter() {
            let dual_node = dual_node_ptr.read_recursive();
            dual_nodes.push(json!({
                if abbrev { "e" } else { "internal_edges" }: dual_node.internal_edges,
                if abbrev { "v" } else { "internal_vertices" }: dual_node.internal_vertices,
                if abbrev { "h" } else { "hair_edges" }: dual_node.hair_edges,
                if abbrev { "d" } else { "dual_variable" }: dual_node.dual_variable.to_f64(),
                if abbrev { "dn" } else { "dual_variable_numerator" }: dual_node.dual_variable.numer().to_i64(),
                if abbrev { "dd" } else { "dual_variable_denominator" }: dual_node.dual_variable.denom().to_i64(),
                if abbrev { "r" } else { "grow_rate" }: dual_node.grow_rate.to_f64(),
                if abbrev { "rn" } else { "grow_rate_numerator" }: dual_node.grow_rate.numer().to_i64(),
                if abbrev { "rd" } else { "grow_rate_denominator" }: dual_node.grow_rate.denom().to_i64(),
            }));
        }
        let sum_dual = self.sum_dual_variables();
        json!({
            "interface": {
                "sum_dual": sum_dual.to_f64(),
                "sdn": sum_dual.numer().to_i64(),
                "sdd": sum_dual.denom().to_i64(),
            },
            "dual_nodes": dual_nodes,
        })
    }
}
