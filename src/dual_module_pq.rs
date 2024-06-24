//! Dual Module with Priority Queue
//!
//! A serial implementation of the dual module with priority queue optimization
//!
//! Only debug tests are failing, which aligns with the dual_module_serial behavior
//!

use crate::num_traits::{ToPrimitive, Zero};
use crate::pointers::*;
use crate::util::*;
use crate::visualize::*;
use crate::{add_shared_methods, dual_module::*};

use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeSet, BinaryHeap},
};

use derivative::Derivative;
use itertools::Itertools;
use num_traits::{FromPrimitive, Signed};
use parking_lot::{lock_api::RwLockWriteGuard, RawRwLock};

/* Helper structs for events/obstacles during growing */
#[derive(Debug, Clone)]
pub struct FutureEvent<T: Ord + PartialEq + Eq, E> {
    /// when the event will happen
    pub time: T,
    /// the event
    pub event: E,
}

impl<T: Ord + PartialEq + Eq, E> PartialEq for FutureEvent<T, E> {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

impl<T: Ord + PartialEq + Eq, E> Eq for FutureEvent<T, E> {}

impl<T: Ord + PartialEq + Eq, E> Ord for FutureEvent<T, E> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.time.cmp(&other.time)
    }
}

impl<T: Ord + PartialEq + Eq, E> PartialOrd for FutureEvent<T, E> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Obstacle {
    Conflict { edge_index: EdgeIndex },
    ShrinkToZero { dual_node_ptr: DualNodePtr },
}

impl Obstacle {
    /// return if the current obstacle is valid, only needed for pq that allows for invalid (duplicates that are different) events
    fn is_valid<Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Clone>(
        &self,
        dual_module_pq: &DualModulePQ<Queue>,
        event_time: &Rational, // time associated with the obstacle
    ) -> bool {
        #[allow(clippy::unnecessary_cast)]
        match self {
            Obstacle::Conflict { edge_index } => {
                let edge = dual_module_pq.edges[*edge_index as usize].read_recursive();
                // not changing, cannot have conflict
                if !edge.grow_rate.is_positive() {
                    return false;
                }
                let growth_at_event_time =
                    &edge.growth_at_last_updated_time + (event_time - &edge.last_updated_time) * &edge.grow_rate;
                if edge.grow_rate.is_positive() {
                    // postivie grow rate, should become tight
                    if growth_at_event_time != edge.weight {
                        return false;
                    }
                }
            }
            Obstacle::ShrinkToZero { dual_node_ptr } => {
                let node = dual_node_ptr.read_recursive();
                // only negative grow rates can shrink to zero
                if !node.grow_rate.is_negative() {
                    return false;
                }
                let growth_at_event_time =
                    &node.dual_variable_at_last_updated_time + (event_time - &node.last_updated_time) * &node.grow_rate;
                if !growth_at_event_time.is_zero() {
                    return false;
                }
            }
        };
        true
    }
}

pub type FutureObstacle<T> = FutureEvent<T, Obstacle>;
pub type MinBinaryHeap<F> = BinaryHeap<Reverse<F>>;
pub type FutureObstacleQueue<T> = MinBinaryHeap<FutureObstacle<T>>;

pub trait FutureQueueMethods<T: Ord + PartialEq + Eq + std::fmt::Debug, E: std::fmt::Debug> {
    /// defines the behavior of `will_happen`, if the queue can contain invalid/duplicate events
    const MAY_BE_INVALID: bool = true;

    /// Append an event at time T
    ///     Note: this may have multiple distinct yet valid behaviors, e,g, weather there are duplicates allowed in the data strcture, default to allow
    fn will_happen(&mut self, time: T, event: E);

    /// peek for a queue
    fn peek_event(&self) -> Option<(&T, &E)>;

    /// pop for a queue
    fn pop_event(&mut self) -> Option<(T, E)>;

    /// clear for a queue
    fn clear(&mut self);
}

impl<T: Ord + PartialEq + Eq + std::fmt::Debug, E: std::fmt::Debug> FutureQueueMethods<T, E>
    for MinBinaryHeap<FutureEvent<T, E>>
{
    fn will_happen(&mut self, time: T, event: E) {
        self.push(Reverse(FutureEvent { time, event }))
    }
    fn peek_event(&self) -> Option<(&T, &E)> {
        self.peek().map(|future| (&future.0.time, &future.0.event))
    }
    fn pop_event(&mut self) -> Option<(T, E)> {
        self.pop().map(|future| (future.0.time, future.0.event))
    }
    fn clear(&mut self) {
        self.clear();
    }

    const MAY_BE_INVALID: bool = true;
}

