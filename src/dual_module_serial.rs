//! Serial Dual Module
//!
//! A serial implementation of the dual module
//!

use crate::derivative::Derivative;
use crate::dual_module::*;
use crate::num_traits::sign::Signed;
use crate::num_traits::{ToPrimitive, Zero};
use crate::pointers::*;
use crate::util::*;
use crate::visualize::*;
use num_traits::FromPrimitive;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::collections::HashMap;


pub struct DualModuleSerial {
    /// all vertices including virtual ones
    pub vertices: Vec<VertexPtr>,
    /// keep edges, which can also be accessed in [`Self::vertices`]
    pub edges: Vec<EdgePtr>,
    /// maintain an active list to optimize for average cases: most defect vertices have already been matched, and we only need to work on a few remained;
    /// note that this list may contain duplicate nodes
    pub active_edges: BTreeSet<EdgeIndex>,
    /// active nodes
    pub active_nodes: BTreeSet<DualNodePtr>,
    /// helps to deduplicate active_edges and active_nodes
    current_cycle: usize,
    /// temporary list of synchronize requests, i.e. those propagating into the mirrored vertices; should always be empty when not partitioned, i.e. serial version
    pub sync_requests: Vec<SyncRequest>,
    /// current timestamp
    pub active_timestamp: FastClearTimestamp,
    /// deduplicate edges in the boundary, helpful when the decoding problem is partitioned
    pub edge_dedup_timestamp: FastClearTimestamp,
    /// temporary variable to reduce reallocation
    updated_boundary: Vec<(bool, EdgeWeak)>,
    /// temporary variable to reduce reallocation
    propagating_vertices: Vec<(VertexWeak, Option<DualNodeInternalWeak>)>,
    /// nodes internal information
    pub nodes: Vec<Option<DualNodeInternalPtr>>,
}

pub type DualModuleSerialPtr = ArcRwLock<DualModuleSerial>;
pub type DualModuleSerialWeak = WeakRwLock<DualModuleSerial>;

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
    /// (added by yl) if it's a mirrored vertex (present on multiple units), then this is the parallel unit that exclusively owns it
    pub mirror_unit: Option<PartitionUnitWeak>,
    /// all neighbor edges, in surface code this should be constant number of edges
    #[derivative(Debug = "ignore")]
    /// propagated dual node
    pub propagated_dual_node: Option<DualNodeInternalWeak>,
    /// propagated grandson node: must be a syndrome node
    pub propagated_grandson_dual_node: Option<DualNodeInternalWeak>,
    /// for fast clear
    pub timestamp: FastClearTimestamp,
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
    /// growth value, growth <= weight
    growth: Rational,
    /// the dual nodes that contributes to this edge
    dual_nodes: Vec<DualNodeWeak>,
    /// the speed of growth
    grow_rate: Rational,
    /// grandson nodes: must be syndrome node
    grandson_dual_nodes: Vec<DualNodeWeak>,
    /// deduplicate edge in a boundary
    dedup_timestamp: (FastClearTimestamp, FastClearTimestamp),
    /// for fast clear
    pub timestamp: FastClearTimestamp,
}

pub type EdgePtr = ArcRwLock<Edge>;
pub type EdgeWeak = WeakRwLock<Edge>;

impl std::fmt::Debug for EdgePtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let edge = self.read_recursive();
        write!(
            f,
            "[edge: {}]: weight: {}, grow_rate: {}, growth: {}\n\tdual_nodes: {:?}",
            edge.edge_index, edge.weight, edge.grow_rate, edge.growth, edge.dual_nodes
        )
    }
}

impl std::fmt::Debug for EdgeWeak {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let edge_ptr = self.upgrade_force();
        let edge = edge_ptr.read_recursive();
        write!(
            f,
            "[edge: {}]: weight: {}, grow_rate: {}, growth: {}\n\tdual_nodes: {:?}",
            edge.edge_index, edge.weight, edge.grow_rate, edge.growth, edge.dual_nodes
        )
    }
}

///////////////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////////////

