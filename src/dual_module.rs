//! Dual Module
//!
//! Generics for dual modules
//!


use color_print::cprint;
use color_print::cprintln;

use crate::dual_module_parallel::*;
use crate::dual_module_pq::{FutureQueueMethods, Obstacle};
use std::ops::DerefMut;
use crate::decoding_hypergraph::*;
use crate::derivative::Derivative;
use crate::invalid_subgraph::*;
use crate::model_hypergraph::*;
use crate::num_traits::{FromPrimitive, One, Signed, ToPrimitive, Zero};
use crate::ordered_float::OrderedFloat;
use crate::pointers::*;
use crate::primal_module::Affinity;
use crate::primal_module_serial::{PrimalClusterPtr, PrimalModuleSerialNodeWeak};
use crate::relaxer_optimizer::OptimizerResult;
use crate::util::*;
use crate::visualize::*;
use crate::matrix::*;

use std::collections::BTreeMap;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};

// this is not effecitively doing much right now due to the My (Leo's) desire for ultra performance (inlining function > branches)
#[derive(Default, Debug, Clone)]
pub enum DualModuleMode {
    /// Mode 1
    #[default]
    Search, // Searching for a solution

    /// Mode 2
    Tune, // Tuning for the optimal solution
}

impl DualModuleMode {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn advance(&mut self) {
        match self {
            Self::Search => *self = Self::Tune,
            Self::Tune => panic!("dual module mode is already in tune mode"),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::Search;
    }
}

// Each dual_module impl should have mode and affinity_map, hence these methods should be shared
//      Note: Affinity Map is not implemented in this branch, but a different file/branch (there incurs performance overhead)
#[macro_export]
macro_rules! add_shared_methods {
    () => {
        /// Returns a reference to the mode field.
        fn mode(&self) -> &DualModuleMode {
            &self.mode
        }

        /// Returns a mutable reference to the mode field.
        fn mode_mut(&mut self) -> &mut DualModuleMode {
            &mut self.mode
        }
    };
}

pub struct DualNode {
    /// the index of this dual node, helps to locate internal details of this dual node
    pub index: NodeIndex,
    /// the corresponding invalid subgraph
    pub invalid_subgraph: Arc<InvalidSubgraph>,

    /// the strategy to grow the dual variables
    pub grow_rate: Rational,
    /// the pointer to the global time
    /// Note: may employ some unsafe features while being sound in performance-critical cases
    ///       and can remove option when removing dual_module_serial
    pub global_time: Option<ArcRwLock<Rational>>,
    /// the last time this dual_node is synced/updated with the global time
    pub last_updated_time: Rational,
    /// dual variable's value at the last updated time
    pub dual_variable_at_last_updated_time: Rational,
    /// the corresponding PrimalModuleSerialNode
    pub primal_module_serial_node: Option<PrimalModuleSerialNodeWeak>,
}

impl DualNode {
    /// get the current up-to-date dual_variable
    pub fn get_dual_variable(&self) -> Rational {
        // in the interest of performance/avoiding redundant work, this may be upgraded to taking in
        // `&mut self` and update the value if needed
        match self.global_time.clone() {
            Some(global_time) => {
                // Note: clone here to give up read lock?
                let global_time = global_time.read_recursive();
                if self.last_updated_time < global_time.clone() {
                    (global_time.clone() - self.last_updated_time.clone()) * self.grow_rate.clone()
                        + self.dual_variable_at_last_updated_time.clone()
                } else {
                    self.dual_variable_at_last_updated_time.clone()
                }
            }
            None => self.dual_variable_at_last_updated_time.clone(),
        }
    }

    /// setter for current dual_variable
    pub fn set_dual_variable(&mut self, new_dual_variable: Rational) {
        self.dual_variable_at_last_updated_time = new_dual_variable;
    }

    /// initialize the global time pointer and the last_updated_time
    pub fn init_time(&mut self, global_time_ptr: ArcRwLock<Rational>) {
        self.last_updated_time = global_time_ptr.read_recursive().clone();
        self.global_time = Some(global_time_ptr);
    }
}

pub type DualNodePtr = ArcRwLock<DualNode>;
pub type DualNodeWeak = WeakRwLock<DualNode>;

impl std::fmt::Debug for DualNodePtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let dual_node = self.read_recursive(); // reading index is consistent
        let new = ArcRwLock::new_value(Rational::zero());
        let global_time = dual_node.global_time.as_ref().unwrap_or(&new).read_recursive();
        write!(
            f,
            "\n\t\tindex: {}, global_time: {:?}, grow_rate: {:?}, dual_variable: {}\n\t\tdual_variable_at_last_updated_time: {}, last_updated_time: {}\n\timpacted_edges: {:?}\n",
            dual_node.index,
            global_time,
            dual_node.grow_rate,
            dual_node.get_dual_variable(),
            dual_node.dual_variable_at_last_updated_time,
            dual_node.last_updated_time,
            dual_node.invalid_subgraph.hair.iter().map(|e| e.read_recursive().edge_index).collect::<Vec<_>>(),
        )
    }
}

