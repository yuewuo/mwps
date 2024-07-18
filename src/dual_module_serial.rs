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
use std::collections::{BTreeSet, HashMap};
use weak_table::PtrWeakKeyHashMap;
use std::cmp::Ordering;
use core::hash::{Hash, Hasher};

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
    /// the number of all vertices (including those partitioned into other serial module)
    pub vertex_num: VertexNum, 
    /// the number of all edges (including those partitioned into other seiral module)
    pub edge_num: usize,
    /// vertices exclusively owned by this module, useful when partitioning the decoding graph into multiple [`DualModuleSerial`]
    pub owning_range: VertexRange,
    /// temporary variable to reduce reallocation
    updated_boundary: BTreeSet<EdgeIndex>,
    /// temporary variable to reduce reallocation
    propagating_vertices: Vec<VertexWeak>,
    /// temporary list of synchronize requests, i.e. those propagating into the mirrored vertices; should always be empty when not partitioned, i.e. serial version
    pub sync_requests: Vec<SyncRequest>,
    /// module information when used as a component in the partitioned dual module
    pub unit_module_info: Option<UnitModuleInfo>,
    /// nodes internal information
    pub nodes: Vec<Option<DualNodePtr>>,
    /// current nodes length, to enable constant-time clear operation
    pub nodes_length: usize,
}

/// records information only available when used as a unit in the partitioned dual module
#[derive(Derivative)]
#[derivative(Debug)]
pub struct UnitModuleInfo {
    /// unit index
    pub unit_index: usize,
    /// all mirrored vertices (excluding owned ones) to query if this module contains the vertex
    pub mirrored_vertices: HashMap<VertexIndex, VertexIndex>,
    /// owned dual nodes range
    pub owning_dual_range: NodeRange,
    /// hash table for mapping [`DualNodePtr`] to internal [`DualNodeInternalPtr`]
    pub dual_node_pointers: PtrWeakKeyHashMap<DualNodeWeak, usize>,
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
    /// (added by yl) whether a vertex is in the boundary vertices, since boundary vertices are not "owned" by any partition and should be 
    /// shared/mirroed between adjacent partitions   
    pub is_boundary: bool,
    /// propagated dual node
    pub propagated_dual_node: Option<DualNodeWeak>,
    /// if it's a mirrored vertex (present on multiple units), then this is the parallel unit that exclusively owns it
    pub mirror_unit: Option<PartitionUnitWeak>,
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
    pub edge_index: EdgeIndex,
    /// total weight of this edge
    pub weight: Rational,
    #[derivative(Debug = "ignore")]
    pub vertices: Vec<VertexWeak>,
    /// growth value, growth <= weight
    pub growth: Rational,
    /// the dual nodes that contributes to this edge
    pub dual_nodes: Vec<DualNodeWeak>,
    /// the speed of growth
    pub grow_rate: Rational,
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
                    is_boundary: false,
                    propagated_dual_node: None,
                    mirror_unit: None,
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
            vertex_num: initializer.vertex_num,
            edge_num: initializer.weighted_edges.len(),
            owning_range: VertexRange::new(0, initializer.vertex_num),
            updated_boundary: BTreeSet::new(),
            propagating_vertices: vec![],
            sync_requests: vec![],
            unit_module_info: None,
            nodes: vec![],
            nodes_length: 0,
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

    fn add_defect_node(&mut self, dual_node_ptr: &DualNodePtr, bias: usize) {
        let dual_node = dual_node_ptr.read_recursive();
        debug_assert!(dual_node.invalid_subgraph.edges.is_empty());
        debug_assert!(
            dual_node.invalid_subgraph.vertices.len() == 1,
            "defect node (without edges) should only work on a single vertex, for simplicity"
        );
        let vertex_index = self.
        get_vertex_index(*dual_node.invalid_subgraph.vertices.iter().next().unwrap()).
        expect("syndrome not belonging to this dual module");
        
        // for vertex0 in dual_node.invalid_subgraph.vertices.iter() {
        //     println!("dual node invalid subgraph vertices: {vertex0:?}");
        // }
        // println!("vertex_index to be accessed {vertex_index:?}");
        // println!("self.vertices len {}", self.vertices.len());
        // for vertex00 in self.vertices.iter() {
        //     println!("vertex index in self.vertices {}", vertex00.read().vertex_index);
        // }
        let mut vertex = self.vertices[vertex_index].write();
        assert!(!vertex.is_defect, "defect should not be added twice");
        vertex.is_defect = true;
        drop(dual_node);
        drop(vertex);
        self.add_dual_node(dual_node_ptr);
    }