/// internal information of the dual node, added to the [`DualNode`]
#[derive(Derivative)]
#[derivative(Debug)]
pub struct DualNodeInternal {
    /// the pointer to the origin [`DualNode`]
    pub origin: DualNodeWeak,
    /// local index, to find myself in [`DualModuleSerial::nodes`]
    index: NodeIndex,
    /// dual variable of this node
    pub dual_variable: Weight,
    /// edges on the boundary of this node, (`is_left`, `edge`)
    pub boundary: Vec<(bool, EdgeWeak)>,
    /// over-grown vertices on the boundary of this node, this is to solve a bug where all surrounding edges are fully grown
    /// so all edges are deleted from the boundary... this will lose track of the real boundary when shrinking back
    pub overgrown_stack: Vec<(VertexWeak, Weight)>,
    /// helps to prevent duplicate visit in a single cycle
    last_visit_cycle: usize,
}

// when using feature `dangerous_pointer`, it doesn't provide the `upgrade()` function, so we have to fall back to the safe solution
pub type DualNodeInternalPtr = ArcRwLock<DualNodeInternal>;
pub type DualNodeInternalWeak = WeakRwLock<DualNodeInternal>;

impl std::fmt::Debug for DualNodeInternalPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let dual_node_internal = self.read_recursive();
        write!(f, "{}", dual_node_internal.index)
    }
}

impl std::fmt::Debug for DualNodeInternalWeak {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.upgrade_force().fmt(f)
    }
}

////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////////////

