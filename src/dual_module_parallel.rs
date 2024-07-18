//! Serial Dual Parallel
//! 
//! A parallel implementation of the dual module, leveraging the serial version
//! 
//! 


#![cfg_attr(feature = "unsafe_pointer", allow(dropping_references))]
use super::dual_module::*;
use super::dual_module_serial::*;
use super::pointers::*;
use super::util::*;
use super::visualize::*;
use crate::rayon::prelude::*;
use crate::serde_json;
use crate::weak_table::PtrWeakHashSet;
use itertools::partition;
use petgraph::csr::Neighbors;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::hash::Hash;
use std::os::unix::thread;
use std::sync::{Arc, Weak};
use std::collections::VecDeque;
use crate::num_traits::sign::Signed;
use crate::num_traits::{ToPrimitive, Zero};
use petgraph::Graph;
use petgraph::Undirected;
use weak_table::PtrWeakKeyHashMap;


////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////
////////////For the big picture, define DualModuleParallel//////////////


pub struct DualModuleParallel<SerialModule: DualModuleImpl + Send + Sync> {
    /// the set of all DualModuleParallelUnits, one for each partition
    /// we set the read-write lock 
    pub units: Vec<ArcRwLock<DualModuleParallelUnit<SerialModule>>>,
    /// configuration such as thread_pool_size 
    pub config: DualModuleParallelConfig,
    /// partition information 
    pub partition_info: Arc<PartitionInfo>,
    /// thread pool used to execute async functions in parallel
    pub thread_pool: Arc<rayon::ThreadPool>,
    // /// an empty sync requests queue just to implement the trait
    // pub empty_sync_request: Vec<SyncRequest>,

    /// a dynamic (to-be-update) undirected graph (DAG) to keep track of the relationship between different partition units, assumed to be acylic if we partition
    /// along the time axis, but could be cyclic depending on the partition and fusion strategy
    pub dag_partition_units: BTreeSet<(usize, usize, bool)>, // (unit_index0, unit_index1, is_fused)
    /// partitioned initializers, used in both primal and dual parallel modules
    pub partitioned_initializers: Vec<PartitionedSolverInitializer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DualModuleParallelConfig {
    /// enable async execution of dual operations; only used when calling top-level operations, not used in individual units
    #[serde(default = "dual_module_parallel_default_configs::thread_pool_size")]
    pub thread_pool_size: usize,
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
    pub fn enable_parallel_execution() -> bool {
        false
    } // by default disabled: parallel execution may cause too much context switch, yet not much speed benefit
}

impl<SerialModule: DualModuleImpl + Send + Sync> DualModuleParallel<SerialModule> {
    /// create a new instance, specifically initialize for each DualModuleParallelUnit
    #[allow(clippy::unnecessary_cast)]
    pub fn new_config(
        initializer: &SolverInitializer,
        partition_info: &PartitionInfo, // contains the partition info of all partition units
        config: DualModuleParallelConfig,
    ) -> Self {
        // automatic reference counter for partition info
        let partition_info = Arc::new(partition_info.clone());

        // build thread pool 
        let mut thread_pool_builder = rayon::ThreadPoolBuilder::new();
        if config.thread_pool_size != 0 {
            thread_pool_builder = thread_pool_builder.num_threads(config.thread_pool_size);
        }
        let thread_pool = thread_pool_builder.build().expect("creating thread pool failed");

        // create partition_units
        let mut units = vec![];
        let unit_count = partition_info.units.len();
        let partition_units: Vec<PartitionUnitPtr> = (0..unit_count).map(|unit_index| {
            PartitionUnitPtr::new_value(PartitionUnit {
                unit_index,
            })
        }).collect();

        // build partition initializer
        let mut partitioned_initializers: Vec<PartitionedSolverInitializer> = (0..unit_count).map(|unit_index| {
            let unit_partition_info = &partition_info.units[unit_index];
            let owning_range = &unit_partition_info.owning_range;
            // let boundary_vertices = &unit_partition_info.boundary_vertices;

            PartitionedSolverInitializer {
                unit_index,
                vertex_num: initializer.vertex_num,
                edge_num: initializer.weighted_edges.len(),
                owning_range: *owning_range,
                weighted_edges: vec![],
                boundary_vertices: unit_partition_info.boundary_vertices.clone(),
                adjacent_partition_units: unit_partition_info.adjacent_partition_units.clone(),
                owning_interface: Some(partition_units[unit_index].downgrade()),
            }
        }).collect();

        // now we assign each edge to its unique partition
        // println!("edge num: {}", initializer.weighted_edges.len());
        let mut edge_bias_vec = [core::usize::MAX, unit_count];
        for (edge_index, hyper_edge) in initializer.weighted_edges.iter().enumerate() {
            let mut vertices_unit_indices = vec![];
            let mut boundary_vertices_adjacent_units_index = vec![];
            let mut exist_boundary_vertex = false;
            for vertex_index in hyper_edge.vertices.iter() {
                let adjacent_unit_indices = partition_info.boundary_vertex_to_adjacent_units.get(vertex_index);
                match adjacent_unit_indices {
                    Some(adjacent_unit_indices) => {
                        // it belongs to boundary vertices 
                        exist_boundary_vertex = true;
                        boundary_vertices_adjacent_units_index.push((vertex_index, adjacent_unit_indices));
                    },
                    None => {
                        // it does not belong to boundary vertices, instead it belongs to the non-boundary-interface region of owning_range
                        let vertex_unit_index = partition_info.vertex_to_owning_unit.get(vertex_index);
                        match vertex_unit_index {
                            Some(vertex_unit_index) => vertices_unit_indices.push((vertex_index, vertex_unit_index)),
                            None => assert!(!vertex_unit_index.is_none(), "partition unit owning range contains vertex {} but this vertex corresponds to None unit", vertex_index),
                        }
                    }
                }
            }

            // println!("hyper_edge index: {edge_index}");
            // println!("vertices_unit_indices: {vertices_unit_indices:?}");
            // println!("boundary vertices adjacent unit indices: {boundary_vertices_adjacent_units_index:?}");

            // if all vertices are the boundary vertices 
            if vertices_unit_indices.len() == 0 {
                // assume the boundary vertices are adjacent to exactly 2 partition units
                let adjacent_partition_1 = boundary_vertices_adjacent_units_index[0].1.0;
                let adjacent_partition_2 = boundary_vertices_adjacent_units_index[0].1.1;
                partitioned_initializers[adjacent_partition_1].weighted_edges.push((hyper_edge.clone(), edge_index));
                partitioned_initializers[adjacent_partition_2].weighted_edges.push((hyper_edge.clone(), edge_index));
                if edge_index < edge_bias_vec[adjacent_partition_1] {
                    edge_bias_vec[adjacent_partition_1] = edge_index;
                }
                if edge_index < edge_bias_vec[adjacent_partition_2] {
                    edge_bias_vec[adjacent_partition_2] = edge_index;
                }
            } else {
                let first_vertex_unit_index = *vertices_unit_indices[0].1;
                let all_vertex_from_same_unit = vertices_unit_indices.iter().all(|&item| *(item.1) == first_vertex_unit_index);
                if !exist_boundary_vertex {
                    // all within owning range of one unit 
                    // we assume that for vertices of a hyperedge, if there aren't any boundary vertices among them, they must belong to the same partition unit 
                    assert!(all_vertex_from_same_unit, "For the vertices of hyperedge {}, there does not exist boundary vertex but all the vertices do not belong to the same unit", edge_index);
                    // since all vertices this hyperedge connects to belong to the same unit, we can assign this hyperedge to that partition unit
                    partitioned_initializers[first_vertex_unit_index].weighted_edges.push((hyper_edge.clone(), edge_index));
                    if edge_index < edge_bias_vec[first_vertex_unit_index] {
                        edge_bias_vec[first_vertex_unit_index] = edge_index;
                    }
                } else {
                    // since we have assumed to partition along the time axis, there could only be 2 different units the vertices (excluding the boundary vertices) could be in
                    // if all vertices (excluding the boundary vertices) are from the same unit, we can assign this hyperedge to that partition unit
                    if all_vertex_from_same_unit {
                        partitioned_initializers[first_vertex_unit_index].weighted_edges.push((hyper_edge.clone(), edge_index));
                        if edge_index < edge_bias_vec[first_vertex_unit_index] {
                            edge_bias_vec[first_vertex_unit_index] = edge_index;
                        }
                    } else {
                        // println!("exist boundary vertices, vertices unit indices {vertices_unit_indices:?}");
                        // if the vertices of this hyperedge (excluding the boundary vertices) belong to 2 different partition unit
                        // sanity check: there really are only 2 unique partition units 
                        let mut sanity_check = HashSet::new();
                        for (_vertex_index, vertex_unit_index) in &vertices_unit_indices {
                            sanity_check.insert(vertex_unit_index);
                        }
                        assert!(sanity_check.len() == 2, "there are fewer than 2 or more than 2 partition units");
    
                        // we create new hyperedge with the boundary vertex + verticies exlusive for one partition unit
                        let mut vertices_for_partition_1 = vec![];
                        let mut vertices_for_partition_2 = vec![];
                        let mut unit_index_partition_1 = 0;
                        let mut unit_index_partition_2 = 0;
                        for (&vertex_index, &vertex_unit_index) in vertices_unit_indices {
                            if vertex_unit_index == first_vertex_unit_index {
                                unit_index_partition_1 = vertex_unit_index;
                                vertices_for_partition_1.push(vertex_index);
                            } else {
                                unit_index_partition_2 = vertex_unit_index;
                                vertices_for_partition_2.push(vertex_index);
                            }
                        }
                        println!("vertices for partition 1: {vertices_for_partition_1:?}");
                        // now we add the boundary vertices in 
                        for (&vertex_index, adjacent_units) in boundary_vertices_adjacent_units_index {
                            // sanity check, the adjacent partition units of the boundary vertices must match with unit_index_partition_1 and unit_index_partition_2
                            assert!((adjacent_units.0 == unit_index_partition_1 && adjacent_units.1 == unit_index_partition_2) || 
                            (adjacent_units.1 == unit_index_partition_1 && adjacent_units.0 == unit_index_partition_2), 
                                "this boundary vertex {} is adjacent to partition unit {} and {} that is not the partition units {} and {} in owning range", 
                                vertex_index, adjacent_units.0, adjacent_units.1, unit_index_partition_1, unit_index_partition_2);
                           
                            // for partition 1, we add in all the boundary vertices
                            vertices_for_partition_1.push(vertex_index);
                            // for partition 2, we add in all the boundary vertices
                            vertices_for_partition_2.push(vertex_index);
                        }
    
                        partitioned_initializers[unit_index_partition_1].weighted_edges.push(
                            (HyperEdge::new(vertices_for_partition_1, hyper_edge.weight), edge_index)
                        );
                        partitioned_initializers[unit_index_partition_2].weighted_edges.push(
                            (HyperEdge::new(vertices_for_partition_2, hyper_edge.weight), edge_index)
                        );
                        if edge_index < edge_bias_vec[unit_index_partition_1] {
                            edge_bias_vec[unit_index_partition_1] = edge_index;
                        }
                        if edge_index < edge_bias_vec[unit_index_partition_2] {
                            edge_bias_vec[unit_index_partition_2] = edge_index;
                        }
                    }
                }
            }
        }

        // now that we are done with assigning hyperedge to its unique partitions, we proceed to initialize DualModuleParallelUnit for every partition
        // print function for check during dev
        // println!("partitioned_initializers: {:?}", partitioned_initializers);
        thread_pool.scope(|_| {
            (0..unit_count)
                .into_par_iter()
                .map(|unit_index| {
                    // println!("unit_index: {unit_index}");
                    let dual_module = DualModuleSerial::new_partitioned(&partitioned_initializers[unit_index]);

                    // iterate through all the 



                    DualModuleParallelUnitPtr::new_value(DualModuleParallelUnit {
                        unit_index,
                        partition_info: Arc::clone(&partition_info),
                        partition_unit: partition_units[unit_index].clone(),
                        owning_range: partition_info.units[unit_index].owning_range,
                        serial_module: dual_module,
                        enable_parallel_execution: config.enable_parallel_execution,
                        elevated_dual_nodes: PtrWeakHashSet::new(),
                        adjacent_parallel_units: PtrWeakKeyHashMap::new(),
                        done_fused_with_all_adjacent_units: false,
                        vertex_bias: partition_info.units[unit_index].owning_range.range[0],
                        has_active_node: true, // set to true by default
                        involved_in_fusion: false,
                        owning_edge_range: IndexRange::new(
                            partitioned_initializers[unit_index].weighted_edges[0].1, 
                            partitioned_initializers[unit_index].weighted_edges.last().unwrap().1
                        ),
                        edge_bias: edge_bias_vec[unit_index],
                        empty_sync_request: vec![],
                    })
                  
                })
                .collect_into_vec(&mut units);
        });

        // we need to fill in the adjacent_parallel_units here 
        for unit_index in 0..unit_count {
            let mut unit = units[unit_index].write();
            for adjacent_unit_index in partition_info.units[unit_index].adjacent_partition_units.clone().into_iter() {
                unit.adjacent_parallel_units.insert(units[adjacent_unit_index].clone(), false);
            }
        }

        // now we are initializing dag_partition_units 
        let mut dag_partition_units = BTreeSet::new();
        let graph = &partition_info.config.dag_partition_units;
        for edge_index in graph.edge_indices() {
            let (source, target) = graph.edge_endpoints(edge_index).unwrap();
            dag_partition_units.insert((source.index(), target.index(), false));
        }
        
        Self {
            units,
            config,
            partition_info,
            thread_pool: Arc::new(thread_pool),
            dag_partition_units,
            partitioned_initializers,
        }
    }

