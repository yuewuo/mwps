//! Serial Dual Parallel
//! 
//! A parallel implementation of the dual module, leveraging the serial version 
//! 
//! 
use super::model_hypergraph::ModelHyperGraph;
use super::dual_module::*;
use super::dual_module_serial::*;
use super::pointers::*;
use super::util::*;
use super::visualize::*;
use crate::rayon::prelude::*; // Rayon is a data-parallelism library that makes it easy to convert sequential computations into parallel.
use crate::serde_json;
use crate::weak_table::PtrWeakHashSet;
use itertools::partition;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::sync::{Arc, Weak};

pub struct DualModuleParallel<SerialModule: DualModuleImpl + Send + Sync> {
    /// the basic wrapped serial modules at the beginning, afterwards the fused units are appended after them
    pub units: Vec<ArcRwLock<DualModuleParallelUnit<SerialModule>>>,
    /// local configuration
    pub config: DualModuleParallelConfig,
    /// partition information generated by the config
    pub partition_info: Arc<PartitionInfo>,
    /// thread pool used to execute async functions in parallel
    pub thread_pool: Arc<rayon::ThreadPool>,
    /// an empty sync requests queue just to implement the trait
    pub empty_sync_request: Vec<SyncRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DualModuleParallelConfig {
    /// enable async execution of dual operations; only used when calling top-level operations, not used in individual units
    #[serde(default = "dual_module_parallel_default_configs::thread_pool_size")]
    pub thread_pool_size: usize,
    /// strategy of edges placement: if edges are placed in the fusion unit, it's good for software implementation because there are no duplicate
    /// edges and no unnecessary vertices in the descendant units. On the other hand, it's not very favorable if implemented on hardware: the
    /// fusion unit usually contains a very small amount of vertices and edges for the interfacing between two blocks, but maintaining this small graph
    /// may consume additional hardware resources and increase the decoding latency. I want the algorithm to finally work on the hardware efficiently
    /// so I need to verify that it does work by holding all the fusion unit's owned vertices and edges in the descendants, although usually duplicated.
    #[serde(default = "dual_module_parallel_default_configs::edges_in_fusion_unit")]
    pub edges_in_fusion_unit: bool,
    /// enable parallel execution of a fused dual module
    #[serde(default = "dual_module_parallel_default_configs::enable_parallel_execution")]
    pub enable_parallel_execution: bool,
}

impl Default for DualModuleParallelConfig {
    fn default() -> Self {
        serde_json::from_value(json!({})).unwrap()
    }
}

pub mod dual_module_parallel_default_configs {
    pub fn thread_pool_size() -> usize {
        0
    } // by default to the number of CPU cores
      // pub fn thread_pool_size() -> usize { 1 }  // debug: use a single core
    pub fn edges_in_fusion_unit() -> bool {
        true
    } // by default use the software-friendly approach because of removing duplicate edges
    pub fn enable_parallel_execution() -> bool {
        false
    } // by default disabled: parallel execution may cause too much context switch, yet not much speed benefit
}

pub struct DualModuleParallelUnit<SerialModule: DualModuleImpl + Send + Sync> {
    /// the index
    pub unit_index: usize,
    /// partition information generated by the config
    pub partition_info: Arc<PartitionInfo>,
    /// information shared with serial module
    pub partition_unit: PartitionUnitPtr,
    /// whether it's active or not; some units are "placeholder" units that are not active until they actually fuse their children
    pub is_active: bool,
    /// the vertex range of this parallel unit consists of all the owning_range of its descendants
    pub whole_range: VertexRange,
    /// the vertices owned by this unit, note that owning_range is a subset of whole_range
    pub owning_range: VertexRange,
    /// the vertices that are mirrored outside of whole_range, in order to propagate a vertex's sync event to every unit that mirrors it
    pub extra_descendant_mirrored_vertices: HashSet<VertexIndex>,
    /// the owned serial dual module
    pub serial_module: SerialModule,
    /// left and right children dual modules
    pub children: Option<(
        DualModuleParallelUnitWeak<SerialModule>,
        DualModuleParallelUnitWeak<SerialModule>,
    )>,
    /// parent dual module
    pub parent: Option<DualModuleParallelUnitWeak<SerialModule>>,
    /// elevated dual nodes: whose descendent not on the representative path of a dual node
    pub elevated_dual_nodes: PtrWeakHashSet<DualNodeWeak>,
    /// an empty sync requests queue just to implement the trait
    pub empty_sync_request: Vec<SyncRequest>,
    /// run things in thread pool
    pub enable_parallel_execution: bool,
    /// whether any descendant unit has active dual node
    pub has_active_node: bool,
}

pub type DualModuleParallelUnitPtr<SerialModule> = ArcRwLock<DualModuleParallelUnit<SerialModule>>;
pub type DualModuleParallelUnitWeak<SerialModule> = WeakRwLock<DualModuleParallelUnit<SerialModule>>;

impl<SerialModule: DualModuleImpl + Send + Sync> std::fmt::Debug for DualModuleParallelUnitPtr<SerialModule> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let unit = self.read_recursive();
        write!(f, "{}", unit.unit_index)
    }
}