/* Vertices and Edges */
#[derive(Derivative)]
#[derivative(Debug)]
pub struct Vertex {
    /// the index of this vertex in the decoding graph, not necessary the index in [`DualModuleSerial::vertices`] if it's partitioned
    pub vertex_index: VertexIndex,
    /// if a vertex is defect, then [`Vertex::propagated_dual_node`] always corresponds to that root
    pub is_defect: bool,
    /// all neighbor edges, in surface code this should be constant number of edges
    #[derivative(Debug = "ignore")]
    pub edges: Vec<EdgeWeak>,
}

impl Vertex {
    fn clear(&mut self) {
        self.is_defect = false;
    }
}

pub type VertexPtr = ArcRwLock<Vertex>;
pub type VertexWeak = WeakRwLock<Vertex>;

impl std::fmt::Debug for VertexPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let vertex = self.read_recursive();
        write!(f, "{}", vertex.vertex_index)
    }
}

impl std::fmt::Debug for VertexWeak {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let vertex_ptr = self.upgrade_force();
        let vertex = vertex_ptr.read_recursive();
        write!(f, "{}", vertex.vertex_index)
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Edge {
    /// global edge index
    edge_index: EdgeIndex,
    /// total weight of this edge
    weight: Rational,
    #[derivative(Debug = "ignore")]
    vertices: Vec<VertexWeak>,
    /// the dual nodes that contributes to this edge
    dual_nodes: Vec<DualNodeWeak>,

    /* fields that are different from that of dual_module_serial, or slightly differently interpreted */
    /// the speed of growth, at the current time
    ///     Note: changing this should cause the `growth_at_last_updated_time` and `last_updated_time` to update
    grow_rate: Rational,
    /// the last time this Edge is synced/updated with the global time
    last_updated_time: Rational,
    /// growth value at the last updated time, also, growth_at_last_updated_time <= weight
    growth_at_last_updated_time: Rational,
}

impl Edge {
    fn clear(&mut self) {
        self.growth_at_last_updated_time = Rational::zero();
        self.last_updated_time = Rational::zero();
        self.dual_nodes.clear();
    }
}

pub type EdgePtr = ArcRwLock<Edge>;
pub type EdgeWeak = WeakRwLock<Edge>;

impl std::fmt::Debug for EdgePtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let edge = self.read_recursive();
        write!(
            f,
            "[edge: {}]: weight: {}, grow_rate: {}, growth_at_last_updated_time: {}, last_updated_time: {}\n\tdual_nodes: {:?}\n",
            edge.edge_index, 
            edge.weight, 
            edge.grow_rate, 
            edge.growth_at_last_updated_time, 
            edge.last_updated_time, 
            edge.dual_nodes.iter().filter(|node| !node.upgrade_force().read_recursive().grow_rate.is_zero()).collect::<Vec<_>>()
        )
    }
}

impl std::fmt::Debug for EdgeWeak {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let edge_ptr = self.upgrade_force();
        let edge = edge_ptr.read_recursive();
        write!(
            f,
            "[edge: {}]: weight: {}, grow_rate: {}, growth_at_last_updated_time: {}, last_updated_time: {}\n\tdual_nodes: {:?}\n",
            edge.edge_index, edge.weight, edge.grow_rate, edge.growth_at_last_updated_time, edge.last_updated_time, edge.dual_nodes.iter().filter(|node| !node.upgrade_force().read_recursive().grow_rate.is_zero()).collect::<Vec<_>>()
        )
    }
}

/* the actual dual module */
pub struct DualModulePQ<Queue>
where
    Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Clone,
{
    /// all vertices including virtual ones
    pub vertices: Vec<VertexPtr>,
    /// keep edges, which can also be accessed in [`Self::vertices`]
    pub edges: Vec<EdgePtr>,
    /// storing all the anticipated events in a priority queue structure
    ///     ordering: the event in the most recent future is at the head of the queue
    obstacle_queue: Queue,
    /// the global time of this dual module
    ///     Note: Wrap-around edge case is not currently considered
    global_time: ArcRwLock<Rational>,

    /// the current mode of the dual module
    ///     note: currently does not have too much functionality
    mode: DualModuleMode,
}