impl std::fmt::Debug for DualNodeWeak {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.upgrade_force().fmt(f)
    }
}

impl DualNodePtr {
    /// we mainly use the vertex_index from this function to run bfs to find the partition unit responsible for this dual node
    pub fn get_representative_vertex(&self) -> VertexPtr {
        let dual_node = self.read_recursive();
        let defect_vertex = dual_node.invalid_subgraph.vertices.first().unwrap();
        defect_vertex.clone()
    }
}

/// an array of dual nodes
/// dual nodes, once created, will never be deconstructed until the next run
#[derive(Derivative)]
#[derivative(Debug)]
pub struct DualModuleInterface {
    /// all the dual node that can be used to control a concrete dual module implementation
    pub nodes: Vec<DualNodePtr>,
    /// given an invalid subgraph, find its corresponding dual node
    pub hashmap: HashMap<Arc<InvalidSubgraph>, NodeIndex>,
    // /// the decoding graph
    // pub decoding_graph: DecodingHyperGraph,
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
#[derive(Derivative, PartialEq, Eq, Clone, PartialOrd, Ord)]
#[derivative(Debug, Default(new = "true"))]
pub enum MaxUpdateLength {
    /// unbounded
    #[derivative(Default)]
    Unbounded,
    /// non-zero maximum update length
    ValidGrow(Rational),
    /// conflicting growth, violating the slackness constraint
    Conflicting(EdgePtr),
    /// hitting 0 dual variable while shrinking, only happens when `grow_rate` < 0
    ///     note: Using OrderedDualNodePtr since we can compare without acquiring the lock, for enabling btreeset/hashset/pq etc. with lower overhead
    ShrinkProhibited(OrderedDualNodePtr),
}

/// a pair of node index and dual node pointer, used for comparison without acquiring the lock
/// useful for when inserting into sets
#[derive(Derivative, PartialEq, Eq, Clone, Debug)]
pub struct OrderedDualNodePtr {
    pub index: NodeIndex,
    pub ptr: DualNodePtr,
}

impl OrderedDualNodePtr {
    pub fn new(index: NodeIndex, ptr: DualNodePtr) -> Self {
        Self { index, ptr }
    }
}
impl PartialOrd for OrderedDualNodePtr {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Some(self.index.cmp(&other.index))
        Some(self.cmp(other))
    }
}
impl Ord for OrderedDualNodePtr {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // self.index.cmp(&other.index)
        let ptr1 = Arc::as_ptr(self.ptr.ptr());
        let ptr2 = Arc::as_ptr(other.ptr.ptr());
        // https://doc.rust-lang.org/reference/types/pointer.html
        // "When comparing raw pointers they are compared by their address, rather than by what they point to."
        ptr1.cmp(&ptr2)
    }
}

#[derive(Derivative, PartialEq, Eq, Clone, Debug)]
pub struct OrderedDualNodeWeak {
    pub index: NodeIndex,
    pub weak_ptr: DualNodeWeak,
}

impl OrderedDualNodeWeak {
    pub fn new(index: NodeIndex, weak_ptr: DualNodeWeak) -> Self {
        Self { index, weak_ptr }
    }

    pub fn upgrade_force(&self) -> OrderedDualNodePtr {
        OrderedDualNodePtr::new(self.index, self.weak_ptr.upgrade_force())
    }
}
impl PartialOrd for OrderedDualNodeWeak {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Some(self.index.cmp(&other.index))
        Some(self.cmp(other))
    }
}
impl Ord for OrderedDualNodeWeak {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // the old version of cmp is to compare their indices only
        // compare the pointer address 
        let ptr1 = Arc::as_ptr(self.weak_ptr.upgrade_force().ptr());
        let ptr2 = Arc::as_ptr(other.weak_ptr.upgrade_force().ptr());
        // https://doc.rust-lang.org/reference/types/pointer.html
        // "When comparing raw pointers they are compared by their address, rather than by what they point to."
        ptr1.cmp(&ptr2)
    }
}