    /// find the parallel unit that handles this dual node, should be unique
    pub fn find_handling_parallel_unit(&self, dual_node_ptr: &DualNodePtr) -> DualModuleParallelUnitPtr<SerialModule> {
        let defect_index = dual_node_ptr.get_representative_vertex();
        let owning_unit_index = self.partition_info.vertex_to_owning_unit.get(&defect_index);
        match owning_unit_index {
            Some(x) => {
                let owning_unit_ptr = self.units[*x].clone();
                // drop(binding);
                return owning_unit_ptr;
            },
            None => {
                let adjacent_unit_indices = self.partition_info.boundary_vertex_to_adjacent_units.get(&defect_index);
                match adjacent_unit_indices {
                Some(x) => {
                    // we let the 1st/smaller partition unit in the tuple takes in charge of this dual node
                    let owning_unit_ptr = self.units[x.0].clone();
                    // drop(binding);
                    return owning_unit_ptr;
                },
                None => {panic!("This dual node {} is not contained in any partition, we cannot find a parallel unit that handles this dual node.", defect_index);},
            }},
        }
    }

    // statically fuse all units 
    pub fn static_fuse_all(&mut self) {
        let unit_1_ptr = &self.units[0];
        let unit_2_ptr = &self.units[1];
        let mut unit_1 = unit_1_ptr.write();
        let mut unit_2 = unit_2_ptr.write();
        if let Some(unit_1_fused) = unit_1.adjacent_parallel_units.get_mut(&unit_2_ptr) {
            *unit_1_fused = true;
        }
        if let Some(unit_2_fused) = unit_2.adjacent_parallel_units.get_mut(&unit_1_ptr) {
            *unit_2_fused = true;
        }

        
        // for unit_ptr in self.units.iter() {
        //     let mut unit = unit_ptr.write();
        //     unit.adjacent_parallel_units.iter()
        // }
    }
}


// now we implement the DualModuleImpl trait for DualModuleParallel
impl<SerialModule: DualModuleImpl + Send + Sync> DualModuleImpl for DualModuleParallel<SerialModule> {
    /// create a new dual module with empty syndrome
    fn new_empty(initializer: &SolverInitializer) -> Self {
        Self::new_config(initializer, 
            &PartitionConfig::new(initializer.vertex_num).info(), 
            DualModuleParallelConfig::default(),)
    }