impl<SerialModule: DualModuleImpl + Send + Sync> std::fmt::Debug for DualModuleParallelUnitWeak<SerialModule> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.upgrade_force().fmt(f)
    }
}

impl<SerialModule: DualModuleImpl + Send + Sync> DualModuleParallel<SerialModule> {
    /// recommended way to create a new instance, given a customized configuration
    #[allow(clippy::unnecessary_cast)]
    pub fn new_config(
        initializer: &SolverInitializer,
        partition_info: &PartitionInfo,
        config: DualModuleParallelConfig,
    ) -> Self {
        let partition_info = Arc::new(partition_info.clone());
        let mut thread_pool_builder = rayon::ThreadPoolBuilder::new();
        if config.thread_pool_size != 0 {
            thread_pool_builder = thread_pool_builder.num_threads(config.thread_pool_size);
        }
        let thread_pool = thread_pool_builder.build().expect("creating thread pool failed");
        let mut units = vec![];
        let unit_count = partition_info.units.len();
        let hyper_graph = ModelHyperGraph::new(Arc::new(initializer.clone())); // build the graph to construct the NN data structure
        let mut contained_vertices_vec: Vec<BTreeSet<VertexIndex>> = vec![]; // all vertices maintained by each unit
        // let mut is_vertex_virtual: Vec<_> = (0..initializer.vertex_num).map(|_| false).collect();
        // for virtual_vertex in initializer.virtual_vertices.iter() {
        //     is_vertex_virtual[*virtual_vertex as usize] = true;
        // }
        let partition_units: Vec<PartitionUnitPtr> = (0..unit_count)
            .map(|unit_index| {
                PartitionUnitPtr::new_value(PartitionUnit {
                    unit_index,
                    enabled: unit_index < partition_info.config.partitions.len(),
                })
            })
            .collect();
        let mut partitioned_initializers: Vec<PartitionedSolverInitializer> = (0..unit_count)
            .map(|unit_index| {
                let mut interfaces = vec![];
                let mut current_index = unit_index;
                let owning_range = &partition_info.units[unit_index].owning_range;
                let mut contained_vertices = BTreeSet::new();
                for vertex_index in owning_range.iter() {
                    contained_vertices.insert(vertex_index);
                }
                while let Some(parent_index) = &partition_info.units[current_index].parent {
                    let mut mirror_vertices = vec![];
                    if config.edges_in_fusion_unit {
                        // partition_info.units[*parent_index].owning_range is the boundary between partitions
                        for vertex_index in partition_info.units[*parent_index].owning_range.iter() {
                            let mut is_incident = false;
                            for peer_index in hyper_graph.vertices[vertex_index as usize].edges.iter() {
                                if owning_range.contains(*peer_index) {
                                    is_incident = true;
                                    break;
                                }
                            }
                            if is_incident {
                                mirror_vertices.push(vertex_index);
                                contained_vertices.insert(vertex_index);
                            }
                        }
                    } else {
                        // first check if there EXISTS any vertex that's adjacent of it's contains vertex
                        let mut has_incident = false;
                        for vertex_index in partition_info.units[*parent_index].owning_range.iter() {
                            for peer_index in hyper_graph.vertices[vertex_index as usize].edges.iter() {
                                if contained_vertices.contains(peer_index) {
                                    // important diff: as long as it has an edge with contained vertex, add it
                                    has_incident = true;
                                    break;
                                }
                            }
                            if has_incident {
                                break;
                            }
                        }
                        if has_incident {
                            // add all vertices as mirrored
                            for vertex_index in partition_info.units[*parent_index].owning_range.iter() {
                                mirror_vertices.push(vertex_index);
                                contained_vertices.insert(vertex_index);
                            }
                        }
                    }
                    if !mirror_vertices.is_empty() {
                        // only add non-empty mirrored parents is enough
                        interfaces.push((partition_units[*parent_index].downgrade(), mirror_vertices));
                    }
                    current_index = *parent_index;
                }
                contained_vertices_vec.push(contained_vertices);
                PartitionedSolverInitializer {
                    unit_index,
                    vertex_num: initializer.vertex_num,
                    edge_num: initializer.weighted_edges.len(),
                    owning_range: *owning_range,
                    owning_interface: if unit_index < partition_info.config.partitions.len() {
                        None
                    } else {
                        Some(partition_units[unit_index].downgrade())
                    },
                    weighted_edges: vec![], // to be filled later
                    interfaces,
                } // note that all fields can be modified later
            })
            .collect();
        // assign each edge to its unique partition
        for (edge_index, hyper_edge) in initializer.weighted_edges.iter().enumerate() {
            let mut ancestor_unit_index;
            let mut vertices_unit_indices = vec![];
            for vertex_index in hyper_edge.vertices.iter() {
                assert!(vertex_index.clone() < initializer.vertex_num, "hyperedge {edge_index} connected to an invalid vertex {vertex_index}");
                let vertex_unit_index = partition_info.vertex_to_owning_unit[vertex_index.clone()];
                vertices_unit_indices.push(vertex_unit_index);
            }

            for i in 0..vertices_unit_indices.len() {
                for j in 0..vertices_unit_indices.len() {
                    let i_unit_index = vertices_unit_indices[i];
                    let j_unit_index = vertices_unit_indices[j];
                    let is_i_ancestor = partition_info.units[i_unit_index].descendants.contains(&vertices_unit_indices[j]);
                    let is_j_ancestor = partition_info.units[j_unit_index].descendants.contains(&vertices_unit_indices[i]);

                    let anscestor_unit_index = if is_i_ancestor {i_unit_index} else {j_unit_index};
                    let descendant_unit_index: usize = if is_i_ancestor {j_unit_index} else {i_unit_index};

                    // it seems that this is always set to True
                    if config.edges_in_fusion_unit {
                        // the edge should be added to the descendant, and it's guaranteed that the descendant unit contains (although not necessarily owned) the vertex
                        partitioned_initializers[descendant_unit_index]
                            .weighted_edges
                            .push(hyper_edge.clone());
                    } 
                }
            }
        }
        println!("partitioned_initializers: {:?}", partitioned_initializers);
        thread_pool.scope(|_| {
            (0..unit_count)
                .into_par_iter()
                .map(|unit_index| {
                    // println!("unit_index: {unit_index}");
                    let dual_module = SerialModule::new_partitioned(&partitioned_initializers[unit_index]);
                    DualModuleParallelUnitPtr::new_wrapper(
                        dual_module,
                        unit_index,
                        Arc::clone(&partition_info),
                        partition_units[unit_index].clone(),
                        config.enable_parallel_execution,
                    )
                })
                .collect_into_vec(&mut units);
        });
        // fill in the children and parent references
        for unit_index in 0..unit_count {
            let mut unit = units[unit_index].write();
            if let Some((left_children_index, right_children_index)) = &partition_info.units[unit_index].children {
                unit.children = Some((
                    units[*left_children_index].downgrade(),
                    units[*right_children_index].downgrade(),
                ))
            }
            if let Some(parent_index) = &partition_info.units[unit_index].parent {
                unit.parent = Some(units[*parent_index].downgrade());
            }
        }
        // fill in the extra_descendant_mirrored_vertices, cache to store where the "event of growing out of its own partition" goes
        for unit_index in 0..unit_count {
            lock_write!(unit, units[unit_index]);
            let whole_range = &partition_info.units[unit_index].whole_range;
            let partitioned_initializer = &partitioned_initializers[unit_index];
            for (_, interface_vertices) in partitioned_initializer.interfaces.iter() {
                for vertex_index in interface_vertices.iter() {
                    if !whole_range.contains(*vertex_index) {
                        unit.extra_descendant_mirrored_vertices.insert(*vertex_index);
                    }
                }
            }
            if let Some((left_children_weak, right_children_weak)) = unit.children.clone() {
                for child_weak in [left_children_weak, right_children_weak] {
                    // note: although iterating over HashSet is not performance optimal, this only happens at initialization and thus it's fine
                    for vertex_index in child_weak
                        .upgrade_force()
                        .read_recursive()
                        .extra_descendant_mirrored_vertices
                        .iter()
                    {
                        if !whole_range.contains(*vertex_index) {
                            unit.extra_descendant_mirrored_vertices.insert(*vertex_index);
                        }
                    }
                }
            }
            // println!("{} extra_descendant_mirrored_vertices: {:?}", unit.unit_index, unit.extra_descendant_mirrored_vertices);
        }
        Self {
            units,
            config,
            partition_info,
            thread_pool: Arc::new(thread_pool),
            empty_sync_request: vec![],
        }
    }