#[derive(Derivative, Clone)]
#[derivative(Debug, Default(new = "true"))]
pub enum GroupMaxUpdateLength {
    /// unbounded
    #[derivative(Default)]
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

    /// add defect node
    fn add_defect_node(&mut self, dual_node_ptr: &DualNodePtr);

    /// add corresponding dual node, note that the `internal_vertices` and `hair_edges` are not set
    fn add_dual_node(&mut self, dual_node_ptr: &DualNodePtr);

    /// update grow rate
    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational);

    /// An optional function that helps to break down the implementation of [`DualModuleImpl::compute_maximum_update_length`]
    /// check the maximum length to grow (shrink) specific dual node, if length is 0, give the reason of why it cannot further grow (shrink).
    /// if `simultaneous_update` is true, also check for the peer node according to [`DualNode::grow_state`].
    fn compute_maximum_update_length_dual_node(
        &mut self,
        _dual_node_ptr: &DualNodePtr,
        _simultaneous_update: bool,
    ) -> MaxUpdateLength {
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
    /// note that a negative growth should be implemented by reversing the speed of each dual node
    fn grow(&mut self, length: Rational);

    fn get_edge_nodes(&self, edge_ptr: EdgePtr) -> Vec<DualNodePtr>;
    fn get_edge_slack(&self, edge_ptr: EdgePtr) -> Rational;
    fn is_edge_tight(&self, edge_ptr: EdgePtr) -> bool;

    /* New tuning-related methods */
    /// mode mangements
    fn mode(&self) -> &DualModuleMode;
    fn mode_mut(&mut self) -> &mut DualModuleMode;
    fn advance_mode(&mut self) {
        eprintln!("this dual_module does not implement different modes");
    }
    fn reset_mode(&mut self) {
        *self.mode_mut() = DualModuleMode::default();
    }

    /// "set_grow_rate", but in tuning phase
    fn set_grow_rate_tune(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        eprintln!("this dual_module does not implement tuning");
        self.set_grow_rate(dual_node_ptr, grow_rate)
    }

    /// "add_dual_node", but in tuning phase
    fn add_dual_node_tune(&mut self, dual_node_ptr: &DualNodePtr) {
        eprintln!("this dual_module does not implement tuning");
        self.add_dual_node(dual_node_ptr);
    }

    /// syncing all possible states (dual_variable and edge_weights) with global time, so global_time can be discarded later
    fn sync(&mut self) {
        panic!("this dual_module does not have global time and does not need to sync");
    }

    /// grow a specific edge on the spot
    fn grow_edge(&self, _edge_ptr: EdgePtr, _amount: &Rational) {
        panic!("this dual_module doesn't support edge growth");
    }

    /// `is_edge_tight` but in tuning phase
    fn is_edge_tight_tune(&self, edge_ptr: EdgePtr) -> bool {
        eprintln!("this dual_module does not implement tuning");
        self.is_edge_tight(edge_ptr)
    }

    /// `get_edge_slack` but in tuning phase
    fn get_edge_slack_tune(&self, edge_ptr: EdgePtr) -> Rational {
        eprintln!("this dual_module does not implement tuning");
        self.get_edge_slack(edge_ptr)
    }

    /* miscs */

    /// print all the states for the current dual module
    fn debug_print(&self) {
        println!("this dual_module doesn't support debug print");
    }

    /* affinity */

    /// calculate affinity based on the following metric
    ///     Clusters with larger primal-dual gaps will receive high affinity because working on those clusters
    ///     will often reduce the gap faster. However, clusters with a large number of dual variables, vertices,
    ///     and hyperedges will receive a lower affinity
    fn calculate_cluster_affinity(&mut self, _cluster: PrimalClusterPtr) -> Option<Affinity> {
        eprintln!("not implemented, skipping");
        Some(OrderedFloat::from(100.0))
    }

    fn get_conflicts_tune(
        &self,
        optimizer_result: OptimizerResult,
        // dual_node_deltas: BTreeMap<OrderedDualNodePtr, Rational>,
        dual_node_deltas: BTreeMap<OrderedDualNodePtr, (Rational, PrimalClusterPtr)>,
    ) -> BTreeSet<MaxUpdateLength> {
        let mut conflicts: BTreeSet<MaxUpdateLength> = BTreeSet::new();
        match optimizer_result {
            OptimizerResult::EarlyReturned => {
                // if early returned, meaning optimizer didn't optimize, but simply should find current conflicts and return
                // for (dual_node_ptr, grow_rate) in dual_node_deltas.into_iter() {
                for (dual_node_ptr, (grow_rate, _)) in dual_node_deltas.into_iter() {
                    let node_ptr_read = dual_node_ptr.ptr.read_recursive();
                    if grow_rate.is_negative() && node_ptr_read.dual_variable_at_last_updated_time.is_zero() {
                        conflicts.insert(MaxUpdateLength::ShrinkProhibited(OrderedDualNodePtr::new(
                            node_ptr_read.index,
                            dual_node_ptr.ptr.clone(),
                        )));
                    }
                    for edge_ptr in node_ptr_read.invalid_subgraph.hair.iter() {
                        if grow_rate.is_positive() && self.is_edge_tight_tune(edge_ptr.clone()) {
                            conflicts.insert(MaxUpdateLength::Conflicting( edge_ptr.clone() ));
                        }
                    }
                }
            }
            OptimizerResult::Skipped => {
                // if skipped, should check if is growable, if not return the conflicts that leads to that conclusion
                // for (dual_node_ptr, grow_rate) in dual_node_deltas.into_iter() {
                for (dual_node_ptr, (grow_rate, cluster_index)) in dual_node_deltas.into_iter() {
                    // check if the single direction is growable
                    let mut actual_grow_rate = Rational::from_usize(std::usize::MAX).unwrap();
                    let node_ptr_read = dual_node_ptr.ptr.read_recursive();
                    for edge_ptr in node_ptr_read.invalid_subgraph.hair.iter() {
                        actual_grow_rate = std::cmp::min(actual_grow_rate, self.get_edge_slack_tune(edge_ptr.clone()));
                    }
                    if actual_grow_rate.is_zero() {
                        // if not, return the current conflicts
                        for edge_ptr in node_ptr_read.invalid_subgraph.hair.iter() {
                            if grow_rate.is_positive() && self.is_edge_tight_tune(edge_ptr.clone()) {
                                conflicts.insert(MaxUpdateLength::Conflicting( edge_ptr.clone()));
                            }
                        }
                        if grow_rate.is_negative() && node_ptr_read.dual_variable_at_last_updated_time.is_zero() {
                            conflicts.insert(MaxUpdateLength::ShrinkProhibited(OrderedDualNodePtr::new(
                                node_ptr_read.index,
                                dual_node_ptr.ptr.clone(),
                            )));
                        }
                    } else {
                        // if yes, grow and return new conflicts
                        drop(node_ptr_read);
                        let mut node_ptr_write = dual_node_ptr.ptr.write();
                        for edge_ptr in node_ptr_write.invalid_subgraph.hair.iter() {
                            self.grow_edge(edge_ptr.clone(), &actual_grow_rate);
                            #[cfg(feature = "incr_lp")]
                            self.update_edge_cluster_weights(*edge_index, cluster_index, actual_grow_rate); // note: comment out if not using cluster-based
                            if actual_grow_rate.is_positive() && self.is_edge_tight_tune(edge_ptr.clone()) {
                                conflicts.insert(MaxUpdateLength::Conflicting(edge_ptr.clone()));
                            }
                        }
                        node_ptr_write.dual_variable_at_last_updated_time += actual_grow_rate.clone();
                        if actual_grow_rate.is_negative() && node_ptr_write.dual_variable_at_last_updated_time.is_zero() {
                            conflicts.insert(MaxUpdateLength::ShrinkProhibited(OrderedDualNodePtr::new(
                                node_ptr_write.index,
                                dual_node_ptr.ptr.clone(),
                            )));
                        }
                    }
                }
            }
            _ => {
                // in other cases, optimizer should have optimized, so we should apply the deltas and return the new conflicts
                let mut edge_deltas: BTreeMap<EdgePtr, OrderedFloat> = BTreeMap::new();
                // for (dual_node_ptr, grow_rate) in dual_node_deltas.into_iter() {
                for (dual_node_ptr, (grow_rate, cluster_index)) in dual_node_deltas.into_iter() {
                    // update the dual node and check for conflicts
                    let mut node_ptr_write = dual_node_ptr.ptr.write();
                    node_ptr_write.dual_variable_at_last_updated_time += grow_rate.clone();
                    if grow_rate.is_negative() && node_ptr_write.dual_variable_at_last_updated_time.is_zero() {
                        conflicts.insert(MaxUpdateLength::ShrinkProhibited(OrderedDualNodePtr::new(
                            node_ptr_write.index,
                            dual_node_ptr.ptr.clone(),
                        )));
                    }

                    // calculate the total edge deltas
                    for edge_ptr in node_ptr_write.invalid_subgraph.hair.iter() {
                        match edge_deltas.entry(edge_ptr.clone()) {
                            std::collections::btree_map::Entry::Vacant(v) => {
                                v.insert(grow_rate.clone());
                            }
                            std::collections::btree_map::Entry::Occupied(mut o) => {
                                let current = o.get_mut();
                                *current += grow_rate.clone();
                            }
                        }
                        #[cfg(feature = "incr_lp")]
                        self.update_edge_cluster_weights(*edge_index, cluster_index, grow_rate.clone());
                        // note: comment out if not using cluster-based
                    }
                }

                // apply the edge deltas and check for conflicts
                for (edge_ptr, grow_rate) in edge_deltas.into_iter() {
                    if grow_rate.is_zero() {
                        continue;
                    }
                    self.grow_edge(edge_ptr.clone(), &grow_rate);
                    if grow_rate.is_positive() && self.is_edge_tight_tune(edge_ptr.clone()) {
                        conflicts.insert(MaxUpdateLength::Conflicting(edge_ptr.clone()));
                    }
                }
            }
        }
        conflicts
    }

    /// get the edge free weight, for each edge what is the weight that are free to use by the given participating dual variables
    fn get_edge_free_weight(
        &self,
        edge_ptr: EdgePtr,
        participating_dual_variables: &hashbrown::HashSet<usize>,
    ) -> Rational;

    #[cfg(feature = "incr_lp")]
    fn update_edge_cluster_weights(&self, edge_index: EdgeIndex, cluster_index: NodeIndex, grow_rate: Rational);

    #[cfg(feature = "incr_lp")]
    fn get_edge_free_weight_cluster(&self, edge_index: EdgeIndex, cluster_index: NodeIndex) -> Rational;

    #[cfg(feature = "incr_lp")]
    fn update_edge_cluster_weights_union(
        &self,
        dual_node_ptr: &DualNodePtr,
        drained_cluster_index: NodeIndex,
        absorbing_cluster_index: NodeIndex,
    );

    fn get_vertex_ptr(&self, vertex_index: VertexIndex) -> VertexPtr;
    fn get_edge_ptr(&self, edge_index: EdgeIndex) -> EdgePtr;
}