impl DualModuleImpl for DualModuleSerial {
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
                    mirror_unit: None,
                    propagated_dual_node: None,
                    propagated_grandson_dual_node: None,
                    timestamp: 0,
                })
            })
            .collect();
        // set edges
        let mut edges = Vec::<EdgePtr>::new();
        for hyperedge in initializer.weighted_edges.iter() {
            let edge_ptr = EdgePtr::new_value(Edge {
                edge_index: edges.len() as EdgeIndex,
                growth: Rational::zero(),
                weight: Rational::from_usize(hyperedge.weight).unwrap(),
                dual_nodes: vec![],
                vertices: hyperedge
                    .vertices
                    .iter()
                    .map(|i| vertices[*i as usize].downgrade())
                    .collect::<Vec<_>>(),
                grow_rate: Rational::zero(),
                grandson_dual_nodes: vec![],
                dedup_timestamp: (0, 0),
                timestamp: 0,
            });
            for &vertex_index in hyperedge.vertices.iter() {
                vertices[vertex_index as usize].write().edges.push(edge_ptr.downgrade());
            }
            edges.push(edge_ptr);
        }
        Self {
            vertices,
            edges,
            active_edges: BTreeSet::new(),
            active_nodes: BTreeSet::new(),
            sync_requests: vec![],
            edge_dedup_timestamp: 0,
            updated_boundary: vec![],
            propagating_vertices: vec![],
            active_timestamp: 0,
            current_cycle: 0,
            nodes: vec![],
        }
    }

    /// clear all growth and existing dual nodes
    fn clear(&mut self) {
        self.active_edges.clear();
        self.active_nodes.clear();
        for vertex_ptr in self.vertices.iter() {
            vertex_ptr.write().clear();
        }
        for edge_ptr in self.edges.iter() {
            edge_ptr.write().clear();
        }
    }

    fn add_defect_node(&mut self, dual_node_ptr: &DualNodePtr) {
        let dual_node = dual_node_ptr.read_recursive();
        debug_assert!(dual_node.invalid_subgraph.edges.is_empty());
        debug_assert!(
            dual_node.invalid_subgraph.vertices.len() == 1,
            "defect node (without edges) should only work on a single vertex, for simplicity"
        );
        let vertex_index = dual_node.invalid_subgraph.vertices.iter().next().unwrap();
        let mut vertex = self.vertices[*vertex_index].write();
        assert!(!vertex.is_defect, "defect should not be added twice");
        vertex.is_defect = true;
        drop(dual_node);
        drop(vertex);
        self.add_dual_node(dual_node_ptr);
    }

    #[allow(clippy::unnecessary_cast)]
    fn add_dual_node(&mut self, dual_node_ptr: &DualNodePtr) {
        // make sure the active edges are set
        let dual_node_weak = dual_node_ptr.downgrade();
        let dual_node = dual_node_ptr.read_recursive();
        for &edge_index in dual_node.invalid_subgraph.hair.iter() {
            let mut edge = self.edges[edge_index as usize].write();
            edge.grow_rate += &dual_node.grow_rate;
            edge.dual_nodes.push(dual_node_weak.clone());
            if edge.grow_rate.is_zero() {
                self.active_edges.remove(&edge_index);
            } else {
                self.active_edges.insert(edge_index);
            }
        }
        self.active_nodes.insert(dual_node_ptr.clone());
    }

    #[allow(clippy::unnecessary_cast)]
    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        let mut dual_node = dual_node_ptr.write();
        let grow_rate_diff = grow_rate.clone() - &dual_node.grow_rate;
        dual_node.grow_rate = grow_rate;
        drop(dual_node);
        let dual_node = dual_node_ptr.read_recursive();
        for &edge_index in dual_node.invalid_subgraph.hair.iter() {
            let mut edge = self.edges[edge_index as usize].write();
            edge.grow_rate += &grow_rate_diff;
            if edge.grow_rate.is_zero() {
                self.active_edges.remove(&edge_index);
            } else {
                self.active_edges.insert(edge_index);
            }
        }
        if dual_node.grow_rate.is_zero() {
            self.active_nodes.remove(dual_node_ptr);
        } else {
            self.active_nodes.insert(dual_node_ptr.clone());
        }
    }

    #[allow(clippy::collapsible_else_if, clippy::unnecessary_cast)]
    fn compute_maximum_update_length_dual_node(
        &mut self,
        dual_node_ptr: &DualNodePtr,
        simultaneous_update: bool,
    ) -> MaxUpdateLength {
        let node = dual_node_ptr.read_recursive();
        let mut max_update_length = MaxUpdateLength::new();
        for &edge_index in node.invalid_subgraph.hair.iter() {
            let edge = self.edges[edge_index as usize].read_recursive();
            let mut grow_rate = Rational::zero();
            if simultaneous_update {
                // consider all dual nodes
                for node_weak in edge.dual_nodes.iter() {
                    grow_rate += node_weak.upgrade_force().read_recursive().grow_rate.clone();
                }
            } else {
                grow_rate = node.grow_rate.clone();
            }
            if grow_rate.is_positive() {
                let edge_remain = edge.weight.clone() - edge.growth.clone();
                if edge_remain.is_zero() {
                    max_update_length.merge(MaxUpdateLength::Conflicting(edge_index));
                } else {
                    max_update_length.merge(MaxUpdateLength::ValidGrow(edge_remain / grow_rate));
                }
            } else if grow_rate.is_negative() {
                if edge.growth.is_zero() {
                    if node.grow_rate.is_negative() {
                        max_update_length.merge(MaxUpdateLength::ShrinkProhibited(dual_node_ptr.clone()));
                    } else {
                        // find a negatively growing edge
                        let mut found = false;
                        for node_weak in edge.dual_nodes.iter() {
                            let node_ptr = node_weak.upgrade_force();
                            if node_ptr.read_recursive().grow_rate.is_negative() {
                                max_update_length.merge(MaxUpdateLength::ShrinkProhibited(node_ptr));
                                found = true;
                                break;
                            }
                        }
                        assert!(found, "unreachable");
                    }
                } else {
                    max_update_length.merge(MaxUpdateLength::ValidGrow(-edge.growth.clone() / grow_rate));
                }
            }
        }
        max_update_length
    }

    #[allow(clippy::unnecessary_cast)]
    fn compute_maximum_update_length(&mut self) -> GroupMaxUpdateLength {
        // added by yl
        
        let mut group_max_update_length = GroupMaxUpdateLength::new();
        for &edge_index in self.active_edges.iter() {
            let edge = self.edges[edge_index as usize].read_recursive();
            let mut grow_rate = Rational::zero();
            for node_weak in edge.dual_nodes.iter() {
                let node_ptr = node_weak.upgrade_force();
                let node = node_ptr.read_recursive();
                grow_rate += node.grow_rate.clone();
            }
            if grow_rate.is_positive() {
                let edge_remain = edge.weight.clone() - edge.growth.clone();
                if edge_remain.is_zero() {
                    group_max_update_length.add(MaxUpdateLength::Conflicting(edge_index));
                } else {
                    group_max_update_length.add(MaxUpdateLength::ValidGrow(edge_remain / grow_rate));
                }
            } else if grow_rate.is_negative() {
                if edge.growth.is_zero() {
                    // it will be reported when iterating active dual nodes
                } else {
                    group_max_update_length.add(MaxUpdateLength::ValidGrow(-edge.growth.clone() / grow_rate));
                }
            }
        }
        for node_ptr in self.active_nodes.iter() {
            let node = node_ptr.read_recursive();
            if node.grow_rate.is_negative() {
                if node.get_dual_variable().is_positive() {
                    group_max_update_length
                        .add(MaxUpdateLength::ValidGrow(-node.get_dual_variable() / node.grow_rate.clone()));
                } else {
                    group_max_update_length.add(MaxUpdateLength::ShrinkProhibited(node_ptr.clone()));
                }
            }
        }
        group_max_update_length
    }

    #[allow(clippy::unnecessary_cast)]
    fn grow_dual_node(&mut self, dual_node_ptr: &DualNodePtr, length: Rational) {
        if length.is_zero() {
            eprintln!("[warning] calling `grow_dual_node` with zero length, nothing to do");
            return;
        }
        let node = dual_node_ptr.read_recursive();
        let grow_amount = length * node.grow_rate.clone();
        for &edge_index in node.invalid_subgraph.hair.iter() {
            let mut edge = self.edges[edge_index as usize].write();
            edge.growth += grow_amount.clone();
            assert!(
                !edge.growth.is_negative(),
                "edge {} over-shrunk: the new growth is {:?}",
                edge_index,
                edge.growth
            );
            assert!(
                edge.growth <= edge.weight,
                "edge {} over-grown: the new growth is {:?}, weight is {:?}",
                edge_index,
                edge.growth,
                edge.weight
            );
        }
        drop(node);
        // update dual variable
        let mut dual_node_ptr_write = dual_node_ptr.write();
        let dual_variable = dual_node_ptr_write.get_dual_variable();
        dual_node_ptr_write.set_dual_variable(dual_variable + grow_amount);
    }

    #[allow(clippy::unnecessary_cast)]
    fn grow(&mut self, length: Rational) {
        debug_assert!(
            length.is_positive(),
            "growth should be positive; if desired, please set grow rate to negative for shrinking"
        );
        // update the active edges
        for &edge_index in self.active_edges.iter() {
            let mut edge = self.edges[edge_index as usize].write();
            let mut grow_rate = Rational::zero();
            for node_weak in edge.dual_nodes.iter() {
                grow_rate += node_weak.upgrade_force().read_recursive().grow_rate.clone();
            }
            edge.growth += length.clone() * grow_rate;
            assert!(
                !edge.growth.is_negative(),
                "edge {} over-shrunk: the new growth is {:?}",
                edge_index,
                edge.growth
            );
            assert!(
                edge.growth <= edge.weight,
                "edge {} over-grown: the new growth is {:?}, weight is {:?}",
                edge_index,
                edge.growth,
                edge.weight
            );
        }
        // update dual variables
        for node_ptr in self.active_nodes.iter() {
            let mut node = node_ptr.write();
            let grow_rate = node.grow_rate.clone();
            let dual_variable = node.get_dual_variable();
            node.set_dual_variable(dual_variable + length.clone() * grow_rate);
        }
    }

    #[allow(clippy::unnecessary_cast)]
    fn get_edge_nodes(&self, edge_index: EdgeIndex) -> Vec<DualNodePtr> {
        self.edges[edge_index as usize]
            .read_recursive()
            .dual_nodes
            .iter()
            .map(|x| x.upgrade_force())
            .collect()
    }

    fn get_edge_slack(&self, edge_index: EdgeIndex) -> Rational {
        let edge = self.edges[edge_index].read_recursive();
        edge.weight.clone() - edge.growth.clone()
    }

    #[allow(clippy::unnecessary_cast)]
    fn is_edge_tight(&self, edge_index: EdgeIndex) -> bool {
        let edge = self.edges[edge_index as usize].read_recursive();
        edge.growth == edge.weight
    }

    #[allow(clippy::unnecessary_cast)]
    fn new_partitioned(partitioned_initializer: &PartitionedSolverInitializer) -> Self {
        // create vertices
        let mut vertices: Vec<VertexPtr> = partitioned_initializer
            .owning_range
            .iter()
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    mirror_unit: partitioned_initializer.owning_interface.clone(),
                    edges: Vec::new(),
                    propagated_dual_node: None,
                    propagated_grandson_dual_node: None,
                    timestamp: 0,
                })
            })
            .collect();
        // add interface vertices
        let mut mirrored_vertices = HashMap::<VertexIndex, VertexIndex>::new(); // all mirrored vertices mapping to their local indices
        for (mirror_unit, interface_vertices) in partitioned_initializer.interfaces.iter() {
            for vertex_index in interface_vertices.iter() {
                mirrored_vertices.insert(*vertex_index, vertices.len() as VertexIndex);
                vertices.push(VertexPtr::new_value(Vertex {
                    vertex_index: *vertex_index,
                    is_defect: false,
                    mirror_unit: Some(mirror_unit.clone()),
                    edges: Vec::new(),
                    propagated_dual_node: None,
                    propagated_grandson_dual_node: None,
                    timestamp: 0,
                }))
            }
        }
        // set edges
        let mut edges = Vec::<EdgePtr>::new();
        for (edge_index, hyper_edge) in partitioned_initializer.weighted_edges.iter().enumerate() {
            // sanity check, turn off for performance, added by yl
            for i in 0..hyper_edge.vertices.len() {
                for j in i+1..hyper_edge.vertices.len() {
                    assert_ne!(hyper_edge.vertices[i], hyper_edge.vertices[j], "invalid edge connecting 2 same vertex {}", hyper_edge.vertices[i]);
                }
            }
            assert!(hyper_edge.weight >= 0, "edge ({}) is negative-weighted", edge_index);
            // calculate the vertex index in partition
            let mut partitioned_vertex_indicies = Vec::new();
            let mut verticies_in_partition = Vec::new();
            for vertex_index in hyper_edge.vertices.iter() {
                debug_assert!(
                    partitioned_initializer.owning_range.contains(vertex_index.clone()) || mirrored_vertices.contains_key(vertex_index),
                    "edge ({}) connected to an invalid vertex {}", edge_index, vertex_index
                );
                let vertex_index_in_partition = if partitioned_initializer.owning_range.contains(vertex_index.clone()) {
                    vertex_index - partitioned_initializer.owning_range.start()
                } else {
                    mirrored_vertices[vertex_index]
                };
                partitioned_vertex_indicies.push(vertex_index_in_partition);
                verticies_in_partition.push(vertices[vertex_index_in_partition].downgrade())
            }
            // define new edge_ptr
            let edge_ptr = EdgePtr::new_value(Edge {
                edge_index: edge_index as EdgeIndex,
                weight: Rational::from_usize(hyper_edge.weight).unwrap(),
                vertices: verticies_in_partition,
                growth: Rational::zero(),
                dual_nodes: vec![],
                grow_rate: Rational::zero(),
                grandson_dual_nodes: vec![],
                dedup_timestamp: (0, 0),
                timestamp: 0,
            });
            for &vertex_index in hyper_edge.vertices.iter() {
                vertices[vertex_index as usize].write().edges.push(edge_ptr.downgrade());
            }
            edges.push(edge_ptr);
        }
        Self {
            vertices,
            edges,
            active_edges: BTreeSet::new(),
            active_nodes: BTreeSet::new(),
            current_cycle: 0,
            sync_requests: vec![],
            updated_boundary: vec![],
            active_timestamp: 0,
            edge_dedup_timestamp: 0,
            propagating_vertices: vec![],
            nodes: vec![],
        }
    }

    // // prepare the growing or shrinking state of all nodes and return a list of sync requests in case of mirrored vertices are changed
    // fn prepare_all(&mut self) -> &mut Vec<SyncRequest> {
    //     debug_assert!(
    //         self.sync_requests.is_empty(),
    //         "make sure to remove all sync requests before prepare to avoid out-dated requests"
    //     );
    //     self.renew_active_list();
    //     for i in 0..self.active_list.len() {
    //         let dual_node_ptr = {
    //             if let Some(internal_dual_node_ptr) = self.active_list[i].upgrade() {
    //                 let dual_node_internal = internal_dual_node_ptr.read_recursive();
    //                 dual_node_internal.origin.upgrade_force()
    //             } else {
    //                 continue; // a blossom could be in the active list even after it's been removed
    //             }
    //         };
    //         let dual_node = dual_node_ptr.read_recursive();
    //         match dual_node.grow_state {
    //             DualNodeGrowState::Grow => {}
    //             DualNodeGrowState::Shrink => {
    //                 self.prepare_dual_node_growth(&dual_node_ptr, false);
    //             }
    //             DualNodeGrowState::Stay => {} // do not touch, Stay nodes might have become a part of a blossom, so it's not safe to change the boundary
    //         };
    //     }
    //     for i in 0..self.active_list.len() {
    //         let dual_node_ptr = {
    //             if let Some(internal_dual_node_ptr) = self.active_list[i].upgrade() {
    //                 let dual_node_internal = internal_dual_node_ptr.read_recursive();
    //                 dual_node_internal.origin.upgrade_force()
    //             } else {
    //                 continue; // a blossom could be in the active list even after it's been removed
    //             }
    //         };
    //         let dual_node = dual_node_ptr.read_recursive();
    //         match dual_node.grow_state {
    //             DualNodeGrowState::Grow => {
    //                 self.prepare_dual_node_growth(&dual_node_ptr, true);
    //             }
    //             DualNodeGrowState::Shrink => {}
    //             DualNodeGrowState::Stay => {} // do not touch, Stay nodes might have become a part of a blossom, so it's not safe to change the boundary
    //         };
    //     }
    //     &mut self.sync_requests
    // }

}