    /// clear all growth and existing dual nodes, prepared for the next decoding
    #[inline(never)]
    fn clear(&mut self) {
        self.thread_pool.scope(|_| {
            self.units.par_iter().enumerate().for_each(|(unit_index, unit_ptr)| {
                let mut unit = unit_ptr.write();
                unit.clear(); // to be implemented in DualModuleParallelUnit
                // unit.partition_unit.write().enabled = false; not sure whether we need it to enable/disable mirror vertices
                unit.elevated_dual_nodes.clear();

            })
        })
    }

    /// add defect node
    fn add_defect_node(&mut self, dual_node_ptr: &DualNodePtr, bias: usize) {
        let unit_ptr = self.find_handling_parallel_unit(dual_node_ptr);
        self.thread_pool.scope(|_| {
            let mut unit = unit_ptr.write();
            unit.add_defect_node(dual_node_ptr, 0); // to be implemented in DualModuleParallelUnit
        })
    }

    /// add corresponding dual node, note that the `internal_vertices` and `hair_edges` are not set
    fn add_dual_node(&mut self, dual_node_ptr: &DualNodePtr) {
        let unit_ptr = self.find_handling_parallel_unit(dual_node_ptr);
        self.thread_pool.scope(|_| {
            let mut unit = unit_ptr.write();
            unit.add_dual_node(dual_node_ptr); // to be implemented in DualModuleParallelUnit
        })
    }

    /// update grow rate
    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        let unit_ptr = self.find_handling_parallel_unit(dual_node_ptr);
        self.thread_pool.scope(|_| {
            let mut unit = unit_ptr.write();
            unit.set_grow_rate(dual_node_ptr, grow_rate); // to be implemented in DualModuleParallelUnit
        })
    }

    /// An optional function that helps to break down the implementation of [`DualModuleImpl::compute_maximum_update_length`]
    /// check the maximum length to grow (shrink) specific dual node, if length is 0, give the reason of why it cannot further grow (shrink).
    /// if `simultaneous_update` is true, also check for the peer node according to [`DualNode::grow_state`].
    fn compute_maximum_update_length_dual_node(
        &mut self,
        dual_node_ptr: &DualNodePtr,
        simultaneous_update: bool,
    ) -> MaxUpdateLength {
        let unit_ptr = self.find_handling_parallel_unit(dual_node_ptr);
        self.thread_pool.scope(|_| {
            let mut unit = unit_ptr.write();
            unit.compute_maximum_update_length_dual_node(dual_node_ptr, simultaneous_update) // to be implemented in DualModuleParallelUnit
        })
    }

    /// check the maximum length to grow (shrink) for all nodes, return a list of conflicting reason and a single number indicating the maximum rate to grow:
    /// this number will be 0 if any conflicting reason presents
    fn compute_maximum_update_length(&mut self) -> GroupMaxUpdateLength {
        // self.execute_sync_event(sync_event);
        println!("compute max");
        self.thread_pool.scope(|_| {
            let results: Vec<_> = self
                .units
                .par_iter()
                .filter_map(|unit_ptr| {
                    let mut unit = unit_ptr.write();
                    Some(unit.compute_maximum_update_length())
                })
                .collect();
            let mut group_max_update_length = GroupMaxUpdateLength::new();
            for local_group_max_update_length in results.into_iter() {
                group_max_update_length.extend(local_group_max_update_length); 
            }
            group_max_update_length
        })
    }

    /// An optional function that can manipulate individual dual node, not necessarily supported by all implementations
    fn grow_dual_node(&mut self, dual_node_ptr: &DualNodePtr, length: Rational) {
        let unit_ptr = self.find_handling_parallel_unit(dual_node_ptr);
        self.thread_pool.scope(|_| {
            let mut unit = unit_ptr.write();
            unit.grow_dual_node(dual_node_ptr, length) // to be implemented in DualModuleParallelUnit
        })
    }

    /// add if condition to check whether this cluster I want to grow is within this unit
    /// grow a specific length globally, length must be positive.
    /// note that a negative growth should be implemented by reversing the speed of each dual node
    fn grow(&mut self, length: Rational) {
        println!("inside grow!");
        self.thread_pool.scope(|_| {
            self.units.par_iter().for_each(|unit_ptr| {
                let mut unit = unit_ptr.write();
                unit.grow(length.clone()); // to be implemented in DualModuleParallelUnit
            });
        })
    }

    fn get_edge_nodes(&self, edge_index: EdgeIndex) -> Vec<DualNodePtr> {
        for unit_ptr in self.units.iter() {
            let unit = unit_ptr.read_recursive();
            if unit.owning_edge_range.contains(edge_index) {
                return unit.get_edge_nodes(edge_index);
            }
        }
        println!("Error: none of the units contain the edge_index {} for function get_edge_nodes", edge_index);
        return vec![]; // it should never reach here
    }

    fn get_edge_slack(&self, edge_index: EdgeIndex) -> Rational {
        for unit_ptr in self.units.iter() {
            let unit = unit_ptr.read_recursive();
            if unit.owning_edge_range.contains(edge_index) {
                return unit.get_edge_slack(edge_index);
            }
        }
        println!("Error: none of the units contain the edge_index {} for function get_edge_slack", edge_index);
        return Rational::zero(); // it should never reach here
    }

    fn is_edge_tight(&self, edge_index: EdgeIndex) -> bool {
        for unit_ptr in self.units.iter() {
            let unit = unit_ptr.read_recursive();
            if unit.owning_edge_range.contains(edge_index) {
                return unit.is_edge_tight(edge_index);
            }
        }
        println!("Error: none of the units contain the edge_index {} for function is_edge_tight", edge_index);
        return false; // it should never reach here
    }

    fn get_edge_global_index(&self, local_edge_index: EdgeIndex, unit_index: usize) -> EdgeIndex {
        self.units[unit_index].read_recursive().get_edge_global_index(local_edge_index, unit_index)
        // panic!("unsupported, please call this method in DualModuleParallelUnit");
    }
}

// now we implement the DualModuleParallelImpl trait for DualModuleParallel
impl<SerialModule: DualModuleImpl + Send + Sync> DualModuleParallelImpl for DualModuleParallel<SerialModule> {
    type UnitType = DualModuleParallelUnit<SerialModule>;

    fn get_unit(&self, unit_index: usize) -> ArcRwLock<Self::UnitType> {
        self.units[unit_index].clone()
    }
}

// now we implement the visualization functions
impl<SerialModule: DualModuleImpl + MWPSVisualizer + Send + Sync> MWPSVisualizer for DualModuleParallel<SerialModule> {
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        // do the sanity check first before taking snapshot
        // self.sanity_check().unwrap();
        let mut value = json!({});
        for unit_ptr in self.units.iter() {
            let unit = unit_ptr.read_recursive();
            let value_2 = unit.snapshot(abbrev);
            // println!("value in unit {}: {}", unit.unit_index, value_2);
            // snapshot_fix_missing_fields(&mut value_2, abbrev);
            // let value = value.as_object_mut().expect("snapshot must be an object");
            // let value_2 = value_2.as_object_mut().expect("snapshot must be an object");
            // snapshot_copy_remaining_fields(value, value_2);
            snapshot_combine_values(&mut value, value_2, abbrev);
            // snapshot_append_values(&mut value, value_2, abbrev);
            // println!("\n\n");
            // println!("after combine: {}", value);
        }
        value
    }
}


////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////
////////////For Each partition, define DualModuleParallelUnit///////////
/// it is in the methods of DualModuleParallelUnit that we can implement 
/// fusion between 2 DualModuleInterfacePtr (namely, the dual nodes that belonged 
/// to 2 units)