impl MaxUpdateLength {
    pub fn merge(&mut self, max_update_length: MaxUpdateLength) {
        match self {
            Self::Unbounded => {
                *self = max_update_length;
            }
            Self::ValidGrow(current_length) => {
                match max_update_length {
                    MaxUpdateLength::Unbounded => {} // do nothing
                    MaxUpdateLength::ValidGrow(length) => {
                        *self = Self::ValidGrow(std::cmp::min(current_length.clone(), length))
                    }
                    _ => *self = max_update_length,
                }
            }
            _ => {} // do nothing if it's already a conflict
        }
    }
}

impl GroupMaxUpdateLength {
    pub fn len (&mut self) -> usize{
        return match self {
            GroupMaxUpdateLength::Unbounded => 0,
            GroupMaxUpdateLength::ValidGrow(_) => 1,
            GroupMaxUpdateLength::Conflicts(conflicts) => conflicts.len(),
        }
    }
    pub fn add(&mut self, max_update_length: MaxUpdateLength) {
        match self {
            Self::Unbounded => {
                match max_update_length {
                    MaxUpdateLength::Unbounded => {} // do nothing
                    MaxUpdateLength::ValidGrow(length) => *self = Self::ValidGrow(length),
                    _ => *self = Self::Conflicts(vec![max_update_length]),
                }
            }
            Self::ValidGrow(current_length) => {
                match max_update_length {
                    MaxUpdateLength::Unbounded => {} // do nothing
                    MaxUpdateLength::ValidGrow(length) => {
                        *self = Self::ValidGrow(std::cmp::min(current_length.clone(), length))
                    }
                    _ => *self = Self::Conflicts(vec![max_update_length]),
                }
            }
            Self::Conflicts(conflicts) => {
                match max_update_length {
                    MaxUpdateLength::Unbounded => {}    // do nothing
                    MaxUpdateLength::ValidGrow(_) => {} // do nothing
                    _ => {
                        conflicts.push(max_update_length);
                    }
                }
            }
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
            Self::ValidGrow(length) => Some(length.clone()),
            _ => None,
        }
    }