/*
Implementing fast clear operations
*/

impl Edge {
    fn clear(&mut self) {
        self.growth = Rational::zero();
        self.dual_nodes.clear();
    }
}

impl Vertex {
    fn clear(&mut self) {
        self.is_defect = false;
    }
}

//////////////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////////////
impl DualModuleSerial {
    /// hard clear all growth (manual call not recommended due to performance drawback)
    pub fn hard_clear_graph(&mut self) {
        for edge in self.edges.iter() {
            let mut edge = edge.ptr().write();
            edge.clear();
            edge.timestamp = 0;
        }
        for vertex in self.vertices.iter() {
            let mut vertex = vertex.ptr().write();
            vertex.clear();
            vertex.timestamp = 0;
        }
        self.active_timestamp = 0;
    }

    /// soft clear all growth
    pub fn clear_graph(&mut self) {
        if self.active_timestamp == FastClearTimestamp::MAX {
            // rarely happens
            self.hard_clear_graph();
        }
        self.active_timestamp += 1; // implicitly clear all edges growth
    }
    
    /// necessary for boundary deduplicate when the unit is partitioned
    fn hard_clear_edge_dedup(&mut self) {
        for edge in self.edges.iter() {
            let mut edge = edge.ptr().write();
            edge.dedup_timestamp = (0, 0);
        }
        self.edge_dedup_timestamp = 0;
    }

