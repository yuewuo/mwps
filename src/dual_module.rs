//! Dual Module
//!
//! Generics for dual modules
//!

use crate::decoding_hypergraph::*;
use crate::derivative::Derivative;
use crate::invalid_subgraph::*;
use crate::model_hypergraph::*;
use crate::num_traits::{One, ToPrimitive, Zero};
use crate::pointers::*;
use crate::util::*;
use crate::visualize::*;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

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
    global_time: Option<ArcRwLock<Rational>>,
    /// the last time this dual_node is synced/updated with the global time
    pub last_updated_time: Rational,
    /// dual variable's value at the last updated time
    pub dual_variable_at_last_updated_time: Rational,
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
            "\n\t\tindex: {}, global_time: {:?}, grow_rate: {:?}, dual_variable: {}\n\t\tdual_variable_at_last_updated_time: {}, last_updated_time: {}",
            dual_node.index,
            global_time,
            dual_node.grow_rate,
            dual_node.get_dual_variable(),
            dual_node.dual_variable_at_last_updated_time,
            dual_node.last_updated_time
        )
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

////////////////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////////////////////////
/// Added by yl
// note that here, DualNodePtr = ArcRwLock<DualNode> instead of the ArcManualSafeLock<DualNode> in fusion blossom
impl DualNodePtr {
    // when fused, dual node may be outdated; refresh here
    pub fn update(&self) -> &Self {
        unimplemented!()
    }