    pub fn pop(&mut self) -> Option<MaxUpdateLength> {
        match self {
            Self::Unbounded | Self::ValidGrow(_) => {
                panic!("please call GroupMaxUpdateLength::get_valid_growth to check if this group is none_zero_growth");
            }
            Self::Conflicts(conflicts) => conflicts.pop(),
        }
    }

    pub fn peek(&self) -> Option<&MaxUpdateLength> {
        match self {
            Self::Unbounded | Self::ValidGrow(_) => {
                panic!("please call GroupMaxUpdateLength::get_valid_growth to check if this group is none_zero_growth");
            }
            Self::Conflicts(conflicts) => conflicts.last(),
        }
    }

    pub fn extend(&mut self, other: Self) {
        match self {
            Self::Conflicts(conflicts) => {
                if let Self::Conflicts(other_conflicts) = other {
                    conflicts.extend(other_conflicts);
                } // only add conflicts
            },
            Self::Unbounded => {
                match other {
                    Self::Unbounded => {} // do nothing
                    Self::ValidGrow(length) => *self = Self::ValidGrow(length),
                    Self::Conflicts(mut other_list) => {
                        let mut list = Vec::<MaxUpdateLength>::new();
                        std::mem::swap(&mut list, &mut other_list);
                        *self = Self::Conflicts(list);
                    }
                }
            },
            Self::ValidGrow(current_length) => match other {
                Self::Conflicts(mut other_list) => {
                    let mut list = Vec::<MaxUpdateLength>::new();
                    std::mem::swap(&mut list, &mut other_list);
                    *self = Self::Conflicts(list);
                }
                Self::Unbounded => {} // do nothing
                Self::ValidGrow(length) => {
                    *current_length = std::cmp::min(current_length.clone(), length);
                }
            }
        }
    }
}