    fn clear_edge_dedup(&mut self) {
        if self.edge_dedup_timestamp == FastClearTimestamp::MAX {
            // rarely happens
            self.hard_clear_edge_dedup();
        }
        self.edge_dedup_timestamp += 1; // implicitly clear all edges growth
    }

    // /// increment the global cycle so that each node in the active list can be accessed exactly once
    // #[allow(clippy::unnecessary_cast)]
    // fn renew_active_list(&mut self) {
    //     if self.current_cycle == usize::MAX {
    //         for i in 0..self.nodes.len() {
    //             let internal_dual_node_ptr = {
    //                 match self.nodes[i].as_ref() {
    //                     Some(internal_dual_node_ptr) => internal_dual_node_ptr.clone(),
    //                     _ => continue,
    //                 }
    //             };
    //             let mut internal_dual_node = internal_dual_node_ptr.write();
    //             internal_dual_node.last_visit_cycle = 0;
    //         }
    //         self.current_cycle = 0;
    //     }
    //     self.current_cycle += 1;
    //     // renew the active_list
    //     let mut updated_active_list = Vec::with_capacity(self.active_list.len());
    //     for i in 0..self.active_list.len() {
    //         let (dual_node_ptr, internal_dual_node_ptr) = {
    //             match self.active_list[i].upgrade() {
    //                 Some(internal_dual_node_ptr) => {
    //                     let mut dual_node_internal = internal_dual_node_ptr.write();
    //                     if self.nodes[dual_node_internal.index as usize].is_none() {
    //                         continue;
    //                     } // removed
    //                     if dual_node_internal.last_visit_cycle == self.current_cycle {
    //                         continue;
    //                     } // visited
    //                     dual_node_internal.last_visit_cycle = self.current_cycle; // mark as visited
    //                     (dual_node_internal.origin.upgrade_force(), internal_dual_node_ptr.clone())
    //                 }
    //                 _ => continue,
    //             }
    //         };
    //         let dual_node = dual_node_ptr.read_recursive();
    //         match dual_node.grow_state {
    //             DualNodeGrowState::Grow | DualNodeGrowState::Shrink => {
    //                 updated_active_list.push(internal_dual_node_ptr.downgrade());
    //             }
    //             DualNodeGrowState::Stay => {} // no longer in the active list
    //         };
    //     }
    //     self.active_list = updated_active_list;
    // }


}