pub struct DualModuleParallelUnit<SerialModule: DualModuleImpl + Send + Sync> {
    /// the unit index, this should be the same as the partition index I suppose
    pub unit_index: usize,
    /// partition information 
    pub partition_info: Arc<PartitionInfo>,
    /// information shared with serial module
    pub partition_unit: PartitionUnitPtr,
    /// the vertices owned by this unit
    pub owning_range: VertexRange,
    /// the edge owned by this unit
    pub owning_edge_range: EdgeRange,
    /// the specific serial module belonged to this partition unit
    pub serial_module: DualModuleSerial,
    /// hmmmmm i dont know, it keeps track of which partition unit(s) the dual nodes grow into? 
    /// or those that are not on the representative path of a dual node.
    /// PtrWeakHashSet: A hash set with weak elements, hashed on element pointer.
    pub elevated_dual_nodes: PtrWeakHashSet<DualNodeWeak>,
    /// run things in thread pool
    pub enable_parallel_execution: bool,
    /// prev, remember the dag of partition unit? 
    /// adjacent DualModuleParallelUnitWeak according to the dag of partition unit
    /// maybe we need to keep a fusion plan dag and a dynamic dag for the already fused units
    /// (Pointer to a parallel unit, whether_this_unit_has_been_fused_with_self)
    pub adjacent_parallel_units: PtrWeakKeyHashMap<DualModuleParallelUnitWeak<SerialModule>, bool>,
    /// (tentative) whether this unit has fused with all its adjacent units
    pub done_fused_with_all_adjacent_units: bool,
    /// whether this unit has ever been fused with other units
    pub involved_in_fusion: bool,
    /// the amount the vertices in this unit is off-set (biased) by, assuming all the vertex index in this unit is continuous
    pub vertex_bias: usize,
    /// the amount the vertices in this unit is off-set (biased) by, assuming all the vertex index in this unit is continuous
    pub edge_bias: usize,
    /// whether any descendant unit has active dual node
    pub has_active_node: bool,    
    /// an empty sync requests queue just to implement the trait
    pub empty_sync_request: Vec<SyncRequest>,
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

impl<SerialModule: DualModuleImpl + Send + Sync> DualModuleParallelUnitPtr<SerialModule> {
    pub fn fuse(
        &mut self, 
        self_interface: &DualModuleInterfacePtr, 
        other_interface: &DualModuleInterfacePtr, 
        other_dual_unit: &DualModuleParallelUnitPtr<SerialModule>
    ) {

        // change the index of dual nodes in the other interface
        
        let mut dual_unit = self.write();
        if let Some(is_fused) = dual_unit.adjacent_parallel_units.get_mut(other_dual_unit) {
            *is_fused = true;
        }     

        // fuse dual unit
        // self.fuse_helper(other_dual_unit);
        // if let Some(is_fused) = self.adjacent_parallel_units.get_mut(other_dual_unit) {
        //     *is_fused = true;
        // }        
        println!("fuse asdf");
        // now we fuse the interface (copying the interface of other to myself)
        self_interface.fuse(other_interface);
    }
}

impl<SerialModule: DualModuleImpl + Send + Sync> DualModuleParallelUnit<SerialModule> {
    pub fn fuse_helper(&mut self,         
        other_dual_unit: &DualModuleParallelUnitPtr<SerialModule>
    ) {
        if let Some(is_fused) = self.adjacent_parallel_units.get_mut(other_dual_unit) {
            *is_fused = true;
        }        
    }
    
    // pub fn fuse(
    //     &mut self, 
    //     self_interface: &DualModuleInterfacePtr, 
    //     other_interface: &DualModuleInterfacePtr, 
    //     other_dual_unit: &DualModuleParallelUnitPtr<SerialModule>
    // ) {

    //     // change the index of dual nodes in the other interface
        

    //     // fuse dual unit
    //     self.fuse_helper(other_dual_unit);
    //     // if let Some(is_fused) = self.adjacent_parallel_units.get_mut(other_dual_unit) {
    //     //     *is_fused = true;
    //     // }        
    //     println!("fuse asdf");
    //     // now we fuse the interface (copying the interface of other to myself)
    //     self_interface.fuse(other_interface);
    // }

    /// dfs to add defect node
    fn dfs_add_defect_node(&mut self, dual_node_ptr: &DualNodePtr, defect_vertex: VertexIndex, visited: &mut HashSet<usize>) {

        if self.owning_range.contains(defect_vertex) {
            // println!("the unit containing this dual node is {} with owning range {} to {}", self.unit_index, self.owning_range.range[0], self.owning_range.range[1]);
            self.serial_module.add_defect_node(dual_node_ptr, self.owning_range.range[0]);
            return;
        }

        visited.insert(self.unit_index);

        for (neighbor, _) in self.adjacent_parallel_units.iter() {
            if !visited.contains(&neighbor.read_recursive().unit_index) {
                neighbor.write().dfs_add_defect_node(dual_node_ptr, defect_vertex, visited);
            }
        }
    }

    fn dfs_add_dual_node(&mut self, dual_node_ptr: &DualNodePtr, defect_vertex: VertexIndex, visited: &mut HashSet<usize>) {
        if self.owning_range.contains(defect_vertex) {
            // println!("the unit containing this dual node is {} with owning range {} to {}, with defect_vertex {}", self.unit_index, self.owning_range.range[0], self.owning_range.range[1], defect_vertex);
            self.serial_module.add_dual_node(dual_node_ptr);
            return;
        }

        visited.insert(self.unit_index);

        for (neighbor, _) in self.adjacent_parallel_units.iter() {
            if !visited.contains(&neighbor.read_recursive().unit_index) {
                neighbor.write().dfs_add_dual_node(dual_node_ptr, defect_vertex, visited);
            }
        }
    }

    /// dfs to add defect node
    fn dfs_grow_dual_node(&mut self, dual_node_ptr: &DualNodePtr, length: Rational, defect_vertex: VertexIndex, visited: &mut HashSet<usize>) {

        if self.owning_range.contains(defect_vertex) {
            // println!("the unit containing this dual node is {} with owning range {} to {}", self.unit_index, self.owning_range.range[0], self.owning_range.range[1]);
            self.serial_module.grow_dual_node(dual_node_ptr, length);
            return;
        }

        visited.insert(self.unit_index);

        // println!("neighbor len: {}", self.adjacent_parallel_units.len());
        for (neighbor, _) in self.adjacent_parallel_units.iter() {
            if !visited.contains(&neighbor.read_recursive().unit_index) {
                neighbor.write().dfs_grow_dual_node(dual_node_ptr, length.clone(), defect_vertex, visited);
            }
        }
    }

    fn dfs_set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational, defect_vertex: VertexIndex, visited: &mut HashSet<usize>) {
        if self.owning_range.contains(defect_vertex) {
            // println!("the unit containing this dual node is {} with owning range {} to {}", self.unit_index, self.owning_range.range[0], self.owning_range.range[1]);
            self.serial_module.set_grow_rate(dual_node_ptr, grow_rate);
            return;
        }

        visited.insert(self.unit_index);

        // println!("neighbor len: {}", self.adjacent_parallel_units.len());
        for (neighbor, _) in self.adjacent_parallel_units.iter() {
            if !visited.contains(&neighbor.read_recursive().unit_index) {
                neighbor.write().dfs_set_grow_rate(dual_node_ptr, grow_rate.clone(), defect_vertex, visited);
            }
        }
    }