impl<Queue> DualModulePQ<Queue>
where
    Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Clone,
{
    /// helper function to bring an edge update to speed with current time if needed
    fn update_edge_if_necessary(&self, edge: &mut RwLockWriteGuard<RawRwLock, Edge>) {
        let global_time = self.global_time.read_recursive();
        if edge.last_updated_time == global_time.clone() {
            // the edge is not behind
            return;
        }

        debug_assert!(
            global_time.clone() >= edge.last_updated_time,
            "global time is behind, maybe a wrap-around has happened"
        );

        let time_diff = global_time.clone() - &edge.last_updated_time;
        let newly_grown_amount = &time_diff * &edge.grow_rate;
        edge.growth_at_last_updated_time += newly_grown_amount;
        edge.last_updated_time = global_time.clone();
        debug_assert!(
            edge.growth_at_last_updated_time <= edge.weight,
            "growth larger than weight: check if events are 1) inserted and 2) handled correctly"
        );
    }

    /// helper function to bring a dual node update to speed with current time if needed
    fn update_dual_node_if_necessary(&mut self, node: &mut RwLockWriteGuard<RawRwLock, DualNode>) {
        let global_time = self.global_time.read_recursive();
        if node.last_updated_time == global_time.clone() {
            // the edge is not behind
            return;
        }

        debug_assert!(
            global_time.clone() >= node.last_updated_time,
            "global time is behind, maybe a wrap-around has happened"
        );

        let dual_variable = node.get_dual_variable();
        node.set_dual_variable(dual_variable);
        node.last_updated_time = global_time.clone();
        debug_assert!(
            !node.get_dual_variable().is_negative(),
            "negative dual variable: check if events are 1) inserted and 2) handled correctly"
        );
    }

    /// debugging function
    #[allow(dead_code)]
    fn debug_update_all(&mut self, dual_node_ptrs: &[DualNodePtr]) {
        // updating all edges
        for edge in self.edges.iter() {
            let mut edge = edge.write();
            self.update_edge_if_necessary(&mut edge);
        }
        // updating all dual nodes
        for dual_node_ptr in dual_node_ptrs.iter() {
            let mut dual_node = dual_node_ptr.write();
            self.update_dual_node_if_necessary(&mut dual_node);
        }
    }
}

pub type DualModulePQlPtr<Queue> = ArcRwLock<DualModulePQ<Queue>>;
pub type DualModulePQWeak<Queue> = WeakRwLock<DualModulePQ<Queue>>;