//////////////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////////////




/*
Implementing visualization functions
*/

impl MWPSVisualizer for DualModuleSerial {
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
            let unexplored = edge.weight.clone() - edge.growth.clone();
            edges.push(json!({
                if abbrev { "w" } else { "weight" }: edge.weight.to_f64(),
                if abbrev { "v" } else { "vertices" }: edge.vertices.iter().map(|x| x.upgrade_force().read_recursive().vertex_index).collect::<Vec<_>>(),
                if abbrev { "g" } else { "growth" }: edge.growth.to_f64(),
                "gn": edge.growth.numer().to_i64(),
                "gd": edge.growth.denom().to_i64(),
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
    fn dual_module_serial_basics_1() {
        // cargo test dual_module_serial_basics_1 -- --nocapture
        let visualize_filename = "dual_module_serial_basics_1.json".to_string();
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
        let mut dual_module = DualModuleSerial::new_empty(&model_graph.initializer);
        // try to work on a simple syndrome
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, vec![3, 12]);
        let interface_ptr = DualModuleInterfacePtr::new_load(decoding_graph, &mut dual_module);
        visualizer
            .snapshot_combined("syndrome".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        // grow them each by half
        let dual_node_3_ptr = interface_ptr.read_recursive().nodes[0].clone();
        let dual_node_12_ptr = interface_ptr.read_recursive().nodes[1].clone();
        dual_module.grow_dual_node(&dual_node_3_ptr, Rational::from_usize(weight / 2).unwrap());
        dual_module.grow_dual_node(&dual_node_12_ptr, Rational::from_usize(weight / 2).unwrap());
        visualizer
            .snapshot_combined("grow".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        // cluster becomes solved
        dual_module.grow_dual_node(&dual_node_3_ptr, Rational::from_usize(weight / 2).unwrap());
        dual_module.grow_dual_node(&dual_node_12_ptr, Rational::from_usize(weight / 2).unwrap());
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
    fn dual_module_serial_basics_2() {
        // cargo test dual_module_serial_basics_2 -- --nocapture
        let visualize_filename = "dual_module_serial_basics_2.json".to_string();
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
        let mut dual_module = DualModuleSerial::new_empty(&model_graph.initializer);
        // try to work on a simple syndrome
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, vec![23, 24, 29, 30]);
        let interface_ptr = DualModuleInterfacePtr::new_load(decoding_graph, &mut dual_module);
        visualizer
            .snapshot_combined("syndrome".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        // grow them each by half
        let dual_node_23_ptr = interface_ptr.read_recursive().nodes[0].clone();
        let dual_node_24_ptr = interface_ptr.read_recursive().nodes[1].clone();
        let dual_node_29_ptr = interface_ptr.read_recursive().nodes[2].clone();
        let dual_node_30_ptr = interface_ptr.read_recursive().nodes[3].clone();
        dual_module.grow_dual_node(&dual_node_23_ptr, Rational::from_usize(weight / 4).unwrap());
        dual_module.grow_dual_node(&dual_node_24_ptr, Rational::from_usize(weight / 4).unwrap());
        dual_module.grow_dual_node(&dual_node_29_ptr, Rational::from_usize(weight / 4).unwrap());
        dual_module.grow_dual_node(&dual_node_30_ptr, Rational::from_usize(weight / 4).unwrap());
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
    fn dual_module_serial_basics_3() {
        // cargo test dual_module_serial_basics_3 -- --nocapture
        let visualize_filename = "dual_module_serial_basics_3.json".to_string();
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
        let mut dual_module = DualModuleSerial::new_empty(&model_graph.initializer);
        // try to work on a simple syndrome
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, vec![17, 23, 29, 30]);
        let interface_ptr = DualModuleInterfacePtr::new_load(decoding_graph, &mut dual_module);
        visualizer
            .snapshot_combined("syndrome".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        // grow them each by half
        let dual_node_17_ptr = interface_ptr.read_recursive().nodes[0].clone();
        let dual_node_23_ptr = interface_ptr.read_recursive().nodes[1].clone();
        let dual_node_29_ptr = interface_ptr.read_recursive().nodes[2].clone();
        let dual_node_30_ptr = interface_ptr.read_recursive().nodes[3].clone();
        dual_module.grow_dual_node(&dual_node_17_ptr, Rational::from_i64(160).unwrap());
        dual_module.grow_dual_node(&dual_node_23_ptr, Rational::from_i64(160).unwrap());
        dual_module.grow_dual_node(&dual_node_29_ptr, Rational::from_i64(160).unwrap());
        dual_module.grow_dual_node(&dual_node_30_ptr, Rational::from_i64(160).unwrap());
        visualizer
            .snapshot_combined("grow".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        // create cluster
        interface_ptr.create_node_vec(&[24], &mut dual_module);
        let dual_node_cluster_ptr = interface_ptr.read_recursive().nodes[4].clone();
        dual_module.grow_dual_node(&dual_node_17_ptr, Rational::from_i64(160).unwrap());
        dual_module.grow_dual_node(&dual_node_cluster_ptr, Rational::from_i64(160).unwrap());
        visualizer
            .snapshot_combined("grow".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        // create bigger cluster
        interface_ptr.create_node_vec(&[18, 23, 24, 31], &mut dual_module);
        let dual_node_bigger_cluster_ptr = interface_ptr.read_recursive().nodes[5].clone();
        dual_module.grow_dual_node(&dual_node_bigger_cluster_ptr, Rational::from_i64(120).unwrap());
        visualizer
            .snapshot_combined("solved".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        // the result subgraph
        let subgraph = vec![82, 24];
        visualizer
            .snapshot_combined("subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph])
            .unwrap();
    }

    #[test]
    fn dual_module_serial_find_valid_subgraph_1() {
        // cargo test dual_module_serial_find_valid_subgraph_1 -- --nocapture
        let visualize_filename = "dual_module_serial_find_valid_subgraph_1.json".to_string();
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
        let mut dual_module = DualModuleSerial::new_empty(&model_graph.initializer);
        // try to work on a simple syndrome
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, vec![3, 12]);
        let interface_ptr = DualModuleInterfacePtr::new_load(decoding_graph.clone(), &mut dual_module);
        visualizer
            .snapshot_combined("syndrome".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        // invalid clusters
        assert!(!decoding_graph.is_valid_cluster_auto_vertices(&vec![20].into_iter().collect()));
        assert!(!decoding_graph.is_valid_cluster_auto_vertices(&vec![9, 20].into_iter().collect()));
        assert!(!decoding_graph.is_valid_cluster_auto_vertices(&vec![15].into_iter().collect()));
        assert!(decoding_graph.is_valid_cluster_auto_vertices(&vec![15, 20].into_iter().collect()));
        // the result subgraph
        let subgraph = decoding_graph
            .find_valid_subgraph_auto_vertices(&vec![9, 15, 20, 21].into_iter().collect())
            .unwrap();
        visualizer
            .snapshot_combined("subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph])
            .unwrap();
    }
}