    fn bfs_compute_maximum_update_length(&mut self, group_max_update_length: &mut GroupMaxUpdateLength) {
        // early terminate if no active dual nodes anywhere in the descendant
        // we know that has_active_node is set to true by default
        // if !self.has_active_node {
        //     return;
        // }
        println!("hihi");

        let serial_module_group_max_update_length = self.serial_module.compute_maximum_update_length();
        // if !serial_module_group_max_update_length.is_active() {
        //     self.has_active_node = false;
        // }
        println!("hijdi");
        group_max_update_length.extend(serial_module_group_max_update_length);

        // we need to find the maximum update length of all connected (fused) units
        // so we run a bfs, we could potentially use rayon to optimize it
        let mut frontier = VecDeque::new();
        let mut visited = HashSet::new();
        visited.insert(self.unit_index);
        for (neighbor, _) in self.adjacent_parallel_units.clone().into_iter() {
            frontier.push_front(neighbor.downgrade());
        }
        println!("hijadfdi");
        while !frontier.is_empty() {
            let temp = frontier.pop_front().unwrap();
            // let mut current = temp.write();
            let serial_module_group_max_update_length = temp.upgrade_force().write().serial_module.compute_maximum_update_length();
            
            println!("in while");
            // if !serial_module_group_max_update_length.is_active() {
            //     current.has_active_node = false;
            // }
            group_max_update_length.extend(serial_module_group_max_update_length);
            println!("in while");
            visited.insert(temp.upgrade_force().read_recursive().unit_index);
            println!("in while");

            for (neighbor, is_fused) in temp.upgrade_force().read_recursive().adjacent_parallel_units.clone().into_iter() {
                println!("in while");
                if !is_fused {
                    continue;
                }
                let neighbor_read = neighbor.read_recursive();
                if !visited.contains(&neighbor_read.unit_index) {
                    println!("in while hh");
                    frontier.push_back(neighbor.downgrade());
                }
                println!("in while h");
            }
            drop(temp);
        }

        println!("after while");

        // we shouldn't need to bfs the graph since each partition does not have children and the has_active_node attribute of children 
        // should not affect this partition 

        // visited.insert(self.unit_index);

        // println!("neighbor len: {}", self.adjacent_parallel_units.len());
        // for neighbor in self.adjacent_parallel_units.iter() {
        //     if !visited.contains(&neighbor.read_recursive().unit_index) {
        //         let neighbor_has_active_node = neighbor.write().dfs_compute_maximum_update_length(group_max_update_length, visited);

        //         if neighbor_has_active_node {
        //             self.has_active_node
        //         }
        //     }
        // }
    }

    // I do need to iteratively grow all the neighbors, instead I only grow this unit
    // this helps me to reduce the time complexity of copying all the nodes from one interface to the other during fusion
    pub fn bfs_grow(&mut self, length: Rational) {
        // early terminate if no active dual nodes in this partition unit
        if !self.has_active_node {
            return;
        }

        self.serial_module.grow(length.clone());
        
        // could potentially use rayon to optimize it
        // implement a breadth first search to grow all connected (fused) neighbors 
        let mut frontier = VecDeque::new();
        let mut visited = HashSet::new();
        visited.insert(self.unit_index);
        for (neighbor, _) in self.adjacent_parallel_units.clone().into_iter() {
            frontier.push_front(neighbor);
        }

        while !frontier.is_empty() {
            let temp = frontier.pop_front().unwrap();
            // let mut current = temp.write();
            temp.write().serial_module.grow(length.clone());
            visited.insert(temp.read_recursive().unit_index);
            
            for (neighbor, is_fused) in temp.read_recursive().adjacent_parallel_units.clone().into_iter() {
                if !is_fused {
                    continue;
                }
                if !visited.contains(&neighbor.read_recursive().unit_index) {
                    frontier.push_back(neighbor);
                }
            }
        }

        // let mut module = self.serial_module;
        // // update the active edges
        // let edge_offset = module.edges[0].read().edge_index;
        // for &edge_index in module.active_edges.iter() {
        //     // if edge_index - edge_offset >= self.edges.len() {
        //     //     continue;
        //     // }
        //     let mut edge = self.edges[edge_index as usize].write();
        //     let mut grow_rate = Rational::zero();
        //     for node_weak in edge.dual_nodes.iter() {
        //         grow_rate += node_weak.upgrade_force().read_recursive().grow_rate.clone();
        //     }
        //     edge.growth += length.clone() * grow_rate;
        //     assert!(
        //         !edge.growth.is_negative(),
        //         "edge {} over-shrunk: the new growth is {:?}",
        //         edge_index,
        //         edge.growth
        //     );
        //     assert!(
        //         edge.growth <= edge.weight,
        //         "edge {} over-grown: the new growth is {:?}, weight is {:?}",
        //         edge_index,
        //         edge.growth,
        //         edge.weight
        //     );
        // }
        // // update dual variables
        // for node_ptr in self.active_nodes.iter() {
        //     let mut node = node_ptr.write();
        //     let grow_rate = node.grow_rate.clone();
        //     let dual_variable = node.get_dual_variable();
        //     node.set_dual_variable(dual_variable + length.clone() * grow_rate);
        // }
    }

    /// dfs to execute sync event
    fn dfs_execute_sync_event(&mut self, sync_event: &SyncRequest, visited: &mut HashSet<usize>) {

        if self.owning_range.contains(sync_event.vertex_index) {
            // println!("the unit containing this dual node is {} with owning range {} to {}", self.unit_index, self.owning_range.range[0], self.owning_range.range[1]);
            self.serial_module.execute_sync_event(sync_event);
            return;
        }

        visited.insert(self.unit_index);

        for (neighbor, _) in self.adjacent_parallel_units.iter() {
            if !visited.contains(&neighbor.read_recursive().unit_index) {
                neighbor.write().dfs_execute_sync_event(sync_event, visited);
            }
        }
    }

    // I do need to iteratively grow all the neighbors, instead I only grow this unit
    // this helps me to reduce the time complexity of copying all the nodes from one interface to the other during fusion
    pub fn bfs_prepare_all(&mut self, sync_requests: &mut Vec<SyncRequest>) {
        // // early terminate if no active dual nodes in this partition unit
        // if !self.has_active_node {
        //     return;
        // }

        let local_sync_requests = self.serial_module.prepare_all();
        sync_requests.append(local_sync_requests);
        
        // could potentially use rayon to optimize it
        // implement a breadth first search to grow all connected (fused) neighbors 
        let mut frontier = VecDeque::new();
        let mut visited = HashSet::new();
        visited.insert(self.unit_index);
        for (neighbor, _) in self.adjacent_parallel_units.clone().into_iter() {
            frontier.push_front(neighbor);
        }

        while !frontier.is_empty() {
            let temp = frontier.pop_front().unwrap();
            // let mut current = temp.write();
            // let local_sync = temp.write().serial_module.prepare_all();
            sync_requests.append(temp.write().serial_module.prepare_all());
            visited.insert(temp.read_recursive().unit_index);
            
            for (neighbor, is_fused) in temp.read_recursive().adjacent_parallel_units.clone().into_iter() {
                if !is_fused {
                    continue;
                }
                if !visited.contains(&neighbor.read_recursive().unit_index) {
                    frontier.push_back(neighbor);
                }
            }
        }
    }

    /// no need to deduplicate the events: the result will always be consistent with the last one
    fn execute_sync_events(&mut self, sync_requests: &[SyncRequest]) {
        // println!("sync_requests: {sync_requests:?}");
        for sync_request in sync_requests.iter() {
            // sync_request.update();
            self.execute_sync_event(sync_request);
        }
    }

    // we need to bias dual node index too when we fuse 2 sets of dual nodes
    pub fn iterative_bias_dual_node_index(&mut self, bias: NodeIndex) {
        // how to access the adjacent DualModuleParallelUnit? Ptr? 
        unimplemented!();
        
        // // depth-first search
        // if let Some((left_child_weak, right_child_weak)) = self.children.as_ref() {
        //     if self.enable_parallel_execution {
        //         rayon::join(
        //             || {
        //                 left_child_weak.upgrade_force().write().iterative_bias_dual_node_index(bias);
        //             },
        //             || {
        //                 right_child_weak.upgrade_force().write().iterative_bias_dual_node_index(bias);
        //             },
        //         );
        //     } else {
        //         left_child_weak.upgrade_force().write().iterative_bias_dual_node_index(bias);
        //         right_child_weak.upgrade_force().write().iterative_bias_dual_node_index(bias);
        //     }
        // }
        // // my serial module
        // self.serial_module.bias_dual_node_index(bias);
    }