    #[allow(clippy::unnecessary_cast)]
    fn add_dual_node(&mut self, dual_node_ptr: &DualNodePtr) {
        self.register_dual_node_ptr(dual_node_ptr); // increase owning_dual_range

        // make sure the active edges are set
        let dual_node_weak = dual_node_ptr.downgrade();
        let dual_node = dual_node_ptr.read_recursive();
        // println!("this dual node index {}", dual_node_ptr.read_recursive().index);
        // println!("edges len : {}", self.edges.len());
        // for &edge_index in dual_node.invalid_subgraph.hair.iter() {
        //     println!("edge index in this invalid subgraph: {edge_index:?}");
        // }

        // for edge00 in self.edges.iter() {
        //     println!("edge index in self.edges {}", edge00.read().edge_index);
        // }

        // let edge_offset = self.edges[0].read().edge_index;
        // println!("edge_offset: {edge_offset:?}");
        for &edge_index in dual_node.invalid_subgraph.hair.iter() {
            // println!("edge_index {}", edge_index);
            // if edge_index - edge_offset >= self.edges.len() {
            //     // println!("edge_offset {}", edge_offset);
            //     // println!("edges len {}", self.edges.len());
            //     continue;
            // }
            let mut edge = self.edges[edge_index].write();
            edge.grow_rate += &dual_node.grow_rate;
            edge.dual_nodes.push(dual_node_weak.clone());
            if edge.grow_rate.is_zero() {
                self.active_edges.remove(&edge_index);
            } else {
                self.active_edges.insert(edge_index);
            }
        }
        self.active_nodes.insert(dual_node_ptr.clone());
        self.nodes_length += 1;
        if self.nodes.len() < self.nodes_length {
            self.nodes.push(None);
        }
        self.nodes[self.nodes_length - 1] = Some(dual_node_ptr.clone());
    }