impl DualModuleInterfacePtr {
    pub fn new() -> Self {
        Self::new_value(DualModuleInterface {
            nodes: Vec::new(),
            hashmap: HashMap::new(),
            // decoding_graph: DecodingHyperGraph::new(model_graph, Arc::new(SyndromePattern::new_empty())),
        })
    }

    /// a dual module interface MUST be created given a concrete implementation of the dual module
    pub fn new_load(syndrome_pattern: Arc<SyndromePattern>, dual_module_impl: &mut impl DualModuleImpl) -> Self {
        let interface_ptr = Self::new();
        interface_ptr.load(syndrome_pattern, dual_module_impl);
        interface_ptr
    }

    // the defect_vertices here are local vertices
    pub fn load(&self, syndrome_pattern: Arc<SyndromePattern>, dual_module_impl: &mut impl DualModuleImpl) {
        // self.write().decoding_graph.set_syndrome(syndrome_pattern.clone());
        for vertex_idx in syndrome_pattern.defect_vertices.iter() {
            self.create_defect_node(*vertex_idx, dual_module_impl);
        }
    }

    // // the defect_vertices here are local vertices
    // pub fn load_ptr<DualSerialModule: DualModuleImpl + Send + Sync, Queue>(
    //     &self, 
    //     syndrome_pattern: Arc<SyndromePattern>, 
    //     dual_module_ptr: &mut DualModuleParallelUnitPtr<DualSerialModule, Queue>,)
    // where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
    // {
    //     // self.write().decoding_graph.set_syndrome(syndrome_pattern.clone());
    //     for vertex_idx in syndrome_pattern.defect_vertices.iter() {
    //         self.create_defect_node(*vertex_idx, dual_module_ptr.write().deref_mut());
    //     }
    // }

    pub fn sum_dual_variables(&self) -> Rational {
        let interface = self.read_recursive();
        let mut sum = Rational::zero();
        for dual_node_ptr in interface.nodes.iter() {
            let dual_node = dual_node_ptr.read_recursive();
            sum += dual_node.get_dual_variable();
        }
        sum
    }