    // implement SyncRequest later
    // /// no need to deduplicate the events: the result will always be consistent with the last one
    // fn execute_sync_events(&mut self, sync_requests: &[SyncRequest]) {
    //     // println!("sync_requests: {sync_requests:?}");
    //     for sync_request in sync_requests.iter() {
    //         sync_request.update();
    //         self.execute_sync_event(sync_request);
    //     }
    // }
}


// now we proceed to implement DualModuleImpl for DualModuleParallelUnit 
impl<SerialModule: DualModuleImpl + Send + Sync> DualModuleImpl for DualModuleParallelUnit<SerialModule> {
    /// create a new dual module with empty syndrome
    fn new_empty(_initializer: &SolverInitializer) -> Self {
        // tentative, but in the future, I need to modify this so that I can create a new PartitionUnit and fuse it with an existing bigger block
        panic!("creating parallel unit directly from initializer is forbidden, use `DualModuleParallel::new` instead");
    }

    /// clear all growth and existing dual nodes, prepared for the next decoding
    fn clear(&mut self) {
        self.serial_module.clear();
    }

    /// add defect node
    fn add_defect_node(&mut self, dual_node_ptr: &DualNodePtr, _bias: usize) {
        let defect_vertex = dual_node_ptr.get_representative_vertex();
        println!("add_defect_node: defect vertex found from dual node ptr is {}", defect_vertex);
        let mut visited: HashSet<usize> = HashSet::new();
        self.dfs_add_defect_node(dual_node_ptr, defect_vertex, &mut visited);
    }

    /// add corresponding dual node, note that the `internal_vertices` and `hair_edges` are not set
    fn add_dual_node(&mut self, dual_node_ptr: &DualNodePtr) {
        let defect_vertex = dual_node_ptr.get_representative_vertex();
        println!("add_dual_node: defect vertex found from dual node ptr is {}", defect_vertex);
        let mut visited: HashSet<usize> = HashSet::new();
        self.dfs_add_dual_node(dual_node_ptr, defect_vertex, &mut visited);
    }

    /// update grow rate
    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        let defect_vertex = dual_node_ptr.get_representative_vertex();
        println!("set_grow_rate: defect vertex found from dual node ptr is {}", defect_vertex);
        let mut visited: HashSet<usize> = HashSet::new();
        self.dfs_set_grow_rate(dual_node_ptr, grow_rate, defect_vertex, &mut visited);
    }

    /// An optional function that helps to break down the implementation of [`DualModuleImpl::compute_maximum_update_length`]
    /// check the maximum length to grow (shrink) specific dual node, if length is 0, give the reason of why it cannot further grow (shrink).
    /// if `simultaneous_update` is true, also check for the peer node according to [`DualNode::grow_state`].
    fn compute_maximum_update_length_dual_node(
        &mut self,
        dual_node_ptr: &DualNodePtr,
        simultaneous_update: bool,
    ) -> MaxUpdateLength {
        // unimplemented!()
        // TODO: execute on all nodes that handles this dual node
        let max_update_length =
            self.serial_module
                .compute_maximum_update_length_dual_node(dual_node_ptr, simultaneous_update);
        
        // updating dual node index is performed in fuse fn 
        // // we only update the max_update_length for the units involed in fusion
        // if self.involved_in_fusion {
        //     // max_update_length.update(); // 
        //     match max_update_length {
        //         Self::Unbounded => {}
        //         Self::Conflicting(edge_index) => {
        //             let dual_nodes = self.get_edge_nodes(edge_index);
        //             debug_assert!(
        //                 !dual_nodes.is_empty(),
        //                 "should not conflict if no dual nodes are contributing"
        //             );
                    

        //         }
        //         Self::ShrinkProhibited() => {
    
        //         }
        //         Self::ValidGrow(_) => {} // do nothing
        //     }
        // }
        max_update_length
    }

    /// check the maximum length to grow (shrink) for all nodes, return a list of conflicting reason and a single number indicating the maximum rate to grow:
    /// this number will be 0 if any conflicting reason presents
    fn compute_maximum_update_length(&mut self) -> GroupMaxUpdateLength {
        // // prepare the sync request iteratively
        // self.prepare_all();

        println!("unit compute max update length");
        let mut group_max_update_length = GroupMaxUpdateLength::new();
        self.bfs_compute_maximum_update_length(&mut group_max_update_length);
        
        // // we only update the group_max_update_length for the units involed in fusion
        // if self.involved_in_fusion {
        //     group_max_update_length.update(); 
        // }
        group_max_update_length
    }

    /// An optional function that can manipulate individual dual node, not necessarily supported by all implementations
    fn grow_dual_node(&mut self, dual_node_ptr: &DualNodePtr, length: Rational) {
        let defect_vertex = dual_node_ptr.get_representative_vertex();
        println!("grow_dual_node: defect vertex found from dual node ptr is {}", defect_vertex);
        let mut visited: HashSet<usize> = HashSet::new();
        self.dfs_grow_dual_node(dual_node_ptr, length, defect_vertex, &mut visited);
    }

    /// grow a specific length globally, length must be positive.
    /// note that a negative growth should be implemented by reversing the speed of each dual node
    fn grow(&mut self, length: Rational) {
        // early terminate if no active dual nodes anywhere in the descendant
        if !self.has_active_node {
            return;
        }
        self.bfs_grow(length);
    }

    fn get_edge_nodes(&self, edge_index: EdgeIndex) -> Vec<DualNodePtr> {
        self.serial_module.get_edge_nodes(edge_index)
    }

    fn get_edge_slack(&self, edge_index: EdgeIndex) -> Rational {
        self.serial_module.get_edge_slack(edge_index)
    }

    fn is_edge_tight(&self, edge_index: EdgeIndex) -> bool {
        self.serial_module.is_edge_tight(edge_index)
    }

    fn execute_sync_event(&mut self, sync_event: &SyncRequest) {
        let mut visited: HashSet<usize> = HashSet::new();
        self.dfs_execute_sync_event(sync_event, &mut visited);
    }

    fn prepare_all(&mut self) -> &mut Vec<SyncRequest> {
        let mut sync_requests: Vec<SyncRequest> = vec![];
        self.bfs_prepare_all(&mut sync_requests);
        self.execute_sync_events(&sync_requests);
        sync_requests.clear();
        &mut self.empty_sync_request
    }

    fn get_edge_global_index(&self, local_edge_index: EdgeIndex, unit_index: usize) -> EdgeIndex {
        self.serial_module.get_edge_global_index(local_edge_index, unit_index)
    }
}

// now we proceed to implement the visualization tool 
impl<SerialModule: DualModuleImpl + MWPSVisualizer + Send + Sync> MWPSVisualizer
    for DualModuleParallelUnit<SerialModule>
{
    fn snapshot(&self, abbrev: bool) -> serde_json::Value {
        // incomplete, tentative
        println!("snapshot unit index {}", self.unit_index);
        self.serial_module.snapshot(abbrev)
    }
}

#[cfg(test)]
pub mod tests {
    use std::usize::MAX;

    use super::super::example_codes::*;
    use super::super::primal_module::*;
    use super::super::primal_module_serial::*;
    use crate::decoding_hypergraph::*;
    use super::*;
    use crate::num_traits::FromPrimitive;

    use crate::plugin_single_hair::PluginSingleHair;
    use crate::plugin_union_find::PluginUnionFind;
    use crate::plugin::PluginVec;
    use crate::model_hypergraph::ModelHyperGraph;