    pub fn updated_index(&self) -> NodeIndex {
        self.update();
        self.read_recursive().index
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////////////////////////


/// an array of dual nodes
/// dual nodes, once created, will never be deconstructed until the next run
#[derive(Derivative)]
#[derivative(Debug)]
pub struct DualModuleInterface {
    /// all the dual node that can be used to control a concrete dual module implementation
    pub nodes: Vec<DualNodePtr>,
    /// given an invalid subgraph, find its corresponding dual node
    pub hashmap: HashMap<Arc<InvalidSubgraph>, NodeIndex>,
    /// the decoding graph
    pub decoding_graph: DecodingHyperGraph,
    /// current nodes length, to enable constant-time clear operation 
    pub nodes_length: usize,
    /// added by yl, for fusion, 
    /// allow pointer reuse will reduce the time of reallocation, but it's unsafe if not owning it;
    /// this will be automatically disabled when [`DualModuleInterface::fuse`] is called;
    /// if an interface is involved in a fusion operation (whether as parent or child), it will be set.
    pub is_fusion: bool,
    /// parent of this interface, when fused
    pub parent: Option<DualModuleInterfaceWeak>,
    /// when fused, this will indicate the relative bias given by the parent
    pub index_bias: NodeIndex,
    /// the two children of this interface, when fused; following the length of this child
    /// given that fused children interface will not have new nodes anymore
    pub children: Option<((DualModuleInterfaceWeak, NodeIndex), (DualModuleInterfaceWeak, NodeIndex))>,
    /// record theh total growing nodes, should be non-negative in a normal running algorithm
    pub sum_grow_speed: Rational,
    /// record the total sum of dual variables
    pub sum_dual_variables: Rational,
    
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

/// synchronize request on vertices, when a vertex is mirrored
#[derive(Derivative)]
#[derivative(Debug)]
pub struct SyncRequest {
    /// the unit that owns this vertex
    pub mirror_unit_weak: PartitionUnitWeak,
    /// the vertex index to be synchronized
    pub vertex_index: VertexIndex,
    /// propagated dual node index and the dual variable of the propagated dual node;
    /// this field is necessary to differentiate between normal shrink and the one that needs to report VertexShrinkStop event, when the syndrome is on the interface;
    /// it also includes the representative vertex of the dual node, so that parents can keep track of whether it should be elevated
    pub propagated_dual_node: Option<(DualNodeWeak, Weight, VertexIndex)>,
    /// propagated grandson node: must be a syndrome node
    pub propagated_grandson_dual_node: Option<(DualNodeWeak, Weight, VertexIndex)>,
}

impl SyncRequest {
    /// update all the interface nodes to be up-to-date, only necessary when there are fusion
    pub fn update(&self) {
        if let Some((weak, ..)) = &self.propagated_dual_node {
            weak.upgrade_force().update();
        }
        if let Some((weak, ..)) = &self.propagated_grandson_dual_node {
            weak.upgrade_force().update();
        }
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
    Conflicting(EdgeIndex),
    /// hitting 0 dual variable while shrinking, only happens when `grow_rate` < 0
    ShrinkProhibited(DualNodePtr),
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

    fn get_edge_nodes(&self, edge_index: EdgeIndex) -> Vec<DualNodePtr>;
    fn get_edge_slack(&self, edge_index: EdgeIndex) -> Rational;
    fn is_edge_tight(&self, edge_index: EdgeIndex) -> bool;


    /*
     * the following apis are only required when this dual module can be used as a partitioned one
     */

    /// create a partitioned dual module (hosting only a subgraph and subset of dual nodes) to be used in the parallel dual module
    fn new_partitioned(_partitioned_initializer: &PartitionedSolverInitializer) -> Self
    where
        Self: std::marker::Sized,
    {
        panic!("the dual module implementation doesn't support this function, please use another dual module")
    }

    /// prepare the growing or shrinking state of all nodes and return a list of sync requests in case of mirrored vertices are changed
    fn prepare_all(&mut self) -> &mut Vec<SyncRequest> {
        panic!("the dual module implementation doesn't support this function, please use another dual module")
    }

    /// execute a synchronize event by updating the state of a vertex and also update the internal dual node accordingly
    fn execute_sync_event(&mut self, _sync_event: &SyncRequest) {
        panic!("the dual module implementation doesn't support this function, please use another dual module")
    }

    /// judge whether the current module hosts the dual node
    fn contains_dual_node(&self, _dual_node_ptr: &DualNodePtr) -> bool {
        panic!("the dual module implementation doesn't support this function, please use another dual module")
    }

    /// judge whether the current module hosts any of these dual node
    fn contains_dual_nodes_any(&self, dual_node_ptrs: &[DualNodePtr]) -> bool {
        for dual_node_ptr in dual_node_ptrs.iter() {
            if self.contains_dual_node(dual_node_ptr) {
                return true;
            }
        }
        false
    }

    /// judge whether the current module hosts a vertex
    fn contains_vertex(&self, _vertex_index: VertexIndex) -> bool {
        panic!("the dual module implementation doesn't support this function, please use another dual module")
    }

    /// bias the global dual node indices
    fn bias_dual_node_index(&mut self, _bias: NodeIndex) {
        panic!("the dual module implementation doesn't support this function, please use another dual module")
    }

}

/// this dual module is a parallel version that hosts many partitioned ones
pub trait DualModuleParallelImpl {
    type UnitType: DualModuleImpl + Send + Sync;

    fn get_unit(&self, unit_index: usize) -> ArcRwLock<Self::UnitType>;
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
}

///////////////////////////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////////////////////////
/// Added by yl


impl DualModuleInterface {
    /// return the count of all nodes including those of the children interfaces
    pub fn nodes_count(&self) -> NodeNum {
        let mut count = self.nodes_length as NodeNum;
        if let Some(((_, left_count), (_, right_count))) = &self.children {
            count += left_count + right_count;
        }
        count
    }

    /// get node ptr by index; if calling from the ancestor interface, node_index is absolute, otherwise it's relative
    /// maybe delete it!!!
    #[allow(clippy::unnecessary_cast)]
    pub fn get_node(&self, relative_node_index: NodeIndex) -> Option<DualNodePtr> {
        debug_assert!(relative_node_index < self.nodes_count(), "cannot find node in this interface");
        let mut bias = 0;
        if let Some(((left_weak, left_count), (right_weak, right_count))) = &self.children {
            if relative_node_index < *left_count {
                // this node belongs to the left
                return left_weak.upgrade_force().read_recursive().get_node(relative_node_index);
            } else if relative_node_index < *left_count + *right_count {
                // this node belongs to the right
                return right_weak
                    .upgrade_force()
                    .read_recursive()
                    .get_node(relative_node_index - *left_count);
            }
            bias = left_count + right_count;
        }
        Some(self.nodes[(relative_node_index - bias) as usize].clone())
    }

    // /// set the corresponding node index to None
    // /// maybe delete it!!!
    // #[allow(clippy::unnecessary_cast)]
    // pub fn remove_node(&mut self, relative_node_index: NodeIndex) {
    //     debug_assert!(relative_node_index < self.nodes_count(), "cannot find node in this interface");
    //     let mut bias = 0;
    //     if let Some(((left_weak, left_count), (right_weak, right_count))) = &self.children {
    //         if relative_node_index < *left_count {
    //             // this node belongs to the left
    //             left_weak.upgrade_force().write().remove_node(relative_node_index);
    //             return;
    //         } else if relative_node_index < *left_count + *right_count {
    //             // this node belongs to the right
    //             right_weak
    //                 .upgrade_force()
    //                 .write()
    //                 .remove_node(relative_node_index - *left_count);
    //             return;
    //         }
    //         bias = left_count + right_count;
    //     }
    //     self.nodes[(relative_node_index - bias) as usize] = None; // we did not define nodes to be Option<DualNode>, so this line has type error and does not compile
    // }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////////////////////////

impl DualModuleInterfacePtr {
    pub fn new(model_graph: Arc<ModelHyperGraph>) -> Self {
        Self::new_value(DualModuleInterface {
            nodes: Vec::new(),
            hashmap: HashMap::new(),
            decoding_graph: DecodingHyperGraph::new(model_graph, Arc::new(SyndromePattern::new_empty())),
            is_fusion: false,
            parent: None,
            index_bias: 0,
            children: None,
            nodes_length: 0,
            sum_grow_speed: Rational::zero(),
            sum_dual_variables: Rational::zero(),
        })
    }

    /// a dual module interface MUST be created given a concrete implementation of the dual module
    pub fn new_load(decoding_graph: DecodingHyperGraph, dual_module_impl: &mut impl DualModuleImpl) -> Self {
        let interface_ptr = Self::new(decoding_graph.model_graph.clone());
        interface_ptr.load(decoding_graph.syndrome_pattern, dual_module_impl);
        interface_ptr
    }

    pub fn load(&self, syndrome_pattern: Arc<SyndromePattern>, dual_module_impl: &mut impl DualModuleImpl) {
        self.write().decoding_graph.set_syndrome(syndrome_pattern.clone());
        for vertex_idx in syndrome_pattern.defect_vertices.iter() {
            self.create_defect_node(*vertex_idx, dual_module_impl);
        }
    }

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
        let mut internal_vertices = BTreeSet::new();
        internal_vertices.insert(vertex_idx);
        let invalid_subgraph = Arc::new(InvalidSubgraph::new_complete(
            vec![vertex_idx].into_iter().collect(),
            BTreeSet::new(),
            &interface.decoding_graph,
        ));
        let node_index = interface.nodes.len() as NodeIndex;
        let node_ptr = DualNodePtr::new_value(DualNode {
            index: node_index,
            invalid_subgraph: invalid_subgraph.clone(),
            grow_rate: Rational::one(),
            dual_variable_at_last_updated_time: Rational::zero(),
            global_time: None,
            last_updated_time: Rational::zero(),
        });
        // println!("created node in create_defect_node {:?}", node_ptr);
        let cloned_node_ptr = node_ptr.clone();
        drop(interface);
        let mut interface = self.write();
        interface.nodes.push(node_ptr);
        interface.hashmap.insert(invalid_subgraph, node_index);
        drop(interface);
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
        });
        interface.nodes.push(node_ptr.clone());
        drop(interface);
        dual_module.add_dual_node(&node_ptr);
        // println!("created node in create_node {:?}", node_ptr);
        node_ptr
    }

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
        });
        interface.nodes.push(node_ptr.clone());
        drop(interface);
        dual_module.add_dual_node(&node_ptr);
        // println!("created node in create_node {:?}", node_ptr);
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