impl<Queue> DualModuleImpl for DualModulePQ<Queue>
where
    Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Clone,
{
    /// initialize the dual module, which is supposed to be reused for multiple decoding tasks with the same structure
    #[allow(clippy::unnecessary_cast)]
    fn new_empty(initializer: &SolverInitializer) -> Self {
        initializer.sanity_check().unwrap();
        // create vertices
        let vertices: Vec<VertexPtr> = (0..initializer.vertex_num)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                })
            })
            .collect();
        // set edges
        let mut edges = Vec::<EdgePtr>::new();
        for hyperedge in initializer.weighted_edges.iter() {
            let edge_ptr = EdgePtr::new_value(Edge {
                edge_index: edges.len() as EdgeIndex,
                weight: Rational::from_usize(hyperedge.weight).unwrap(),
                dual_nodes: vec![],
                vertices: hyperedge
                    .vertices
                    .iter()
                    .map(|i| vertices[*i as usize].downgrade())
                    .collect::<Vec<_>>(),
                last_updated_time: Rational::zero(),
                growth_at_last_updated_time: Rational::zero(),
                grow_rate: Rational::zero(),
            });
            for &vertex_index in hyperedge.vertices.iter() {
                vertices[vertex_index as usize].write().edges.push(edge_ptr.downgrade());
            }
            edges.push(edge_ptr);
        }
        Self {
            vertices,
            edges,
            obstacle_queue: Queue::default(),
            global_time: ArcRwLock::new_value(Rational::zero()),
            mode: DualModuleMode::default(),
        }
    }

    /// clear all growth and existing dual nodes
    fn clear(&mut self) {
        self.vertices.iter().for_each(|p| p.write().clear());
        self.edges.iter().for_each(|p| p.write().clear());

        self.obstacle_queue.clear();
        self.global_time.write().set_zero();
        self.mode_mut().reset();
    }

    #[allow(clippy::unnecessary_cast)]
    /// Adding a defect node to the DualModule
    fn add_defect_node(&mut self, dual_node_ptr: &DualNodePtr) {
        let dual_node = dual_node_ptr.read_recursive();
        debug_assert!(dual_node.invalid_subgraph.edges.is_empty());
        debug_assert!(
            dual_node.invalid_subgraph.vertices.len() == 1,
            "defect node (without edges) should only work on a single vertex, for simplicity"
        );
        let vertex_index = dual_node.invalid_subgraph.vertices.iter().next().unwrap();
        let mut vertex = self.vertices[*vertex_index as usize].write();
        assert!(!vertex.is_defect, "defect should not be added twice");
        vertex.is_defect = true;
        drop(dual_node);
        drop(vertex);
        self.add_dual_node(dual_node_ptr);
    }

    #[allow(clippy::unnecessary_cast)]
    /// Mostly invoked by `add_defect_node`, triggering a pq update, and edges updates
    fn add_dual_node(&mut self, dual_node_ptr: &DualNodePtr) {
        dual_node_ptr.write().init_time(self.global_time.clone());
        let global_time = self.global_time.read_recursive();
        let dual_node_weak = dual_node_ptr.downgrade();
        let dual_node = dual_node_ptr.read_recursive();

        if dual_node.grow_rate.is_negative() {
            self.obstacle_queue.will_happen(
                // it is okay to use global_time now, as this must be up-to-speed
                dual_node.get_dual_variable().clone() / (-dual_node.grow_rate.clone()) + global_time.clone(),
                Obstacle::ShrinkToZero {
                    dual_node_ptr: dual_node_ptr.clone(),
                },
            );
        }

        for &edge_index in dual_node.invalid_subgraph.hair.iter() {
            let mut edge = self.edges[edge_index as usize].write();

            // should make sure the edge is up-to-speed before making its variables change
            self.update_edge_if_necessary(&mut edge);

            edge.grow_rate += &dual_node.grow_rate;
            edge.dual_nodes.push(dual_node_weak.clone());

            if edge.grow_rate.is_positive() {
                self.obstacle_queue.will_happen(
                    // it is okay to use global_time now, as this must be up-to-speed
                    (edge.weight.clone() - edge.growth_at_last_updated_time.clone()) / edge.grow_rate.clone()
                        + global_time.clone(),
                    Obstacle::Conflict { edge_index },
                );
            }
        }
    }

    #[allow(clippy::unnecessary_cast)]
    fn add_dual_node_tune(&mut self, dual_node_ptr: &DualNodePtr) {
        let dual_node_weak = dual_node_ptr.downgrade();
        let dual_node = dual_node_ptr.read_recursive();

        for &edge_index in dual_node.invalid_subgraph.hair.iter() {
            let mut edge = self.edges[edge_index as usize].write();

            edge.grow_rate += &dual_node.grow_rate;
            edge.dual_nodes.push(dual_node_weak.clone());
        }
    }

    #[allow(clippy::unnecessary_cast)]
    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        let mut dual_node = dual_node_ptr.write();
        // println!("set_grow_rate invoked on {:?}, to be {:?}", dual_node.index, grow_rate);
        self.update_dual_node_if_necessary(&mut dual_node);

        let global_time = self.global_time.read_recursive();
        let grow_rate_diff = &grow_rate - &dual_node.grow_rate;

        dual_node.grow_rate = grow_rate;
        if dual_node.grow_rate.is_negative() {
            self.obstacle_queue.will_happen(
                // it is okay to use global_time now, as this must be up-to-speed
                dual_node.get_dual_variable().clone() / (-dual_node.grow_rate.clone()) + global_time.clone(),
                Obstacle::ShrinkToZero {
                    dual_node_ptr: dual_node_ptr.clone(),
                },
            );
        }
        drop(dual_node);

        let dual_node = dual_node_ptr.read_recursive();
        for &edge_index in dual_node.invalid_subgraph.hair.iter() {
            let mut edge = self.edges[edge_index as usize].write();
            self.update_edge_if_necessary(&mut edge);

            edge.grow_rate += &grow_rate_diff;
            if edge.grow_rate.is_positive() {
                self.obstacle_queue.will_happen(
                    // it is okay to use global_time now, as this must be up-to-speed
                    (edge.weight.clone() - edge.growth_at_last_updated_time.clone()) / edge.grow_rate.clone()
                        + global_time.clone(),
                    Obstacle::Conflict { edge_index },
                );
            }
        }
    }

    #[allow(clippy::unnecessary_cast)]
    fn set_grow_rate_tune(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        let mut dual_node = dual_node_ptr.write();

        let grow_rate_diff = &grow_rate - &dual_node.grow_rate;
        dual_node.grow_rate = grow_rate;

        for &edge_index in dual_node.invalid_subgraph.hair.iter() {
            let mut edge = self.edges[edge_index as usize].write();
            edge.grow_rate += &grow_rate_diff;
        }
    }

    fn compute_maximum_update_length(&mut self) -> GroupMaxUpdateLength {
        let global_time = self.global_time.read_recursive();
        // finding a valid event to process, only when invalids exist
        if Queue::MAY_BE_INVALID {
            // getting rid of all the invalid events
            while let Some((time, event)) = self.obstacle_queue.peek_event() {
                // found a valid event
                if event.is_valid(self, time) {
                    // valid grow
                    if time != &global_time.clone() {
                        return GroupMaxUpdateLength::ValidGrow(time - global_time.clone());
                    }
                    // goto else
                    break;
                }
                self.obstacle_queue.pop_event();
            }
        }

        // else , it is a valid conflict to resolve
        if let Some((_, event)) = self.obstacle_queue.pop_event() {
            // this is used, since queues are not sets, and can contain duplicate events
            // Note: chekc that this is the assumption, though not much more overhead anyway
            let mut group_max_update_length_set = BTreeSet::default();
            group_max_update_length_set.insert(match event {
                Obstacle::Conflict { edge_index } => MaxUpdateLength::Conflicting(edge_index),
                Obstacle::ShrinkToZero { dual_node_ptr } => {
                    let index = dual_node_ptr.read_recursive().index;
                    MaxUpdateLength::ShrinkProhibited(OrderedDualNodePtr::new(index, dual_node_ptr))
                }
            });

            // append all conflicts that happen at the same time as now
            while let Some((time, _)) = self.obstacle_queue.peek_event() {
                if &global_time.clone() == time {
                    let (time, event) = self.obstacle_queue.pop_event().unwrap();
                    if !event.is_valid(self, &time) {
                        continue;
                    }
                    // add
                    group_max_update_length_set.insert(match event {
                        Obstacle::Conflict { edge_index } => MaxUpdateLength::Conflicting(edge_index),
                        Obstacle::ShrinkToZero { dual_node_ptr } => {
                            let index = dual_node_ptr.read_recursive().index;
                            MaxUpdateLength::ShrinkProhibited(OrderedDualNodePtr::new(index, dual_node_ptr))
                        }
                    });
                } else {
                    break;
                }
            }

            return GroupMaxUpdateLength::Conflicts(group_max_update_length_set.into_iter().collect_vec());
        }

        // nothing useful could be done, return unbounded
        GroupMaxUpdateLength::new()
    }

    /// for pq implementation, simply updating the global time is enough, could be part of the `compute_maximum_update_length` function
    fn grow(&mut self, length: Rational) {
        debug_assert!(
            length.is_positive(),
            "growth should be positive; if desired, please set grow rate to negative for shrinking"
        );
        let mut global_time_write = self.global_time.write();
        *global_time_write = global_time_write.clone() + length;
    }

    /* identical with the dual_module_serial */
    #[allow(clippy::unnecessary_cast)]
    fn get_edge_nodes(&self, edge_index: EdgeIndex) -> Vec<DualNodePtr> {
        self.edges[edge_index as usize]
            .read_recursive()
            .dual_nodes
            .iter()
            .map(|x| x.upgrade_force())
            .collect()
    }

    #[allow(clippy::unnecessary_cast)]
    /// how much away from saturated is the edge
    fn get_edge_slack(&self, edge_index: EdgeIndex) -> Rational {
        let edge = self.edges[edge_index as usize].read_recursive();
        edge.weight.clone()
            - (self.global_time.read_recursive().clone() - edge.last_updated_time.clone()) * edge.grow_rate.clone()
            - edge.growth_at_last_updated_time.clone()
    }

    /// is the edge saturated
    fn is_edge_tight(&self, edge_index: EdgeIndex) -> bool {
        self.get_edge_slack(edge_index).is_zero()
    }

    /* tuning mode related new methods */

    // tuning mode shared methods
    add_shared_methods!();

    /// is the edge tight, but for tuning mode
    fn is_edge_tight_tune(&self, edge_index: EdgeIndex) -> bool {
        let edge = self.edges[edge_index as usize].read_recursive();
        edge.weight == edge.growth_at_last_updated_time
    }

    fn get_edge_slack_tune(&self, edge_index: EdgeIndex) -> Rational {
        let edge = self.edges[edge_index as usize].read_recursive();
        edge.weight.clone() - edge.growth_at_last_updated_time.clone()
    }

    /// change mode, clear queue as queue is no longer needed. also sync to get rid off the need for global time
    fn advance_mode(&mut self) {
        self.mode_mut().advance();
        self.obstacle_queue.clear();
        self.sync();
    }

    /// grow specific amount for a specific edge
    fn grow_edge(&self, edge_index: EdgeIndex, amount: &Rational) {
        let mut edge = self.edges[edge_index as usize].write();
        edge.growth_at_last_updated_time += amount;
    }

    /// sync all states and global time so the concept of time and pq can retire
    fn sync(&mut self) {

        let mut nodes_touched = BTreeSet::new();
        
        for edges in self.edges.iter_mut() {
            let mut edge = edges.write();

            // update if necessary
            let global_time = self.global_time.read_recursive();
            if edge.last_updated_time != global_time.clone() {
                // the edge is behind
                debug_assert!(
                    global_time.clone() >= edge.last_updated_time,
                    "global time is behind, maybe a wrap-around has happened"
                );

                let time_diff = global_time.clone() - &edge.last_updated_time;
                let newly_grown_amount = &time_diff * &edge.grow_rate;
                edge.growth_at_last_updated_time += newly_grown_amount;
                edge.last_updated_time = global_time.clone();
                debug_assert!(
                    edge.growth_at_last_updated_time <= edge.weight,
                    "growth larger than weight: check if events are 1) inserted and 2) handled correctly"
                );
            }

            for dual_node_ptr in edge.dual_nodes.iter() {
                let _dual_node_ptr = dual_node_ptr.upgrade_force();
                let node = _dual_node_ptr.read_recursive();
                if nodes_touched.contains(&node.index) {
                    continue;
                }
                nodes_touched.insert(node.index);

                // update if necessary
                let global_time = self.global_time.read_recursive();
                if node.last_updated_time != global_time.clone() {
                    // the node is behind
                    debug_assert!(
                        global_time.clone() >= node.last_updated_time,
                        "global time is behind, maybe a wrap-around has happened"
                    );

                    drop(node);
                    let mut node: RwLockWriteGuard<RawRwLock, DualNode> = _dual_node_ptr.write();

                    let dual_variable = node.get_dual_variable();
                    node.set_dual_variable(dual_variable);
                    node.last_updated_time = global_time.clone();
                    debug_assert!(
                        !node.get_dual_variable().is_negative(),
                        "negative dual variable: check if events are 1) inserted and 2) handled correctly"
                    );
                }
            }
        }
    }

    /// misc debug print statement
    fn debug_print(&self) {
        println!("\n[current states]");
        println!("global time: {:?}", self.global_time.read_recursive());
        println!(
            "edges: {:?}",
            self.edges
                .iter()
                .filter(|e| !e.read_recursive().grow_rate.is_zero())
                .collect::<Vec<&EdgePtr>>()
        );
        println!("pq: {:?}", self.obstacle_queue);
    }
}