    #[test]
    fn dual_module_parallel_tentative_test_1() {
        // cargo test dual_module_parallel_tentative_test_1 -- --nocapture
        let visualize_filename = "dual_module_parallel_tentative_test_1.json".to_string();
        let weight = 600; // do not change, the data is hard-coded
        // let pxy = 0.0602828812732227;
        let code = CodeCapacityPlanarCode::new(7, 0.1, weight);
        let mut visualizer = Visualizer::new(
            Some(visualize_data_folder() + visualize_filename.as_str()),
            code.get_positions(),
            true,
        )
        .unwrap();
        print_visualize_link(visualize_filename);
        visualizer.snapshot("code".to_string(), &code).unwrap();

        // create dual module
        let model_graph = code.get_model_graph();
        let initializer = &model_graph.initializer;
        let mut partition_config = PartitionConfig::new(initializer.vertex_num);
        partition_config.partitions = vec![
            VertexRange::new(0, 18),   // unit 0
            VertexRange::new(24, 42), // unit 1
        ];
        partition_config.fusions = vec![
                    (0, 1), // unit 2, by fusing 0 and 1
                ];
        let a = partition_config.dag_partition_units.add_node(());
        let b = partition_config.dag_partition_units.add_node(());
        partition_config.dag_partition_units.add_edge(a, b, false);

        let partition_info = partition_config.info();

        // create dual module
        let mut dual_module: DualModuleParallel<DualModuleSerial> =
            DualModuleParallel::new_config(&initializer, &partition_info, DualModuleParallelConfig::default());
        
        // try to work on a simple syndrome
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, vec![3, 29, 30]);
        let interface_ptr = DualModuleInterfacePtr::new_load(decoding_graph, &mut dual_module);
        
        // println!("interface_ptr json: {}", interface_ptr.snapshot(false));
        // println!("dual_module json: {}", dual_module.snapshot(false));

        visualizer
            .snapshot_combined("syndrome".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();


        // // grow them each by half
        // let dual_node_17_ptr = interface_ptr.read_recursive().nodes[0].clone();
        // let dual_node_23_ptr = interface_ptr.read_recursive().nodes[1].clone();
        // let dual_node_29_ptr = interface_ptr.read_recursive().nodes[2].clone();
        // let dual_node_30_ptr = interface_ptr.read_recursive().nodes[3].clone();
        // dual_module.grow_dual_node(&dual_node_17_ptr, Rational::from_i64(160).unwrap());
        // dual_module.grow_dual_node(&dual_node_23_ptr, Rational::from_i64(160).unwrap());
        // dual_module.grow_dual_node(&dual_node_29_ptr, Rational::from_i64(160).unwrap());
        // dual_module.grow_dual_node(&dual_node_30_ptr, Rational::from_i64(160).unwrap());
        // // visualizer
        // //     .snapshot_combined("grow".to_string(), vec![&interface_ptr, &dual_module])
        // //     .unwrap();
        // // create cluster
        // interface_ptr.create_node_vec(&[24], &mut dual_module);
        // let dual_node_cluster_ptr = interface_ptr.read_recursive().nodes[4].clone();
        // dual_module.grow_dual_node(&dual_node_17_ptr, Rational::from_i64(160).unwrap());
        // dual_module.grow_dual_node(&dual_node_cluster_ptr, Rational::from_i64(160).unwrap());
        // // visualizer
        // //     .snapshot_combined("grow".to_string(), vec![&interface_ptr, &dual_module])
        // //     .unwrap();
        // // create bigger cluster
        // interface_ptr.create_node_vec(&[18, 23, 24, 31], &mut dual_module);
        // let dual_node_bigger_cluster_ptr = interface_ptr.read_recursive().nodes[5].clone();
        // dual_module.grow_dual_node(&dual_node_bigger_cluster_ptr, Rational::from_i64(120).unwrap());
        // // visualizer
        // //     .snapshot_combined("solved".to_string(), vec![&interface_ptr, &dual_module])
        // //     .unwrap();
        // // the result subgraph
        // let subgraph = vec![82, 24];
        // // visualizer
        // //     .snapshot_combined("subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph])
        // //     .unwrap();

        // grow them each by half
        let dual_node_3_ptr = interface_ptr.read_recursive().nodes[0].clone();
        let dual_node_12_ptr = interface_ptr.read_recursive().nodes[1].clone();
        let dual_node_30_ptr = interface_ptr.read_recursive().nodes[2].clone();
        dual_module.grow_dual_node(&dual_node_3_ptr, Rational::from_usize(weight / 2).unwrap());
        dual_module.grow_dual_node(&dual_node_12_ptr, Rational::from_usize(weight / 2).unwrap());
        dual_module.grow_dual_node(&dual_node_30_ptr, Rational::from_usize(weight / 2).unwrap());
        visualizer
            .snapshot_combined("grow".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();

        // cluster becomes solved
        dual_module.grow_dual_node(&dual_node_3_ptr, Rational::from_usize(weight / 2).unwrap());
        dual_module.grow_dual_node(&dual_node_12_ptr, Rational::from_usize(weight / 2).unwrap());
        dual_module.grow_dual_node(&dual_node_30_ptr, Rational::from_usize(weight / 2).unwrap());

        visualizer
            .snapshot_combined("solved".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();

        // the result subgraph
        let subgraph = vec![15, 20, 27];
        visualizer
            .snapshot_combined("subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph])
            .unwrap();

        
        // create primal module
        // let mut primal_module = PrimalModuleSerialPtr::new_empty(&initializer);
        // primal_module.write().debug_resolve_only_one = true; // to enable debug mode
    }

    #[test]
    fn dual_module_parallel_tentative_test_2() {
        // cargo test dual_module_parallel_tentative_test_2 -- --nocapture
        let visualize_filename = "dual_module_parallel_tentative_test.json".to_string();
        let weight = 1; // do not change, the data is hard-coded
        // let pxy = 0.0602828812732227;
        let code = CodeCapacityPlanarCode::new(7, 0.1, weight);
        let defect_vertices = vec![3, 29];

        let plugins = vec![];
        let growing_strategy = GrowingStrategy::SingleCluster;
        let final_dual = 4;

        // visualizer
        let visualizer = {
            let visualizer = Visualizer::new(
                Some(visualize_data_folder() + visualize_filename.as_str()),
                code.get_positions(),
                true,
            )
            .unwrap();
            print_visualize_link(visualize_filename.clone());
            visualizer
        };

        // create model graph 
        let model_graph = code.get_model_graph();

        // create dual module 
        let mut dual_module = DualModuleSerial::new_empty(&model_graph.initializer);

        // create primal module
        let mut primal_module = PrimalModuleSerial::new_empty(&model_graph.initializer, &model_graph);
        primal_module.growing_strategy = growing_strategy;
        primal_module.plugins = Arc::new(plugins);

        // try to work on a simple syndrom 
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, defect_vertices.clone());
        let interface_ptr = DualModuleInterfacePtr::new(decoding_graph.model_graph.clone());
        primal_module.solve_visualizer(
            &interface_ptr,
            decoding_graph.syndrome_pattern.clone(),
            &mut dual_module,
            Some(visualizer).as_mut(),
        );

        let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module);
        // visualizer.snapshot_combined(
        //             "subgraph".to_string(),
        //             vec![&interface_ptr, &dual_module, &subgraph, &weight_range],
        //         )
        //         .unwrap();
        // if let Some(visualizer) = Some(visualizer).as_mut() {
        //     visualizer
        //         .snapshot_combined(
        //             "subgraph".to_string(),
        //             vec![&interface_ptr, &dual_module, &subgraph, &weight_range],
        //         )
        //         .unwrap();
        // }
        assert!(
            decoding_graph
                .model_graph
                .matches_subgraph_syndrome(&subgraph, &defect_vertices),
            "the result subgraph is invalid"
        );
        assert_eq!(
            Rational::from_usize(final_dual).unwrap(),
            weight_range.upper,
            "unmatched sum dual variables"
        );
        assert_eq!(
            Rational::from_usize(final_dual).unwrap(),
            weight_range.lower,
            "unexpected final dual variable sum"
        );


    }

    #[allow(clippy::too_many_arguments)]
    pub fn dual_module_serial_basic_standard_syndrome_optional_viz(
        _code: impl ExampleCode,
        defect_vertices: Vec<VertexIndex>,
        final_dual: Weight,
        plugins: PluginVec,
        growing_strategy: GrowingStrategy,
        mut dual_module: impl DualModuleImpl + MWPSVisualizer,
        model_graph: Arc<crate::model_hypergraph::ModelHyperGraph>,
        mut visualizer: Option<Visualizer>,
    ) -> (
        DualModuleInterfacePtr,
        PrimalModuleSerial,
        impl DualModuleImpl + MWPSVisualizer,
    ) {
        // create primal module
        let mut primal_module = PrimalModuleSerial::new_empty(&model_graph.initializer, &model_graph);
        primal_module.growing_strategy = growing_strategy;
        primal_module.plugins = Arc::new(plugins);
        // primal_module.config = serde_json::from_value(json!({"timeout":1})).unwrap();
        // try to work on a simple syndrome
        let decoding_graph = DecodingHyperGraph::new_defects(model_graph, defect_vertices.clone());
        let interface_ptr = DualModuleInterfacePtr::new(decoding_graph.model_graph.clone());
        primal_module.solve_visualizer(
            &interface_ptr,
            decoding_graph.syndrome_pattern.clone(),
            &mut dual_module,
            visualizer.as_mut(),
        );

        // // Question: should this be called here
        // // dual_module.update_dual_nodes(&interface_ptr.read_recursive().nodes);

        let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module);
        if let Some(visualizer) = visualizer.as_mut() {
            visualizer
                .snapshot_combined(
                    "subgraph".to_string(),
                    vec![&interface_ptr, &dual_module, &subgraph, &weight_range],
                )
                .unwrap();
        }
        assert!(
            decoding_graph
                .model_graph
                .matches_subgraph_syndrome(&subgraph, &defect_vertices),
            "the result subgraph is invalid"
        );
        // assert_eq!(
        //     Rational::from_usize(final_dual).unwrap(),
        //     weight_range.upper,
        //     "unmatched sum dual variables"
        // );
        // assert_eq!(
        //     Rational::from_usize(final_dual).unwrap(),
        //     weight_range.lower,
        //     "unexpected final dual variable sum"
        // );
        (interface_ptr, primal_module, dual_module)
    }