    pub fn clear(&self) {
        let mut interface = self.write();
        interface.nodes.clear();
        interface.hashmap.clear();
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn get_node(&self, node_index: NodeIndex) -> Option<DualNodePtr> {
        let interface = self.read_recursive();
        interface.nodes.get(node_index as usize).cloned()
    }

    /// make it private; use `load` instead
    fn create_defect_node(&self, vertex_idx: VertexIndex, dual_module: &mut impl DualModuleImpl) -> DualNodePtr {
        let interface = self.read_recursive();
        // let mut internal_vertices = BTreeSet::new();
        // internal_vertices.insert(vertex_idx);
        let vertex_ptr = dual_module.get_vertex_ptr(vertex_idx); // this is okay because create_defect_node is only called upon local defect vertices, so we won't access index out of range
        vertex_ptr.write().is_defect = true; // we change the is_defect to true, since is_defect is initialized as false for all vertex pointers
        let mut vertices = BTreeSet::new();
        vertices.insert(vertex_ptr);
        let invalid_subgraph = Arc::new(InvalidSubgraph::new_complete(
            &vertices,
            &BTreeSet::new()
        ));
        let node_index = interface.nodes.len() as NodeIndex;
        let node_ptr = DualNodePtr::new_value(DualNode {
            index: node_index,
            invalid_subgraph: invalid_subgraph.clone(),
            grow_rate: Rational::one(),
            dual_variable_at_last_updated_time: Rational::zero(),
            global_time: None,
            last_updated_time: Rational::zero(),
            primal_module_serial_node: None, // to be filled in when initializing a primalnode
        });

        let cloned_node_ptr = node_ptr.clone();
        drop(interface);
        let mut interface = self.write();
        interface.nodes.push(node_ptr.clone());
        interface.hashmap.insert(invalid_subgraph, node_index);
        drop(interface);
        // println!("node created in `create_defect_node`: {:?}", node_ptr.clone());
        dual_module.add_defect_node(&cloned_node_ptr);
        
        cloned_node_ptr
    }

    /// find existing node
    #[allow(clippy::unnecessary_cast)]
    pub fn find_node(&self, invalid_subgraph: &Arc<InvalidSubgraph>) -> Option<DualNodePtr> {
        let interface = self.read_recursive();
        interface
            .hashmap
            .get(invalid_subgraph)
            .map(|index| interface.nodes[*index as usize].clone())
    }

    pub fn create_node(&self, invalid_subgraph: Arc<InvalidSubgraph>, dual_module: &mut impl DualModuleImpl) -> DualNodePtr {
        // cprintln!("<yellow>create_node</yellow>");
        debug_assert!(
            self.find_node(&invalid_subgraph).is_none(),
            "do not create the same node twice"
        );
        let mut interface = self.write();
        let node_index = interface.nodes.len() as NodeIndex;
        interface.hashmap.insert(invalid_subgraph.clone(), node_index);
        let node_ptr = DualNodePtr::new_value(DualNode {
            index: node_index,
            invalid_subgraph,
            grow_rate: Rational::one(),
            dual_variable_at_last_updated_time: Rational::zero(),
            global_time: None,
            last_updated_time: Rational::zero(),
            primal_module_serial_node: None, // to be filled in when initializing a primalnode
        });
        interface.nodes.push(node_ptr.clone());
        drop(interface);
        dual_module.add_dual_node(&node_ptr);

        node_ptr
    }

    /// `create_node` for tuning
    pub fn create_node_tune(
        &self,
        invalid_subgraph: Arc<InvalidSubgraph>,
        dual_module: &mut impl DualModuleImpl,
    ) -> DualNodePtr {
        debug_assert!(
            self.find_node(&invalid_subgraph).is_none(),
            "do not create the same node twice"
        );
        let mut interface = self.write();
        let node_index = interface.nodes.len() as NodeIndex;
        interface.hashmap.insert(invalid_subgraph.clone(), node_index);
        let node_ptr = DualNodePtr::new_value(DualNode {
            index: node_index,
            invalid_subgraph,
            grow_rate: Rational::zero(),
            dual_variable_at_last_updated_time: Rational::zero(),
            global_time: None,
            last_updated_time: Rational::zero(),
            primal_module_serial_node: None, // to be filled in when initializing a primalnode
        });
        interface.nodes.push(node_ptr.clone());
        drop(interface);
        dual_module.add_dual_node_tune(&node_ptr);

        node_ptr
    }

    /// return whether it's existing node or not
    pub fn find_or_create_node(
        &self,
        invalid_subgraph: &Arc<InvalidSubgraph>,
        dual_module: &mut impl DualModuleImpl,
    ) -> (bool, DualNodePtr) {
        match self.find_node(invalid_subgraph) {
            Some(node_ptr) => (true, node_ptr),
            None => (false, self.create_node(invalid_subgraph.clone(), dual_module)),
        }
    }

    /// `find_or_create_node` for tuning
    pub fn find_or_create_node_tune(
        &self,
        invalid_subgraph: &Arc<InvalidSubgraph>,
        dual_module: &mut impl DualModuleImpl,
    ) -> (bool, DualNodePtr) {
        match self.find_node(invalid_subgraph) {
            Some(node_ptr) => (true, node_ptr),
            None => (false, self.create_node_tune(invalid_subgraph.clone(), dual_module)),
        }
    }

    pub fn is_valid_cluster_auto_vertices(&self, edges: &BTreeSet<EdgePtr>) -> bool {
        self.find_valid_subgraph_auto_vertices(edges).is_some()
    }

    pub fn find_valid_subgraph_auto_vertices(&self, edges: &BTreeSet<EdgePtr>) -> Option<Subgraph> {
        let mut vertices: BTreeSet<VertexPtr> = BTreeSet::new();
        for edge_ptr in edges.iter() {
            // let local_vertices = &edge_ptr.get_vertex_neighbors();
            let local_vertices = &edge_ptr.read_recursive().vertices;
            for vertex in local_vertices {
                vertices.insert(vertex.upgrade_force());
            }
        }

        self.find_valid_subgraph(edges, &vertices)
    }

    pub fn find_valid_subgraph(&self, edges: &BTreeSet<EdgePtr>, vertices: &BTreeSet<VertexPtr>) -> Option<Subgraph> {
        let mut matrix = Echelon::<CompleteMatrix>::new();
        for edge_index in edges.iter() {
            matrix.add_variable(edge_index.downgrade());
        }

        for vertex_index in vertices.iter() {
            let incident_edges = &vertex_index.read_recursive().edges;
            let parity = vertex_index.read_recursive().is_defect;
            matrix.add_constraint(vertex_index.downgrade(), &incident_edges, parity);
        }
        matrix.get_solution()
    }
}

// shortcuts for easier code writing at debugging
impl DualModuleInterfacePtr {
    pub fn create_node_vec(&self, edges: &[EdgeWeak], dual_module: &mut impl DualModuleImpl) -> DualNodePtr {
        let strong_edges = edges.iter()
            .filter_map(|weak_edge| weak_edge.upgrade())
            .collect();
        let invalid_subgraph = Arc::new(InvalidSubgraph::new(
            &strong_edges
        ));
        self.create_node(invalid_subgraph, dual_module)
    }
    pub fn create_node_complete_vec(
        &self,
        vertices: &[VertexWeak],
        edges: &[EdgeWeak],
        dual_module: &mut impl DualModuleImpl,
    ) -> DualNodePtr {
        let strong_edges = edges.iter()
            .filter_map(|weak_edge| weak_edge.upgrade())
            .collect();
        let strong_vertices = vertices.iter()
            .filter_map(|weak_vertex| weak_vertex.upgrade())
            .collect();

        let invalid_subgraph = Arc::new(InvalidSubgraph::new_complete(
            &strong_vertices,
            &strong_edges
        ));
        self.create_node(invalid_subgraph, dual_module)
    }
}

impl MWPSVisualizer for DualModuleInterfacePtr {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        let interface = self.read_recursive();
        let mut dual_nodes = Vec::<serde_json::Value>::new();
        for dual_node_ptr in interface.nodes.iter() {
            let dual_node = dual_node_ptr.read_recursive();
            let edges: Vec<usize> = dual_node.invalid_subgraph.edges.iter().map(|e|e.upgradable_read().edge_index).collect();
            let vertices: Vec<usize> = dual_node.invalid_subgraph.vertices.iter().map(|e|e.upgradable_read().vertex_index).collect();
            let hair: Vec<usize>  = dual_node.invalid_subgraph.hair.iter().map(|e|e.upgradable_read().edge_index).collect();
            dual_nodes.push(json!({
                if abbrev { "e" } else { "edges" }: edges,
                if abbrev { "v" } else { "vertices" }: vertices,
                if abbrev { "h" } else { "hair" }: hair,
                if abbrev { "d" } else { "dual_variable" }: dual_node.get_dual_variable().to_f64(),
                if abbrev { "dn" } else { "dual_variable_numerator" }: dual_node.get_dual_variable().numer().to_i64(),
                if abbrev { "dd" } else { "dual_variable_denominator" }: dual_node.get_dual_variable().denom().to_i64(),
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