    /// return whether it's existing node or not
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
}

// shortcuts for easier code writing at debugging
impl DualModuleInterfacePtr {
    pub fn create_node_vec(&self, edges: &[EdgeIndex], dual_module: &mut impl DualModuleImpl) -> DualNodePtr {
        let invalid_subgraph = Arc::new(InvalidSubgraph::new(
            edges.iter().cloned().collect(),
            &self.read_recursive().decoding_graph,
        ));
        self.create_node(invalid_subgraph, dual_module)
    }
    pub fn create_node_complete_vec(
        &self,
        vertices: &[VertexIndex],
        edges: &[EdgeIndex],
        dual_module: &mut impl DualModuleImpl,
    ) -> DualNodePtr {
        let invalid_subgraph = Arc::new(InvalidSubgraph::new_complete(
            vertices.iter().cloned().collect(),
            edges.iter().cloned().collect(),
            &self.read_recursive().decoding_graph,
        ));
        self.create_node(invalid_subgraph, dual_module)
    }

    /// Added by yl
    /// tree structure fuse, same as fusion blossom 
    /// fuse 2 interfaces by (virtually) copying the nodes in `other` into myself, with O(1) time complexity
    /// consider implementating fuse as a chain, so that we do not have to copy; in other words, fusion should
    /// only depend on the boundary, not the volume of the block
    pub fn fuse(&self, left: &Self, right: &Self) {
        let parent_weak = self.downgrade();
        let left_weak = left.downgrade();
        let right_weak = right.downgrade();
        let mut interface = self.write();
        interface.is_fusion = true; // for sanity 
        debug_assert!(interface.children.is_none(), "cannot fuse twice");
        let mut left_interface = left.write();
        let mut right_interface = right.write();
        left_interface.is_fusion = true;
        right_interface.is_fusion = true;
        debug_assert!(left_interface.parent.is_none(), "cannot fuse an interface twice");
        debug_assert!(right_interface.parent.is_none(), "cannot fuse an interface twice");
        left_interface.parent = Some(parent_weak.clone());
        right_interface.parent = Some(parent_weak);
        left_interface.index_bias = 0;
        right_interface.index_bias = left_interface.nodes_count();
        interface.children = Some((
            (left_weak, left_interface.nodes_count()),
            (right_weak, right_interface.nodes_count()),
        ));
        for other_interface in [left_interface, right_interface] {
            interface.sum_dual_variables += other_interface.sum_dual_variables.clone();
            interface.sum_grow_speed += other_interface.sum_grow_speed.clone();
        }

    }
}

impl MWPSVisualizer for DualModuleInterfacePtr {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        let interface = self.read_recursive();
        let mut dual_nodes = Vec::<serde_json::Value>::new();
        for dual_node_ptr in interface.nodes.iter() {
            let dual_node = dual_node_ptr.read_recursive();
            dual_nodes.push(json!({
                if abbrev { "e" } else { "edges" }: dual_node.invalid_subgraph.edges,
                if abbrev { "v" } else { "vertices" }: dual_node.invalid_subgraph.vertices,
                if abbrev { "h" } else { "hair" }: dual_node.invalid_subgraph.hair,
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