impl<Queue> MWPSVisualizer for DualModulePQ<Queue>
where
    Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Clone,
{
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        let mut vertices: Vec<serde_json::Value> = vec![];
        for vertex_ptr in self.vertices.iter() {
            let vertex = vertex_ptr.read_recursive();
            vertices.push(json!({
                if abbrev { "s" } else { "is_defect" }: i32::from(vertex.is_defect),
            }));
        }
        let mut edges: Vec<serde_json::Value> = vec![];
        for edge_ptr in self.edges.iter() {
            let edge = edge_ptr.read_recursive();
            let current_growth = &edge.growth_at_last_updated_time
                + (&self.global_time.read_recursive().clone() - &edge.last_updated_time) * &edge.grow_rate;

            let unexplored = &edge.weight - &current_growth;
            edges.push(json!({
                if abbrev { "w" } else { "weight" }: edge.weight.to_f64(),
                if abbrev { "v" } else { "vertices" }: edge.vertices.iter().map(|x| x.upgrade_force().read_recursive().vertex_index).collect::<Vec<_>>(),
                if abbrev { "g" } else { "growth" }: current_growth.to_f64(),
                "gn": current_growth.numer().to_i64(),
                "gd": current_growth.denom().to_i64(),
                "un": unexplored.numer().to_i64(),
                "ud": unexplored.denom().to_i64(),
            }));
        }
        json!({
            "vertices": vertices,
            "edges": edges,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoding_hypergraph::*;
    use crate::example_codes::*;

    #[test]
    fn dual_module_pq_learn_priority_queue_1() {
        // cargo test dual_module_pq_learn_priority_queue_1 -- --nocapture
        let mut future_obstacle_queue = FutureObstacleQueue::<usize>::new();
        assert_eq!(0, future_obstacle_queue.len());
        macro_rules! ref_event {
            ($index:expr) => {
                Some((&$index, &Obstacle::Conflict { edge_index: $index }))
            };
        }
        macro_rules! value_event {
            ($index:expr) => {
                Some(($index, Obstacle::Conflict { edge_index: $index }))
            };
        }
        // test basic order
        future_obstacle_queue.will_happen(2, Obstacle::Conflict { edge_index: 2 });
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 1 });
        future_obstacle_queue.will_happen(3, Obstacle::Conflict { edge_index: 3 });
        assert_eq!(future_obstacle_queue.peek_event(), ref_event!(1));
        assert_eq!(future_obstacle_queue.peek_event(), ref_event!(1));
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(1));
        assert_eq!(future_obstacle_queue.peek_event(), ref_event!(2));
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(2));
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(3));
        assert_eq!(future_obstacle_queue.peek_event(), None);
        // test duplicate elements, the queue must be able to hold all the duplicate events
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 1 });
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 1 });
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 1 });
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(1));
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(1));
        assert_eq!(future_obstacle_queue.pop_event(), value_event!(1));
        assert_eq!(future_obstacle_queue.peek_event(), None);
        // test order of events at the same time
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 2 });
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 1 });
        future_obstacle_queue.will_happen(1, Obstacle::Conflict { edge_index: 3 });
        let mut events = vec![];
        while let Some((time, event)) = future_obstacle_queue.pop_event() {
            assert_eq!(time, 1);
            events.push(event);
        }
        assert_eq!(events.len(), 3);
        println!("events: {events:?}");
    }

    #[test]
    fn dual_module_pq_basics_1() {
        // cargo test dual_module_pq_basics_1 -- --nocapture
        let visualize_filename = "dual_module_pq_basics_1.json".to_string();
        let weight = 1000;
        let code = CodeCapacityColorCode::new(7, 0.1, weight);
        let mut visualizer = Visualizer::new(
            Some(visualize_data_folder() + visualize_filename.as_str()),
            code.get_positions(),
            true,
        )
        .unwrap();
        print_visualize_link(visualize_filename);
        // create dual module
        let model_graph = code.get_model_graph();
        let mut dual_module: DualModulePQ<FutureObstacleQueue<Rational>> = DualModulePQ::new_empty(&model_graph.initializer);
        // try to work on a simple syndrome
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, vec![3, 12]);
        let interface_ptr = DualModuleInterfacePtr::new_load(decoding_graph, &mut dual_module);

        visualizer
            .snapshot_combined("syndrome".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        // grow them each by half
        let dual_node_3_ptr = interface_ptr.read_recursive().nodes[0].clone();
        let dual_node_12_ptr = interface_ptr.read_recursive().nodes[1].clone();
        dual_module.set_grow_rate(&dual_node_3_ptr, Rational::from_usize(1).unwrap());
        dual_module.set_grow_rate(&dual_node_12_ptr, Rational::from_usize(1).unwrap());

        dual_module.grow(Rational::from_usize(weight / 2).unwrap());
        dual_module.debug_update_all(&interface_ptr.read_recursive().nodes);
        visualizer
            .snapshot_combined("grow".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();

        // cluster becomes solved
        dual_module.grow(Rational::from_usize(weight / 2).unwrap());
        dual_module.debug_update_all(&interface_ptr.read_recursive().nodes);
        visualizer
            .snapshot_combined("solved".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();

        // the result subgraph
        let subgraph = vec![15, 20];
        visualizer
            .snapshot_combined("subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph])
            .unwrap();
    }

    #[test]
    fn dual_module_pq_basics_2() {
        // cargo test dual_module_pq_basics_2 -- --nocapture
        let visualize_filename = "dual_module_pq_basics_2.json".to_string();
        let weight = 1000;
        let code = CodeCapacityTailoredCode::new(7, 0., 0.1, weight);
        let mut visualizer = Visualizer::new(
            Some(visualize_data_folder() + visualize_filename.as_str()),
            code.get_positions(),
            true,
        )
        .unwrap();
        print_visualize_link(visualize_filename);
        // create dual module
        let model_graph = code.get_model_graph();
        let mut dual_module: DualModulePQ<FutureObstacleQueue<Rational>> = DualModulePQ::new_empty(&model_graph.initializer);
        // try to work on a simple syndrome
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, vec![23, 24, 29, 30]);
        let interface_ptr = DualModuleInterfacePtr::new_load(decoding_graph, &mut dual_module);
        visualizer
            .snapshot_combined("syndrome".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();

        {
            let interface_ptr_read = interface_ptr.read_recursive();
            let dual_node_ptrs = interface_ptr_read.nodes.iter().take(4).cloned();
            dual_node_ptrs.for_each(|node_ptr| dual_module.set_grow_rate(&node_ptr, Rational::from_usize(1).unwrap()));
        }

        // grow them each by a quarter
        dual_module.grow(Rational::from_usize(weight / 4).unwrap());
        dual_module.debug_update_all(&interface_ptr.read_recursive().nodes);
        visualizer
            .snapshot_combined("solved".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();

        // the result subgraph
        let subgraph = vec![24];
        visualizer
            .snapshot_combined("subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph])
            .unwrap();
    }

    #[test]
    fn dual_module_pq_basics_3() {
        // cargo test dual_module_pq_basics_3 -- --nocapture
        let visualize_filename = "dual_module_pq_basics_3.json".to_string();
        let weight = 600; // do not change, the data is hard-coded
        let pxy = 0.0602828812732227;
        let code = CodeCapacityTailoredCode::new(7, pxy, 0.1, weight); // do not change probabilities: the data is hard-coded
        let mut visualizer = Visualizer::new(
            Some(visualize_data_folder() + visualize_filename.as_str()),
            code.get_positions(),
            true,
        )
        .unwrap();
        print_visualize_link(visualize_filename);
        // create dual module
        let model_graph = code.get_model_graph();
        let mut dual_module: DualModulePQ<FutureObstacleQueue<Rational>> = DualModulePQ::new_empty(&model_graph.initializer);
        // try to work on a simple syndrome
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, vec![17, 23, 29, 30]);
        let interface_ptr = DualModuleInterfacePtr::new_load(decoding_graph, &mut dual_module);
        visualizer
            .snapshot_combined("syndrome".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        let dual_node_17_ptr = interface_ptr.read_recursive().nodes[0].clone();
        let dual_node_23_ptr = interface_ptr.read_recursive().nodes[1].clone();
        let dual_node_29_ptr = interface_ptr.read_recursive().nodes[2].clone();
        let dual_node_30_ptr = interface_ptr.read_recursive().nodes[3].clone();

        // first round of growth
        dual_module.set_grow_rate(&dual_node_17_ptr, Rational::from_i64(1).unwrap());
        dual_module.set_grow_rate(&dual_node_23_ptr, Rational::from_i64(1).unwrap());
        dual_module.set_grow_rate(&dual_node_29_ptr, Rational::from_i64(1).unwrap());
        dual_module.set_grow_rate(&dual_node_30_ptr, Rational::from_i64(1).unwrap());

        dual_module.grow(Rational::from_i64(160).unwrap());
        dual_module.debug_update_all(&interface_ptr.read_recursive().nodes);

        visualizer
            .snapshot_combined("grow".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();

        // reset everything
        dual_module.set_grow_rate(&dual_node_17_ptr, Rational::from_i64(0).unwrap());
        dual_module.set_grow_rate(&dual_node_23_ptr, Rational::from_i64(0).unwrap());
        dual_module.set_grow_rate(&dual_node_29_ptr, Rational::from_i64(0).unwrap());
        dual_module.set_grow_rate(&dual_node_30_ptr, Rational::from_i64(0).unwrap());

        // create cluster
        interface_ptr.create_node_vec(&[24], &mut dual_module);
        let dual_node_cluster_ptr = interface_ptr.read_recursive().nodes[4].clone();
        dual_module.set_grow_rate(&dual_node_17_ptr, Rational::from_i64(1).unwrap());
        dual_module.set_grow_rate(&dual_node_cluster_ptr, Rational::from_i64(1).unwrap());
        dual_module.grow(Rational::from_i64(160).unwrap());
        dual_module.debug_update_all(&interface_ptr.read_recursive().nodes);

        visualizer
            .snapshot_combined("grow".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();

        // reset
        dual_module.set_grow_rate(&dual_node_17_ptr, Rational::from_i64(0).unwrap());
        dual_module.set_grow_rate(&dual_node_cluster_ptr, Rational::from_i64(0).unwrap());

        // create bigger cluster
        interface_ptr.create_node_vec(&[18, 23, 24, 31], &mut dual_module);
        let dual_node_bigger_cluster_ptr = interface_ptr.read_recursive().nodes[5].clone();
        dual_module.set_grow_rate(&dual_node_bigger_cluster_ptr, Rational::from_i64(1).unwrap());

        dual_module.grow(Rational::from_i64(120).unwrap());
        dual_module.debug_update_all(&interface_ptr.read_recursive().nodes);

        visualizer
            .snapshot_combined("solved".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();

        // the result subgraph
        let subgraph = vec![82, 24];
        visualizer
            .snapshot_combined("subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph])
            .unwrap();
    }

    // TODO: write more tests here, perhaps unit tests
}