    pub fn dual_module_serial_basic_standard_syndrome(
        code: impl ExampleCode,
        visualize_filename: String,
        defect_vertices: Vec<VertexIndex>,
        final_dual: Weight,
        plugins: PluginVec,
        growing_strategy: GrowingStrategy,
    ) -> (
        DualModuleInterfacePtr,
        PrimalModuleSerial,
        impl DualModuleImpl + MWPSVisualizer,
    ) {
        println!("hi!");
        println!("{defect_vertices:?}");
        let visualizer = {
            let visualizer = Visualizer::new(
                Some(visualize_data_folder() + visualize_filename.as_str()),
                code.get_positions(),
                true,
            )
            .unwrap();
            print_visualize_link(visualize_filename.clone());
            visualizer
        };

        // create dual module
        let model_graph = code.get_model_graph();
        let initializer = &model_graph.initializer;
        let mut partition_config = PartitionConfig::new(initializer.vertex_num);
        partition_config.partitions = vec![
            VertexRange::new(0, 18),   // unit 0
            VertexRange::new(24, 42), // unit 1
        ];
        partition_config.fusions = vec![
                    (0, 1), // unit 2, by fusing 0 and 1
                ];
        let partition_info = partition_config.info();
        let mut dual_module: DualModuleParallel<DualModuleSerial> =
            DualModuleParallel::new_config(&initializer, &partition_info, DualModuleParallelConfig::default());
        // dual_module.static_fuse_all();

        // let partitioned_initializers = &dual_module.partitioned_initializers;
        // let model_graph = ModelHyperGraph::new_partitioned(&partitioned_initializers[unit_index]);

        dual_module_serial_basic_standard_syndrome_optional_viz(
            code,
            defect_vertices,
            final_dual,
            plugins,
            growing_strategy,
            dual_module,
            model_graph,
            Some(visualizer),
        )
    }

    pub fn graph_time_partition(initializer: &SolverInitializer, positions: &Vec<VisualizePosition>) -> PartitionConfig  {
        assert!(positions.len() > 0, "positive number of positions");
        let mut partition_config = PartitionConfig::new(initializer.vertex_num);
        let mut last_t = positions[0].t;
        let mut t_list: Vec<f64> = vec![];
        t_list.push(last_t);
        for position in positions {
            assert!(position.t >= last_t, "t not monotonically increasing, vertex reordering must be performed before calling this");
            if position.t != last_t {
                t_list.push(position.t);
            }
            last_t = position.t;
        }
            
        // pick the t value in the middle to split it
        let t_split = t_list[t_list.len()/2];
        // find the vertices indices
        let mut split_start_index = MAX;
        let mut split_end_index = MAX;
        for (vertex_index, position) in positions.iter().enumerate() {
            if split_start_index == MAX && position.t == t_split {
                split_start_index = vertex_index;
            }
            if position.t == t_split {
                split_end_index = vertex_index + 1;
            }
        }
        assert!(split_start_index != MAX);
        // partitions are found
        partition_config.partitions = vec![
            VertexRange::new(0, split_start_index),
            VertexRange::new(split_end_index, positions.len()),
        ];
        partition_config.fusions = vec![(0, 1)];
        partition_config
    }

    pub fn dual_module_parallel_evaluation_qec_playground_helper(
        code: impl ExampleCode,
        visualize_filename: String,
        defect_vertices: Vec<VertexIndex>,
        final_dual: Weight,
        plugins: PluginVec,
        growing_strategy: GrowingStrategy,
    ) -> (
        DualModuleInterfacePtr,
        PrimalModuleSerial,
        impl DualModuleImpl + MWPSVisualizer,
    ) {
        println!("{defect_vertices:?}");
        let visualizer = {
            let visualizer = Visualizer::new(
                Some(visualize_data_folder() + visualize_filename.as_str()),
                code.get_positions(),
                true,
            )
            .unwrap();
            print_visualize_link(visualize_filename.clone());
            visualizer
        };

        // create dual module
        let model_graph = code.get_model_graph();
        let initializer = &model_graph.initializer;
        let partition_config = graph_time_partition(&initializer, &code.get_positions());
        let partition_info = partition_config.info();
        let dual_module: DualModuleParallel<DualModuleSerial> =
            DualModuleParallel::new_config(&initializer, &partition_info, DualModuleParallelConfig::default());

        dual_module_serial_basic_standard_syndrome_optional_viz(
            code,
            defect_vertices,
            final_dual,
            plugins,
            growing_strategy,
            dual_module,
            model_graph,
            Some(visualizer),
        )
    }

    /// test a simple case
    #[test]
    fn dual_module_parallel_tentative_test_3() {
        // RUST_BACKTRACE=1 cargo test dual_module_parallel_tentative_test_3 -- --nocapture
        let weight = 1; // do not change, the data is hard-coded
        // let pxy = 0.0602828812732227;
        let code = CodeCapacityPlanarCode::new(7, 0.1, weight);
        // let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
        let defect_vertices = vec![3]; // 3, 29 works

        let visualize_filename = "dual_module_parallel_tentative_test_3.json".to_string();
        dual_module_serial_basic_standard_syndrome(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![],
            GrowingStrategy::SingleCluster,
        );
    }

    #[test]
    fn dual_module_parallel_evaluation_qec_playground() {
        // RUST_BACKTRACE=1 cargo test dual_module_parallel_evaluation_qec_playground -- --nocapture
        let config = json!({
            "code_type": qecp::code_builder::CodeType::RotatedPlanarCode
        });
        
        let code = QECPlaygroundCode::new(3, 0.1, config);
        let defect_vertices = vec![3, 7];

        let visualize_filename = "dual_module_parallel_evaluation_qec_playground.json".to_string();
        dual_module_parallel_evaluation_qec_playground_helper(
            code,
            visualize_filename,
            defect_vertices,
            4,
            vec![],
            GrowingStrategy::SingleCluster,
        );
    }

}