    #[allow(clippy::unnecessary_cast)]
    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        let mut dual_node = dual_node_ptr.write();
        let grow_rate_diff = grow_rate.clone() - &dual_node.grow_rate;
        dual_node.grow_rate = grow_rate;
        drop(dual_node);
        let dual_node = dual_node_ptr.read_recursive();
        // let edge_offset = self.edges[0].read().edge_index;
        for &edge_index in dual_node.invalid_subgraph.hair.iter() {
            // if edge_index - edge_offset >= self.edges.len() {
            //     // println!("edge_offset {}", edge_offset);
            //     // println!("edges len {}", self.edges.len());
            //     continue;
            // }
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
        if !simultaneous_update {
            self.prepare_dual_node_growth_single(dual_node_ptr);
        }

        let node = dual_node_ptr.read_recursive();
        let mut max_update_length = MaxUpdateLength::new();
        // let edge_offset = self.edges[0].read().edge_index;
        // println!("edge_offset: {}", edge_offset);
        for &edge_index in node.invalid_subgraph.hair.iter() {
            // if edge_index >= self.edges.len() {
            //     continue;
            // }
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
        // generate sync request 
        // self.generate_sync_request();
        // self.prepare_all();


        let mut group_max_update_length = GroupMaxUpdateLength::new();
        // let edge_offset = self.edges[0].read().edge_index;
        // println!("edge_offset in compute max update length: {}", edge_offset);
        for &edge_index in self.active_edges.iter() {
            // if edge_index >= self.edges.len() {
            //     continue;
            // }
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
        println!("group max update length: {group_max_update_length:?}");
        group_max_update_length
    }

    #[allow(clippy::unnecessary_cast)]
    fn grow_dual_node(&mut self, dual_node_ptr: &DualNodePtr, length: Rational) {
        if length.is_zero() {
            eprintln!("[warning] calling `grow_dual_node` with zero length, nothing to do");
            return;
        }
        // self.prepare_dual_node_growth_single(dual_node_ptr);


        let node = dual_node_ptr.read_recursive();
        // println!("length: {}, grow_rate {}", length, node.grow_rate);
        let grow_amount = length * node.grow_rate.clone();
        // let edge_offset = self.edges[0].read().edge_index;
        for &edge_index in node.invalid_subgraph.hair.iter() {
            // if edge_index >= self.edges.len() {
            //     continue;
            // }
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
        // let edge_offset = self.edges[0].read().edge_index;
        for &edge_index in self.active_edges.iter() {
            // if edge_index >= self.edges.len() {
            //     continue;
            // }
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
        // let edge_offset = self.edges[0].read().edge_index;
        self.edges[edge_index as usize]
            .read_recursive()
            .dual_nodes
            .iter()
            .map(|x| x.upgrade_force())
            .collect()
    }

    fn get_edge_slack(&self, edge_index: EdgeIndex) -> Rational {
        // let edge_offset = self.edges[0].read().edge_index;
        
        // if edge_index - edge_offset >= self.edges.len() {
        //     continue;
        // }
        let edge = self.edges[edge_index as usize].read_recursive();
        edge.weight.clone() - edge.growth.clone()
    }

    #[allow(clippy::unnecessary_cast)]
    fn is_edge_tight(&self, edge_index: EdgeIndex) -> bool {
        // let edge_offset = self.edges[0].read().edge_index;
        let edge = self.edges[edge_index as usize].read_recursive();
        edge.growth == edge.weight
    }

    fn get_edge_global_index(&self, local_edge_index: EdgeIndex, _unit_index: usize) -> EdgeIndex {
        let edge = self.edges[local_edge_index as usize].read_recursive();
        edge.edge_index
    }

    /// to be called in dual_module_parallel.rs
    fn new_partitioned(partitioned_initializer: &PartitionedSolverInitializer) -> Self {
        // println!("///////////////////////////////////////////////////////////////////////////////");
        // println!("for new_partitioned: {partitioned_initializer:?}");
        // println!("///////////////////////////////////////////////////////////////////////////////");

        // create vertices 
        let mut vertices: Vec<VertexPtr> = partitioned_initializer.owning_range.iter().map(|vertex_index| {
            VertexPtr::new_value(Vertex {
                vertex_index,
                is_defect: false,
                edges: Vec::new(),
                is_boundary: false,
                propagated_dual_node: None,
                mirror_unit: partitioned_initializer.owning_interface.clone(),
            })
        }).collect();

        // now we want to add the boundary vertices into the vertices for this partition
        let mut total_boundary_vertices = HashMap::<VertexIndex, VertexIndex>::new(); // all boundary vertices mapping to the specific local partition index
        let mut mirrored_vertices = HashMap::<VertexIndex, VertexIndex>::new(); // all mirrored vertices mapping to their local indices
        // only the index_range matters here, the units of the adjacent partitions do not matter here
        for (index_range, (_adjacent_partition_1, _adjacent_partition_2)) in &partitioned_initializer.boundary_vertices {
            for vertex_index in index_range.range[0]..index_range.range[1] {
                if !partitioned_initializer.owning_range.contains(vertex_index) {
                    total_boundary_vertices.insert(vertex_index, vertices.len() as VertexIndex);
                    mirrored_vertices.insert(vertex_index, vertices.len() as VertexIndex);
                    vertices.push(VertexPtr::new_value(Vertex {
                        vertex_index: vertex_index,
                        is_defect: false,
                        edges: Vec::new(),
                        is_boundary: true,
                        propagated_dual_node: None,
                        mirror_unit: partitioned_initializer.owning_interface.clone(),
                    }))
                }else{
                    mirrored_vertices.insert(vertex_index, vertices.len() as VertexIndex);
                }
            }
        }

        // set edges 
        let mut edges = Vec::<EdgePtr>::new();
        for (hyper_edge, edge_index) in partitioned_initializer.weighted_edges.iter() {
            // above, we have created the vertices that follow its own numbering rule for the index
            // so we need to calculate the vertex indices of the hyper_edge to make it match the local index 
            // then, we can create EdgePtr 
            let mut local_hyper_edge_vertices = Vec::<WeakRwLock<Vertex>>::new();
            for vertex_index in hyper_edge.vertices.iter() {
                let local_index = if partitioned_initializer.owning_range.contains(*vertex_index) {
                    vertex_index - partitioned_initializer.owning_range.start()
                } else {
                    total_boundary_vertices[vertex_index]
                };
                local_hyper_edge_vertices.push(vertices[local_index].downgrade());
            }
            // now we create the edgeptr
            let edge_ptr = EdgePtr::new_value(Edge {
                edge_index: *edge_index,
                growth: Rational::zero(),
                weight: Rational::from_usize(hyper_edge.weight).unwrap(),
                dual_nodes: vec![],
                vertices: local_hyper_edge_vertices,
                grow_rate: Rational::zero(),
            });

            // we also need to update the vertices of this hyper_edge
            for vertex_index in hyper_edge.vertices.iter() {
                let local_index = if partitioned_initializer.owning_range.contains(*vertex_index) {
                    vertex_index - partitioned_initializer.owning_range.start()
                } else {
                    total_boundary_vertices[vertex_index]
                };
                vertices[local_index].write().edges.push(edge_ptr.downgrade());
            }
            // for &vertex_index in hyper_edge.vertices.iter() {
            //     vertices[vertex_index as usize].write().edges.push(edge_ptr.downgrade());
            // }
            edges.push(edge_ptr);

        }

        Self {
            vertices,
            edges,
            active_edges: BTreeSet::new(),
            active_nodes: BTreeSet::new(),
            vertex_num: partitioned_initializer.vertex_num,
            edge_num: partitioned_initializer.edge_num,
            owning_range: partitioned_initializer.owning_range,
            updated_boundary: BTreeSet::new(),
            propagating_vertices: vec![],
            sync_requests: vec![],
            unit_module_info: Some(UnitModuleInfo {
                unit_index: partitioned_initializer.unit_index,
                mirrored_vertices,
                owning_dual_range: VertexRange::new(0, 0),
                dual_node_pointers: PtrWeakKeyHashMap::<DualNodeWeak, usize>::new(),
            }),
            nodes: vec![],
            nodes_length: 0,
        }
    }

    fn bias_dual_node_index(&mut self, bias: NodeIndex) {
        self.unit_module_info.as_mut().unwrap().owning_dual_range.bias_by(bias);
    }

    fn contains_vertex(&self, vertex_index: VertexIndex) -> bool {
        self.get_vertex_index(vertex_index).is_some()
    }

    fn execute_sync_event(&mut self, sync_event: &SyncRequest) {
        // first check whether the vertex in the sync request is within the owning_range of the unit
        debug_assert!(self.contains_vertex(sync_event.vertex_index));

        let propagated_dual_node_ptr =
            sync_event
                .propagated_dual_node
                .as_ref()
                .map(|(dual_node_weak, dual_variable)| {
                    self.get_otherwise_add_dual_node(&dual_node_weak.upgrade_force(), dual_variable.clone())
                });

        let local_vertex_index = self
        .get_vertex_index(sync_event.vertex_index)
        .expect("cannot synchronize at a non-existing vertex");

        // let vertex_ptr = &self.vertices[local_vertex_index];
        // let mut vertex = vertex_ptr.write();

        self.add_dual_node(&propagated_dual_node_ptr.unwrap());
        
        // if vertex.propagated_dual_node == propagated_dual_node_ptr.as_ref().map(|x| x.downgrade()) {
        //     // actually this may happen: if the same vertex is propagated from two different units with the same distance
        //     // to the closest grandson, it may happen that sync event will conflict on the grandson...
        //     // this conflict doesn't matter anyway: any grandson is good, as long as they're consistent
        //     // assert_eq!(vertex.propagated_grandson_dual_node, propagated_grandson_dual_node_internal_ptr.as_ref().map(|x| x.downgrade()));
        //     println!("the same vertex is propagated from two different units with the same distance
        //     // to the closest grandson, it may happen that sync event will conflict on the grandson...
        //     // this conflict doesn't matter anyway: any grandson is good, as long as they're consistent");
        // } else {
        //     // conflict with existing value, action needed
        //     // first vacate the vertex, recovering dual node boundaries accordingly
        //     if let Some(dual_node_week) = vertex.propagated_dual_node.as_ref() {
        //         debug_assert!(!vertex.is_defect, "cannot vacate a syndrome vertex: it shouldn't happen that a syndrome vertex is updated in any partitioned unit");
        //         // let mut updated_boundary = BTreeSet::new();
        //         let dual_node_ptr = dual_node_week.upgrade_force();
        //         let mut dual_node = dual_node_ptr.write();
        //         // vertex.propagated_dual_node = None;
        //         // // iterate over the boundary to remove any edges associated with the vertex and also reset those edges
        //         // for &edge_index in dual_node.invalid_subgraph.hair.iter() {
        //         //     let edge_ptr = &self.edges[edge_index];
        //         //     let mut edge = edge_ptr.write();
        //         //     for connected_vertex_weak in edge.vertices.clone().iter() {
        //         //         let connected_vertex_ptr = connected_vertex_weak.upgrade_force();
        //         //         if &connected_vertex_ptr == vertex_ptr {
        //         //             edge.clear();
        //         //         } else {
        //         //             updated_boundary.insert(edge_index);
        //         //         }
        //         //     }
        //         // }

        //         // // iterate over the edges around the vertex to add edges to the boundary
        //         // for edge_week in vertex.edges.iter() {
        //         //     let edge_ptr = edge_week.upgrade_force();
        //         //     let mut edge = edge_ptr.write();
        //         //     for (vertex0_index, vertex0_weak) in edge.vertices.clone().iter().enumerate() {
        //         //         let vertex0 = vertex0_weak.upgrade_force();
        //         //         if &vertex0 == vertex_ptr {
        //         //             if vertex0_index < edge.dual_nodes.len() {
        //         //                 let dual_node0 = &edge.dual_nodes[vertex0_index];
        //         //                 // sanity check: if exists, must be the same
        //         //                 debug_assert!(dual_node0.upgrade_force() == dual_node_ptr);
        //         //                 // need to add to the boundary
        //         //                 edge.clear();
        //         //                 updated_boundary.insert(edge.edge_index);
        //         //             }
        //         //         }
        //         //     }
        //         // }
        //         // // update the hair of invalid subgraph 
        //         // let mut invalid_sub = dual_node.invalid_subgraph.write();
        //         // std::mem::swap(&mut updated_boundary, &mut dual_node.invalid_subgraph.hair);
        //     }
        //     // then update the vertex to the dual node
        //     if let Some(dual_node_ptr) = propagated_dual_node_ptr.as_ref() {
        //         vertex.propagated_dual_node = Some(dual_node_ptr.downgrade());
        //         let mut dual_node = dual_node_ptr.write();
        //         for edge_weak in vertex.edges.iter() {
        //             let edge_ptr = edge_weak.upgrade_force();
        //             let mut edge = edge_ptr.write();
        //             for (vertex0_index, vertex0_weak) in edge.vertices.clone().iter().enumerate() {
        //                 let vertex0 = vertex0_weak.upgrade_force();
        //                 if &vertex0 == vertex_ptr {
        //                     edge.dual_nodes.push(dual_node_ptr.downgrade());
        //                     // dual_node.invalid_subgraph.hair.insert(edge.edge_index);
        //                 }
        //             }
        //         }
        //         self.active_nodes.insert(dual_node_ptr.clone());
        //     }
        // }
    }

    fn prepare_all(&mut self) -> &mut Vec<SyncRequest> {
        // debug_assert!(
        //     self.sync_requests.is_empty(),
        //     "make sure to remove all sync requests before prepare to avoid out-dated requests"
        // );
        for node_ptr in self.active_nodes.clone().iter() {
            self.prepare_dual_node_growth_single(node_ptr);
        }
        &mut self.sync_requests
    }
}

impl DualModuleSerial {
    /// register a new dual node ptr, but not creating the internal dual node
    fn register_dual_node_ptr(&mut self, dual_node_ptr: &DualNodePtr) {
        // println!("unit {:?}, register_dual_node_ptr: {:?}", self.unit_module_info, dual_node_ptr);
        let node = dual_node_ptr.read_recursive();
        if let Some(unit_module_info) = self.unit_module_info.as_mut() {
            if unit_module_info.owning_dual_range.is_empty() {
                // set the range instead of inserting into the lookup table, to minimize table lookup
                unit_module_info.owning_dual_range = VertexRange::new(node.index, node.index);
            }
            if unit_module_info.owning_dual_range.end() == node.index
                && self.nodes_length == unit_module_info.owning_dual_range.len()
            {
                // it's able to append into the owning range, minimizing table lookup and thus better performance
                unit_module_info.owning_dual_range.append_by(1);
            } else {
                // will be inserted at this place
                unit_module_info
                    .dual_node_pointers
                    .insert(dual_node_ptr.clone(), self.nodes_length);
            }
        } else {
            debug_assert!(
                self.nodes_length as NodeIndex == node.index,
                "dual node must be created in a sequential manner: no missing or duplicating"
            );
        }
        // println!("unit {:?}, register_dual_node_ptr: {:?}", self.unit_module_info, dual_node_ptr);
    }

    /// get the local index of a vertex, thus has usize type
    #[allow(clippy::unnecessary_cast)]
    pub fn get_vertex_index(&self, vertex_index: VertexIndex) -> Option<usize> {
        if self.owning_range.contains(vertex_index) {
            return Some((vertex_index - self.owning_range.start()) as usize);
        }
        if let Some(unit_module_info) = self.unit_module_info.as_ref() {
            if let Some(index) = unit_module_info.mirrored_vertices.get(&vertex_index) {
                return Some(*index as usize);
            }
        }
        None
    }

    // /// get the local index of a edge, thus has usize type
    // #[allow(clippy::unnecessary_cast)]
    // pub fn get_vertex_index(&self, global_edge_index: EdgeIndex) -> Option<usize> {

    // }


    /// get the local node_index of a dual node, thus has usize type
    #[allow(clippy::unnecessary_cast)]
    pub fn get_dual_node_index(&self, dual_node_ptr: &DualNodePtr) -> Option<usize> {
        let dual_node = dual_node_ptr.read_recursive();
        if let Some(unit_module_info) = self.unit_module_info.as_ref() {
            if unit_module_info.owning_dual_range.contains(dual_node.index) {
                Some((dual_node.index - unit_module_info.owning_dual_range.start()) as usize)
            } else {
                // println!("from unit {:?}, dual_node: {}", self.unit_module_info, dual_node.index);
                unit_module_info.dual_node_pointers.get(dual_node_ptr).copied()
            }
        } else {
            Some(dual_node.index as usize)
        }
    }

    /// possibly add dual node only when sync_event is provided
    #[allow(clippy::unnecessary_cast)]
    pub fn get_otherwise_add_dual_node(
        &mut self,
        dual_node_ptr: &DualNodePtr,
        dual_variable: Rational,
    ) -> DualNodePtr {
        let dual_node_index = self.get_dual_node_index(dual_node_ptr).unwrap_or_else(|| {
            // add a new internal dual node corresponding to the dual_node_ptr
            self.register_dual_node_ptr(dual_node_ptr);
            let node_index = self.nodes_length as NodeIndex;
            let mut node = dual_node_ptr.write();
            node.set_dual_variable(dual_variable);
            self.active_nodes.insert(dual_node_ptr.clone());
            self.nodes_length += 1;
            if self.nodes.len() < self.nodes_length {
                self.nodes.push(None);
            }
            self.nodes[node_index] = Some(dual_node_ptr.clone());
            node_index as usize
        });
        let dual_node_internal_ptr = self.nodes[dual_node_index].as_ref().expect("internal dual node must exists");
        // debug_assert!(
        //     dual_node_ptr == &dual_node_internal_ptr.read_recursive().origin.upgrade_force(),
        //     "dual node and dual internal node must corresponds to each other"
        // );
        dual_node_internal_ptr.clone()
    }


    // /// adjust the boundary of each dual node to fit into the need of growing (`length` > 0) or shrinking (`length` < 0)
    // pub fn prepare_dual_node_growth(&mut self, dual_node_ptr: &DualNodePtr, is_grow: bool) {
    //     let mut need_another = self.prepare_dual_node_growth_single(dual_node_ptr, is_grow);
    //     while need_another {
    //         // when there are 0 weight edges, one may need to run multiple iterations to get it prepared in a proper state
    //         need_another = self.prepare_dual_node_growth_single(dual_node_ptr, is_grow);
    //     }
    // }

    /// this is equivalent to [`DualModuleSerial::prepare_dual_node_growth`] when there are no 0 weight edges, but when it encounters zero-weight edges, it will report `true`
    pub fn prepare_dual_node_growth_single(&mut self, dual_node_ptr: &DualNodePtr) {        
        let node = dual_node_ptr.read_recursive();
        let edge_offset = self.edges[0].read().edge_index;
        for &edge_index in node.invalid_subgraph.hair.iter() {
            if edge_index - edge_offset >= self.edges.len() {
                continue;
            }
            let edge_ptr = &self.edges[edge_index - edge_offset];
            let edge = edge_ptr.read_recursive();
            if edge.growth == edge.weight {
                // we need to propagate to a new node 
                for peer_vertex_weak in edge.vertices.iter() {
                    let peer_vertex_ptr = peer_vertex_weak.upgrade_force();
                    let mut peer_vertex = peer_vertex_ptr.write();

                    // if this peer_vertex is not within the invalid subgraph, we could grow into this vertex 
                    if !node.invalid_subgraph.vertices.contains(&peer_vertex.vertex_index) {
                        if peer_vertex.is_boundary {
                            // (not sure) virtual node is never propagated, so keep this edge in the boundary 
                            // self.updated_boundary.insert(edge_index);
                        } else {
                            // debug_assert!(peer_vertex.propagated_dual_node.is_none(), 
                            // "growing into another propagated vertex forbidden");
                            self.propagating_vertices.push(peer_vertex_weak.clone());
                            // drop(edge); // unlock read
                            // let edge = edge_ptr.write();
                            peer_vertex.propagated_dual_node = Some(dual_node_ptr.downgrade()); // this is useless, delete later
                        }
                    }
                }
            } 
        }
        drop(node); // unlock 

        for vertex_weak in self.propagating_vertices.iter() {
            let vertex_ptr = vertex_weak.upgrade_force();
            let mut vertex = vertex_ptr.write();

            //  add to the sync list 
            if let Some(mirror_unit_weak) = &vertex.mirror_unit {
                self.sync_requests.push(SyncRequest {
                    mirror_unit_weak: mirror_unit_weak.clone(),
                    vertex_index: vertex.vertex_index,
                    propagated_dual_node: vertex.propagated_dual_node.clone().map(|weak| {
                        let dual_node_ptr = weak.upgrade_force();
                        let dual_node = dual_node_ptr.read_recursive();
                        (
                            weak,
                            dual_node.get_dual_variable(),
                        )
                    }),
                })
            };

                // if vertex.propagated_dual_node.is_none() {
                //     vertex.propagated_dual_node = Some(dual_node_ptr.downgrade());
                    
                //     // add to the sync list 
                //     if let Some(mirror_unit_weak) = &vertex.mirror_unit {
                //         self.sync_requests.push(SyncRequest {
                //             mirror_unit_weak: mirror_unit_weak.clone(),
                //             vertex_index: vertex.vertex_index,
                //             propagated_dual_node: vertex.propagated_dual_node.clone().map(|weak| {
                //                 let dual_node_ptr = weak.upgrade_force();
                //                 let dual_node = dual_node_ptr.read_recursive();
                //                 (
                //                     weak,
                //                     dual_node.get_dual_variable(),
                //                 )
                //             }),
                //         })
                //     };

                // }

                // we do not need this, since we do not need to prepare the boundary for grow/shrink
                // let mut count_newly_propagated_edge = 0;
                // for &edge_weak in vertex.edges.iter() {
                //     let edge_ptr = edge_weak.upgrade_force();
                //     let edge = edge_ptr.read_recursive();
                //     if edge.dual_nodes.len() > 1 {
                //         count_newly_propagated_edge += edge.dual_nodes.len() - 1;
                //         self.updated_boundary.insert(edge_index);
                //         let mut edge = edge_ptr.write();
                //         if edge.weight == Rational::zero() {
                //             newly_propagated_edge_has_zero_weight = true;
                //         }
                //         for peer_vertex_ptr in edge.vertices.iter() {
                //             let peer_vertex = peer_vertex_ptr.upgrade_force().write();
                //             peer_vertex.propagated_dual_node = Some(dual_node_ptr.downgrade());
                //         }
                //     }
                // }
                // if count_newly_propagated_edge == 0 {
                //     let mut dual_node = dual_node_ptr.write();
                    // overgrown stack... not implemented
                // }
            
        } 
        // // update the boundary
        // let mut dual_node = dual_node_ptr.write();
        // std::mem::swap(&mut self.updated_boundary, &mut dual_node.invalid_subgraph.hair);
        // println!("{} boundary: {:?}", tree_node.boundary.len(), tree_node.boundary);
        // if self.unit_module_info.is_none() {
        //     debug_assert!(
        //         !dual_node_internal.boundary.is_empty(),
        //         "the boundary of a dual cluster is never empty"
        //     );
        // }
                                   
    }

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

/*
Implementing visualization functions
*/

impl MWPSVisualizer for DualModuleSerial {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        // println!("//////////////////////////////////////////////////////////////////");
        // println!("vertices MWPSVisualizer: ");
        // for vertex in self.vertices.iter() {
        //     println!("vertices {}, is defect {}", vertex.read().vertex_index, vertex.read().is_defect);
        // }
        // println!("//////////////////////////////////////////////////////////////////");

        let mut vertices: Vec<serde_json::Value> = (0..self.vertex_num).map(|_| serde_json::Value::Null).collect();

        for vertex_ptr in self.vertices.iter() {
            let vertex = vertex_ptr.read_recursive();
            // println!("snapshot vertex index {}", vertex.vertex_index);
            vertices[vertex.vertex_index as usize] = json!({
                if abbrev { "v" } else { "is_boundary" }: i32::from(vertex.is_boundary),
            });
            if self.owning_range.contains(vertex.vertex_index) {
                // otherwise I don't know whether it's syndrome or not
                // vertices[vertex.vertex_index as usize].as_object_mut().unwrap().insert(
                //     (if abbrev { "s" } else { "is_defect" }).to_string(),
                //     json!(i32::from(vertex.is_defect)),
                // );
                vertices[vertex.vertex_index as usize] = json!({
                    if abbrev { "s" } else { "is_defect" }: i32::from(vertex.is_defect),
                });
            }
            
            
            // // println!("in snapshot vertex_index {}", vertex.vertex_index);
            // vertices[vertex.vertex_index as usize] = json!({
            //     if abbrev { "s" } else { "is_defect" }: i32::from(vertex.is_defect),
            // });
               
            // vertices[vertex.vertex_index as usize].as_object_mut().unwrap().insert(
            //     (if abbrev { "s" } else { "is_defect" }).to_string(),
            //     json!(i32::from(vertex.is_defect)),
            // );
        }
        // let mut edges: Vec<serde_json::Value> = vec![];
        let mut edges: Vec<serde_json::Value> = (0..self.edge_num).map(|_| serde_json::Value::Null).collect();
        for edge_ptr in self.edges.iter() {
            let edge = edge_ptr.read_recursive();
            // println!("snapshot edge index {}", edge.edge_index);
            let unexplored = edge.weight.clone() - edge.growth.clone();
            // edges.push(json!({
            //     if abbrev { "w" } else { "weight" }: edge.weight.to_f64(),
            //     if abbrev { "v" } else { "vertices" }: edge.vertices.iter().map(|x| x.upgrade_force().read_recursive().vertex_index).collect::<Vec<_>>(),
            //     if abbrev { "g" } else { "growth" }: edge.growth.to_f64(),
            //     "gn": edge.growth.numer().to_i64(),
            //     "gd": edge.growth.denom().to_i64(),
            //     "un": unexplored.numer().to_i64(),
            //     "ud": unexplored.denom().to_i64(),
            // }));
            // println!("in snapshot edge_index {}", edge.edge_index);
            edges[edge.edge_index as usize] = json!({
                if abbrev { "w" } else { "weight" }: edge.weight.to_f64(),
                if abbrev { "v" } else { "vertices" }: edge.vertices.iter().map(|x| x.upgrade_force().read_recursive().vertex_index).collect::<Vec<_>>(),
                if abbrev { "g" } else { "growth" }: edge.growth.to_f64(),
                "gn": edge.growth.numer().to_i64(),
                "gd": edge.growth.denom().to_i64(),
                "un": unexplored.numer().to_i64(),
                "ud": unexplored.denom().to_i64(),
            });
        }
        // json!({
        //     "vertices": vertices,
        //     "edges": edges,
        // })
        let mut value = json!({
            "vertices": vertices,
            "edges": edges,
        });
        // TODO: since each serial module only processes a part of the dual nodes, it's not feasible to list them in a reasonable vector now...
        // update the visualizer to be able to join multiple dual nodes
        // if self.owning_range.start() == 0 && self.owning_range.end() == self.vertex_num {
        //     let mut dual_nodes = Vec::<serde_json::Value>::new();
        //     for node_index in 0..self.nodes_length {
        //         let node_ptr = &self.nodes[node_index];
        //         if let Some(node_ptr) = node_ptr.as_ref() {
        //             let node = node_ptr.read_recursive();
        //             dual_nodes.push(json!({
        //                 if abbrev { "b" } else { "boundary" }: node.boundary.iter().map(|(is_left, edge_weak)|
        //                     (*is_left, edge_weak.upgrade_force().read_recursive(active_timestamp).edge_index)).collect::<Vec<(bool, EdgeIndex)>>(),
        //                 if abbrev { "d" } else { "dual_variable" }: node.dual_variable,
        //             }));
        //         } else {
        //             dual_nodes.push(json!(null));
        //         }
        //     }
        //     value
        //         .as_object_mut()
        //         .unwrap()
        //         .insert("dual_nodes".to_string(), json!(dual_nodes));
        // }
        value
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