    /// find the active ancestor to handle this dual node (should be unique, i.e. any time only one ancestor is active)
    #[inline(never)]
    pub fn find_active_ancestor(&self, dual_node_ptr: &DualNodePtr) -> DualModuleParallelUnitPtr<SerialModule> {
        self.find_active_ancestor_option(dual_node_ptr).unwrap()
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn find_active_ancestor_option(
        &self,
        dual_node_ptr: &DualNodePtr,
    ) -> Option<DualModuleParallelUnitPtr<SerialModule>> {
        // find the first active ancestor unit that should handle this dual node
        let representative_vertex = dual_node_ptr.get_representative_vertex();
        let owning_unit_index = self.partition_info.vertex_to_owning_unit[representative_vertex as usize];
        let mut owning_unit_ptr = self.units[owning_unit_index].clone();
        loop {
            let owning_unit = owning_unit_ptr.read_recursive();
            if owning_unit.is_active {
                break; // find an active unit
            }
            if let Some(parent_weak) = &owning_unit.parent {
                let parent_owning_unit_ptr = parent_weak.upgrade_force();
                drop(owning_unit);
                owning_unit_ptr = parent_owning_unit_ptr;
            } else {
                return None;
            }
        }
        Some(owning_unit_ptr)
    }

    /// statically fuse them all, may be called at any state (meaning each unit may not necessarily be solved locally)
    pub fn static_fuse_all(&mut self) {
        for unit_ptr in self.units.iter() {
            lock_write!(unit, unit_ptr);
            if let Some((left_child_weak, right_child_weak)) = &unit.children {
                {
                    // ignore already fused children and work on others
                    let left_child_ptr = left_child_weak.upgrade_force();
                    let right_child_ptr = right_child_weak.upgrade_force();
                    let left_child = left_child_ptr.read_recursive();
                    let right_child = right_child_ptr.read_recursive();
                    if !left_child.is_active && !right_child.is_active {
                        continue; // already fused, it's ok to just ignore
                    }
                    debug_assert!(
                        left_child.is_active && right_child.is_active,
                        "children must be active at the same time if fusing all together"
                    );
                }
                unit.static_fuse();
            }
        }
    }
}

