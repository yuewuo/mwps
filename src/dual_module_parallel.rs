/// Parallel Implementation of Dual Module PQ
/// 



use super::dual_module_pq::*;
use crate::{add_shared_methods, dual_module::*};
use super::pointers::*;
use super::util::*;
use super::visualize::*;
use crate::dual_module::DualModuleImpl;
use crate::rayon::prelude::*;
use crate::serde_json;
use crate::weak_table::PtrWeakHashSet;
use hashbrown::HashMap;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::collections::BTreeSet;
use std::collections::HashSet;
use crate::primal_module::Affinity;
use crate::primal_module_serial::PrimalClusterPtr;
use crate::num_traits::{ToPrimitive, Zero};
use crate::ordered_float::OrderedFloat;
use std::collections::VecDeque;


pub struct DualModuleParallelUnit<SerialModule: DualModuleImpl + Send + Sync, Queue> 
where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone, {
    pub unit_index: usize, 
    /// The corresponding serial_module, in this case, the serial module with priority queue implementation
    pub serial_module: DualModulePQ<Queue>,
    /// * The serial units being fused with this serial unit. 
    /// * For non-boundary unit, the initial state of this vector contains the DualModuleParallelUnit of the boundary unit (aka
    /// the unit formed by the boundary vertices of this unit). When more than one such boundary vertices units are present at initialization,
    /// we should insert them based on their respective orientation in the time-space chunk block. 
    /// * For boundary unit, the initial state of this vector is the non-boundary unit it connects to.
    /// * When we fuse 2 DualModuleParallelUnit, we could only fuse a non-boundary unit with a boundary unit 
    pub adjacent_parallel_units: Vec<DualModuleParallelUnitWeak<SerialModule, Queue>>,
    /// Whether this unit is a boundary unit
    pub is_boundary_unit: bool,
    /// partition info
    pub partition_info: Arc<PartitionInfo>,
    /// owning_range
    pub owning_range: VertexRange,
    pub enable_parallel_execution: bool,
    /// should think a bit more about whether having this makes sense
    /// the current mode of the dual module
    ///     note: currently does not have too much functionality
    mode: DualModuleMode,
}

pub type DualModuleParallelUnitPtr<SerialModule, Queue> = ArcRwLock<DualModuleParallelUnit<SerialModule, Queue>>;
pub type DualModuleParallelUnitWeak<SerialModule, Queue> = WeakRwLock<DualModuleParallelUnit<SerialModule, Queue>>;

impl<SerialModule: DualModuleImpl + Send + Sync, Queue> std::fmt::Debug for DualModuleParallelUnitPtr<SerialModule, Queue> 
where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let unit = self.read_recursive();
        write!(f, "{}", unit.unit_index)
    }
}

impl<SerialModule: DualModuleImpl + Send + Sync, Queue> std::fmt::Debug for DualModuleParallelUnitWeak<SerialModule, Queue> 
where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.upgrade_force().fmt(f)
    }
}

pub struct DualModuleParallel<SerialModule: DualModuleImpl + Send + Sync, Queue> 
where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone, 
{
    /// the set of all DualModuleParallelUnits, one for each partition
    /// we set the read-write lock 
    pub units: Vec<ArcRwLock<DualModuleParallelUnit<SerialModule, Queue>>>,
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

    /// should think more about whether having this makes sense
    /// the current mode of the dual module
    ///     note: currently does not have too much functionality
    mode: DualModuleMode,
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


impl<SerialModule: DualModuleImpl + Send + Sync, Queue> DualModuleParallel<SerialModule, Queue> 
where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
{
    #[allow(clippy::unnecessary_cast)]
    pub fn new_config(
        initializer: &SolverInitializer,
        partition_info: &PartitionInfo,
        config: DualModuleParallelConfig
    ) -> Self 
    {
        // automatic reference counter for partition info
        let partition_info = Arc::new(partition_info.clone());

        // build thread pool 
        let mut thread_pool_builder = rayon::ThreadPoolBuilder::new();
        if config.thread_pool_size != 0 {
            thread_pool_builder = thread_pool_builder.num_threads(config.thread_pool_size);
        }
        let thread_pool = thread_pool_builder.build().expect("creating thread pool failed");

        // // create partition_units
        
        
        // let partition_units: Vec<PartitionUnitPtr> = (0..unit_count).map(|unit_index| {
        //     PartitionUnitPtr::new_value(PartitionUnit {
        //         unit_index,
        //     })
        // }).collect();

        // build partition initializer
        let mut units = vec![];
        let unit_count = partition_info.units.len();
        let mut partitioned_initializers: Vec<PartitionedSolverInitializer> = (0..unit_count).map(|unit_index| {
            let unit_partition_info = &partition_info.units[unit_index];
            let owning_range = &unit_partition_info.owning_range;
            let boundary_vertices = &unit_partition_info.boundary_vertices;

            PartitionedSolverInitializer {
                unit_index,
                vertex_num: initializer.vertex_num,
                edge_num: initializer.weighted_edges.len(),
                owning_range: *owning_range,
                weighted_edges: vec![],
                boundary_vertices: boundary_vertices.clone(),

                // boundary_vertices: unit_partition_info.boundary_vertices.clone(),
                // adjacent_partition_units: unit_partition_info.adjacent_partition_units.clone(),
                // owning_interface: Some(partition_units[unit_index].downgrade()),
            }
        }).collect();

        // now we assign each edge to its unique partition
        // println!("edge num: {}", initializer.weighted_edges.len());
        let mut edge_bias_vec = [core::usize::MAX, unit_count];
        for (edge_index, hyper_edge) in initializer.weighted_edges.iter().enumerate() {
            let mut vertices_unit_indices: HashMap<usize, Vec<usize>> = HashMap::new();
            let mut boundary_vertices_adjacent_units_index: HashMap<usize, Vec<usize>> = HashMap::new(); // key: unit_index; value: all vertex indices belong to this unit
            let mut exist_boundary_vertex = false;
            for vertex_index in hyper_edge.vertices.iter() {
                let unit_index = partition_info.vertex_to_owning_unit.get(vertex_index).unwrap();
                let unit = &partition_info.units[*unit_index];
                if unit.is_boundary_unit {
                    exist_boundary_vertex = true;
                    if let Some(x) = boundary_vertices_adjacent_units_index.get_mut(unit_index) {
                        x.push(*vertex_index);
                    } else {
                        let mut vertices = vec![];
                        vertices.push(*vertex_index);
                        boundary_vertices_adjacent_units_index.insert(*unit_index, vertices.clone());
                    }
                } else {
                    if let Some(x) = vertices_unit_indices.get_mut(unit_index) {
                        x.push(*vertex_index);
                    } else {
                        let mut vertices = vec![];
                        vertices.push(*vertex_index);
                        vertices_unit_indices.insert(*unit_index, vertices.clone());
                    }
                }
            }

            // println!("hyper_edge index: {edge_index}");
            // println!("vertices_unit_indices: {vertices_unit_indices:?}");
            // println!("boundary vertices adjacent unit indices: {boundary_vertices_adjacent_units_index:?}");


            // if all vertices are the boundary vertices 
            if vertices_unit_indices.len() == 0 {
                // we add the hyperedge to the boundary unit
                let unit_index = boundary_vertices_adjacent_units_index.keys().next().unwrap();
                partitioned_initializers[*unit_index].weighted_edges.push((hyper_edge.clone(), edge_index));
            } else {
                let first_vertex_unit_index = *vertices_unit_indices.keys().next().unwrap();
                let all_vertex_from_same_unit = vertices_unit_indices.len() == 1;
                if !exist_boundary_vertex {
                    // all within owning range of one unit (since for the vertices to span multiple units, one of them has to be the boundary vertex)
                    // we assume that for vertices of a hyperedge, if there aren't any boundary vertices among them, they must belong to the same partition unit 
                    assert!(all_vertex_from_same_unit, "For the vertices of hyperedge {}, there does not exist boundary vertex but all the vertices do not belong to the same unit", edge_index);
                    // since all vertices this hyperedge connects to belong to the same unit, we can assign this hyperedge to that partition unit
                    partitioned_initializers[first_vertex_unit_index].weighted_edges.push((hyper_edge.clone(), edge_index));
                } else {
                    // the vertices span multiple units
                    if all_vertex_from_same_unit {
                        // for sanity check, should not be triggered
                        partitioned_initializers[first_vertex_unit_index].weighted_edges.push((hyper_edge.clone(), edge_index));
                    } else {
                        // println!("exist boundary vertices, vertices unit indices {vertices_unit_indices:?}");
                        // if the vertices of this hyperedge (excluding the boundary vertices) belong to 2 different partition unit
                        // sanity check: there really are only 2 unique partition units 
                        // let mut sanity_check = HashSet::new();
                        // for (_vertex_index, vertex_unit_index) in &vertices_unit_indices {
                        //     sanity_check.insert(vertex_unit_index);
                        // }
                        // assert!(sanity_check.len() == 2, "there are fewer than 2 or more than 2 partition units");
    
                        // we create new hyperedge with the boundary vertex + verticies exlusive for one partition unit
                        for (unit_index, vertices) in vertices_unit_indices.iter_mut() {
                            if let Some(boundary_vertices) = boundary_vertices_adjacent_units_index.get(unit_index) {
                                vertices.extend(boundary_vertices);
                            } 
                        }
                  
                        // now we add the boundary vertices in
                        for (unit_index, vertices) in vertices_unit_indices.iter() {
                            partitioned_initializers[*unit_index].weighted_edges.push(
                                (HyperEdge::new(vertices.clone(), hyper_edge.weight), edge_index)
                            );
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
                    let mut dual_module: DualModulePQ<Queue> = DualModulePQ::new_partitioned(&partitioned_initializers[unit_index]);

                    DualModuleParallelUnitPtr::new_value(DualModuleParallelUnit {
                        unit_index,
                        partition_info: Arc::clone(&partition_info),
                        owning_range: partition_info.units[unit_index].owning_range,
                        serial_module: dual_module,
                        enable_parallel_execution: config.enable_parallel_execution,
                        adjacent_parallel_units: vec![],
                        is_boundary_unit: partition_info.units[unit_index].is_boundary_unit,
                        mode: DualModuleMode::default(),
                    })
                  
                })
                .collect_into_vec(&mut units);
        });

        // we need to fill in the adjacent_parallel_units here 
        for unit_index in 0..unit_count {
            let mut unit = units[unit_index].write();
            for adjacent_unit_index in &partition_info.units[unit_index].adjacent_parallel_units {
                unit.adjacent_parallel_units.push(units[*adjacent_unit_index].downgrade());
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
            mode: DualModuleMode::default(),
        }
    }

    /// find the parallel unit that handles this dual node, should be unique
    pub fn find_handling_parallel_unit(&self, dual_node_ptr: &DualNodePtr) -> DualModuleParallelUnitPtr<SerialModule, Queue> {
        let defect_ptr = dual_node_ptr.get_representative_vertex();
        let owning_unit_index = self.partition_info.vertex_to_owning_unit.get(&defect_ptr.read_recursive().vertex_index);
        match owning_unit_index {
            Some(x) => {
                let owning_unit_ptr = self.units[*x].clone();
                return owning_unit_ptr;
            },
            None => {
                panic!("This dual node {} is not contained in any partition, we cannot find a parallel unit that handles this dual node.", defect_ptr.read_recursive().vertex_index)
            }}
    }
}

impl<SerialModule: DualModuleImpl + Send + Sync, Queue> DualModuleImpl for DualModuleParallel<SerialModule, Queue> 
where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
{
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
            })
        })
    }

    /// add defect node
    fn add_defect_node(&mut self, dual_node_ptr: &DualNodePtr) {
        let unit_ptr = self.find_handling_parallel_unit(dual_node_ptr);
        self.thread_pool.scope(|_| {
            let mut unit = unit_ptr.write();
            unit.add_defect_node(dual_node_ptr); 
        })
    }

    /// add corresponding dual node, note that the `internal_vertices` and `hair_edges` are not set
    fn add_dual_node(&mut self, dual_node_ptr: &DualNodePtr) {
        let unit_ptr = self.find_handling_parallel_unit(dual_node_ptr);
        self.thread_pool.scope(|_| {
            let mut unit = unit_ptr.write();
            unit.add_dual_node(dual_node_ptr); 
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

    /// grow a specific length globally, length must be positive.
    /// note that a negative growth should be implemented by reversing the speed of each dual node
    fn grow(&mut self, length: Rational) {
        self.thread_pool.scope(|_| {
            self.units.par_iter().for_each(|unit_ptr| {
                let mut unit = unit_ptr.write();
                unit.grow(length.clone()); // to be implemented in DualModuleParallelUnit
            });
        })
    }

    /// come back later to fix the owning_edge_range contains
    fn get_edge_nodes(&self, edge_ptr: EdgePtr) -> Vec<DualNodePtr> {
        edge_ptr.read_recursive()
                .dual_nodes
                .iter()
                .map(|x| x.upgrade_force().ptr)
                .collect()
    }
    fn get_edge_slack(&self, edge_ptr: EdgePtr) -> Rational {
        unimplemented!()
        // let edge = edge_ptr.read_recursive();
        // edge.weight.clone()
        //     - (self.global_time.read_recursive().clone() - edge.last_updated_time.clone()) * edge.grow_rate.clone()
        //     - edge.growth_at_last_updated_time.clone()
    }
    fn is_edge_tight(&self, edge_ptr: EdgePtr) -> bool {
        self.get_edge_slack(edge_ptr).is_zero()
    }

    /* New tuning-related methods */   
    // tuning mode shared methods
    add_shared_methods!(); 
    
    /// syncing all possible states (dual_variable and edge_weights) with global time, so global_time can be discarded later
    fn sync(&mut self) {
        self.thread_pool.scope(|_| {
            self.units.par_iter().for_each(|unit_ptr| {
                let mut unit = unit_ptr.write();
                unit.sync(); // to be implemented in DualModuleParallelUnit
            });
        })
    }

    /// grow a specific edge on the spot
    fn grow_edge(&self, edge_ptr: EdgePtr, amount: &Rational) {
        let mut edge = edge_ptr.write();
        edge.growth_at_last_updated_time += amount;
    }

    /// `is_edge_tight` but in tuning phase
    fn is_edge_tight_tune(&self, edge_ptr: EdgePtr) -> bool {
        let edge = edge_ptr.read_recursive();
        edge.weight == edge.growth_at_last_updated_time
    }

    /// `get_edge_slack` but in tuning phase
    fn get_edge_slack_tune(&self, edge_ptr: EdgePtr) -> Rational {
        let edge = edge_ptr.read_recursive();
        edge.weight.clone() - edge.growth_at_last_updated_time.clone()
    }

    /// change mode, clear queue as queue is no longer needed. also sync to get rid off the need for global time
    fn advance_mode(&mut self) {
        unimplemented!()
        // self.mode_mut().advance();
        // self.obstacle_queue.clear();
        // self.sync();
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
    fn calculate_cluster_affinity(&mut self, cluster: PrimalClusterPtr) -> Option<Affinity> {
        let mut start = 0.0;
        let cluster = cluster.read_recursive();
        start -= cluster.edges.len() as f64 + cluster.nodes.len() as f64;

        let mut weight = Rational::zero();
        for edge_ptr in cluster.edges.iter() {
            // let edge_ptr = self.edges[edge_index].read_recursive();
            let edge = edge_ptr.read_recursive();
            weight += &edge.weight - &edge.growth_at_last_updated_time;
        }
        for node in cluster.nodes.iter() {
            let dual_node = node.read_recursive().dual_node_ptr.clone();
            weight -= &dual_node.read_recursive().dual_variable_at_last_updated_time;
        }
        if weight.is_zero() {
            return None;
        }
        start += weight.to_f64().unwrap();
        Some(OrderedFloat::from(start))
    }

    /// get the edge free weight, for each edge what is the weight that are free to use by the given participating dual variables
    fn get_edge_free_weight(
        &self,
        edge_ptr: EdgePtr,
        participating_dual_variables: &hashbrown::HashSet<usize>,
    ) -> Rational {
        // let edge = self.edges[edge_index as usize].read_recursive();
        let edge = edge_ptr.read_recursive();
        let mut free_weight = edge.weight.clone();
        for dual_node in edge.dual_nodes.iter() {
            if participating_dual_variables.contains(&dual_node.index) {
                continue;
            }
            let dual_node = dual_node.upgrade_force();
            free_weight -= &dual_node.ptr.read_recursive().dual_variable_at_last_updated_time;
        }

        free_weight
    }

    /// exist for testing purposes
    fn get_vertex_ptr(&self, vertex_index: VertexIndex) -> VertexPtr {
        for unit in self.units.iter() {
            if unit.read_recursive().owning_range.contains(vertex_index) {
                return unit.read_recursive().get_vertex_ptr(vertex_index);
            }
        }
        panic!("none of the units in DualModuleParallel contain vertex_index, cannot find the corresponding vertex pointer");
    }

    /// exist for testing purposes
    fn get_edge_ptr(&self, edge_index: EdgeIndex) -> EdgePtr {
        for unit in self.units.iter() {
            if unit.read_recursive().owning_range.contains(edge_index) {
                return unit.read_recursive().get_edge_ptr(edge_index);
            }
        }
        panic!("none of the units in DualModuleParallel contain vertex_index, cannot find the corresponding vertex pointer");
    }
}


impl<SerialModule: DualModuleImpl + Send + Sync, Queue> DualModuleImpl for DualModuleParallelUnit<SerialModule, Queue> 
where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
{
    /// create a new dual module with empty syndrome
    fn new_empty(initializer: &SolverInitializer) -> Self {
        // tentative, but in the future, I need to modify this so that I can create a new PartitionUnit and fuse it with an existing bigger block
        panic!("creating parallel unit directly from initializer is forbidden, use `DualModuleParallel::new` instead");
    }

    /// clear all growth and existing dual nodes, prepared for the next decoding
    fn clear(&mut self) {
        self.serial_module.clear();
    }

    /// add defect node
    fn add_defect_node(&mut self, dual_node_ptr: &DualNodePtr) {
        self.serial_module.add_defect_node(dual_node_ptr);
    }

    /// add corresponding dual node, note that the `internal_vertices` and `hair_edges` are not set
    fn add_dual_node(&mut self, dual_node_ptr: &DualNodePtr) {
        self.serial_module.add_dual_node(dual_node_ptr);
    }

    /// update grow rate
    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        self.serial_module.set_grow_rate(dual_node_ptr, grow_rate);
    }

    /// An optional function that helps to break down the implementation of [`DualModuleImpl::compute_maximum_update_length`]
    /// check the maximum length to grow (shrink) specific dual node, if length is 0, give the reason of why it cannot further grow (shrink).
    /// if `simultaneous_update` is true, also check for the peer node according to [`DualNode::grow_state`].
    fn compute_maximum_update_length_dual_node(
        &mut self,
        dual_node_ptr: &DualNodePtr,
        simultaneous_update: bool,
    ) -> MaxUpdateLength {
        self.serial_module
            .compute_maximum_update_length_dual_node(dual_node_ptr, simultaneous_update)
    
        // updating dual node index is performed in fuse fn 
        // // we only update the max_update_length for the units involed in fusion
    }

    /// check the maximum length to grow (shrink) for all nodes, return a list of conflicting reason and a single number indicating the maximum rate to grow:
    /// this number will be 0 if any conflicting reason presents
    fn compute_maximum_update_length(&mut self) -> GroupMaxUpdateLength {
        println!("unit compute max update length");
        let mut group_max_update_length = GroupMaxUpdateLength::new();
        self.bfs_compute_maximum_update_length(&mut group_max_update_length);
        
        // // we only update the group_max_update_length for the units involed in fusion
        // if self.involved_in_fusion {
        //     group_max_update_length.update(); 
        // }
        group_max_update_length
    }

    /// grow a specific length globally, length must be positive.
    /// note that a negative growth should be implemented by reversing the speed of each dual node
    fn grow(&mut self, length: Rational) {
        self.bfs_grow(length);
    }

    fn get_edge_nodes(&self, edge_ptr: EdgePtr) -> Vec<DualNodePtr> {
        self.serial_module.get_edge_nodes(edge_ptr)
    }
    fn get_edge_slack(&self, edge_ptr: EdgePtr) -> Rational {
        self.serial_module.get_edge_slack(edge_ptr)
    }
    fn is_edge_tight(&self, edge_ptr: EdgePtr) -> bool {
        self.serial_module.is_edge_tight(edge_ptr)
    }

    /* New tuning-related methods */
    /// mode mangements
    // tuning mode shared methods
    add_shared_methods!();

    fn advance_mode(&mut self) {
        self.serial_module.advance_mode();
    }

    /// syncing all possible states (dual_variable and edge_weights) with global time, so global_time can be discarded later
    fn sync(&mut self) {
        self.serial_module.sync();
    }

    /// grow a specific edge on the spot
    fn grow_edge(&self, edge_ptr: EdgePtr, amount: &Rational) {
        self.serial_module.grow_edge(edge_ptr, amount);
    }

    /// `is_edge_tight` but in tuning phase
    fn is_edge_tight_tune(&self, edge_ptr: EdgePtr) -> bool {
        self.serial_module.is_edge_tight_tune(edge_ptr)
    }

    /// `get_edge_slack` but in tuning phase
    fn get_edge_slack_tune(&self, edge_ptr: EdgePtr) -> Rational {
        self.serial_module.get_edge_slack_tune(edge_ptr)
    }

    /* miscs */

    /// print all the states for the current dual module
    fn debug_print(&self) {
        self.serial_module.debug_print();
    }

    /* affinity */

    /// calculate affinity based on the following metric
    ///     Clusters with larger primal-dual gaps will receive high affinity because working on those clusters
    ///     will often reduce the gap faster. However, clusters with a large number of dual variables, vertices,
    ///     and hyperedges will receive a lower affinity
    fn calculate_cluster_affinity(&mut self, cluster: PrimalClusterPtr) -> Option<Affinity> {
        self.serial_module.calculate_cluster_affinity(cluster)
    }

    /// get the edge free weight, for each edge what is the weight that are free to use by the given participating dual variables
    fn get_edge_free_weight(
        &self,
        edge_ptr: EdgePtr,
        participating_dual_variables: &hashbrown::HashSet<usize>,
    ) -> Rational {
        self.serial_module.get_edge_free_weight(edge_ptr, participating_dual_variables)
    }

    /// exist for testing purposes
    fn get_vertex_ptr(&self, vertex_index: VertexIndex) -> VertexPtr {
        let local_vertex_index = vertex_index - self.owning_range.start();
        self.serial_module.get_vertex_ptr(local_vertex_index)
    }

    /// exist for testing purposes
    fn get_edge_ptr(&self, edge_index: EdgeIndex) -> EdgePtr {
        let local_edge_index = edge_index - self.owning_range.start();
        self.serial_module.get_edge_ptr(local_edge_index)
    }
}


// impl<SerialModule: DualModuleImpl + Send + Sync, Queue> DualModuleParallelUnit<SerialModule, Queue> 
// where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug,
// {
//     fn new_config(
//         initializer: &SolverInitializer,
//         partition_info: &PartitionInfo, // contains the partition info of all partition units
//         config: DualModuleParallelConfig
//     ) -> Self 
//     {
        


//         Self {
//             unit_index:  ,
//             serial_module: ,
//             adjacent_parallel_units: ,
//             is_boundary_unit: , 

//         }


//     }
// }

impl<SerialModule: DualModuleImpl + Send + Sync, Queue> DualModuleParallelUnit<SerialModule, Queue> 
where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
{
    // pub fn fuse_helper(&mut self,         
    //     other_dual_unit: &DualModuleParallelUnitPtr<SerialModule, Queue>
    // ) {
    //     if let Some(is_fused) = self.adjacent_parallel_units.get_mut(other_dual_unit) {
    //         *is_fused = true;
    //     }        
    // }
    
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
        let mut frontier: VecDeque<WeakRwLock<DualModuleParallelUnit<SerialModule, Queue>>> = VecDeque::new();
        let mut visited = HashSet::new();
        visited.insert(self.unit_index);
        for neighbor in self.adjacent_parallel_units.clone().into_iter() {
            frontier.push_front(neighbor);
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

            for neighbor in temp.upgrade_force().read_recursive().adjacent_parallel_units.clone().into_iter() {
                println!("in while");
                let neighbor_ptr = neighbor.upgrade_force();
                let neighbor_read = neighbor_ptr.read_recursive();
                if !visited.contains(&neighbor_read.unit_index) {
                    println!("in while hh");
                    frontier.push_back(neighbor);
                }
                println!("in while h");
                drop(neighbor_read);
            }
            drop(temp);
        }

        println!("after while");
    }

    // I do need to iteratively grow all the neighbors, instead I only grow this unit
    // this helps me to reduce the time complexity of copying all the nodes from one interface to the other during fusion
    pub fn bfs_grow(&mut self, length: Rational) {
        // early terminate if no active dual nodes in this partition unit
        // if !self.has_active_node {
        //     return;
        // }

        self.serial_module.grow(length.clone());
        
        // could potentially use rayon to optimize it
        // implement a breadth first search to grow all connected (fused) neighbors 
        let mut frontier: VecDeque<WeakRwLock<DualModuleParallelUnit<SerialModule, Queue>>> = VecDeque::new();
        let mut visited = HashSet::new();
        visited.insert(self.unit_index);
        for neighbor in self.adjacent_parallel_units.clone().into_iter() {
            frontier.push_front(neighbor);
        }

        while !frontier.is_empty() {
            let temp = frontier.pop_front().unwrap();
            // let mut current = temp.write();
            temp.upgrade_force().write().serial_module.grow(length.clone());
            visited.insert(temp.upgrade_force().read_recursive().unit_index);
            
            for neighbor in temp.upgrade_force().read_recursive().adjacent_parallel_units.clone().into_iter() {
                if !visited.contains(&neighbor.upgrade_force().read_recursive().unit_index) {
                    frontier.push_back(neighbor);
                }
            }
        }
    }
}



// now we implement the visualization functions
impl<SerialModule: DualModuleImpl + MWPSVisualizer + Send + Sync, Queue> MWPSVisualizer for DualModuleParallel<SerialModule, Queue> 
where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
{
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

// now we proceed to implement the visualization tool 
impl<SerialModule: DualModuleImpl + MWPSVisualizer + Send + Sync, Queue> MWPSVisualizer for DualModuleParallelUnit<SerialModule, Queue> 
where Queue: FutureQueueMethods<Rational, Obstacle> + Default + std::fmt::Debug + Send + Sync + Clone,
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
    fn dual_module_parallel_tentative_test_1() 
    where 
    {
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
        let mut dual_module: DualModuleParallel<DualModulePQ<FutureObstacleQueue<Rational>>, FutureObstacleQueue<Rational>> =
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

        // // the result subgraph
        // let subgraph = vec![15, 20, 27];
        // visualizer
        //     .snapshot_combined("subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph])
        //     .unwrap();

        
        // create primal module
        // let mut primal_module = PrimalModuleSerialPtr::new_empty(&initializer);
        // primal_module.write().debug_resolve_only_one = true; // to enable debug mode
    }

    // #[test]
    // fn dual_module_parallel_tentative_test_2() {
    //     // cargo test dual_module_parallel_tentative_test_2 -- --nocapture
    //     let visualize_filename = "dual_module_parallel_tentative_test.json".to_string();
    //     let weight = 1; // do not change, the data is hard-coded
    //     // let pxy = 0.0602828812732227;
    //     let code = CodeCapacityPlanarCode::new(7, 0.1, weight);
    //     let defect_vertices = vec![3, 29];

    //     let plugins = vec![];
    //     let growing_strategy = GrowingStrategy::SingleCluster;
    //     let final_dual = 4;

    //     // visualizer
    //     let visualizer = {
    //         let visualizer = Visualizer::new(
    //             Some(visualize_data_folder() + visualize_filename.as_str()),
    //             code.get_positions(),
    //             true,
    //         )
    //         .unwrap();
    //         print_visualize_link(visualize_filename.clone());
    //         visualizer
    //     };

    //     // create model graph 
    //     let model_graph = code.get_model_graph();

    //     // create dual module 
    //     let mut dual_module = DualModuleSerial::new_empty(&model_graph.initializer);

    //     // create primal module
    //     let mut primal_module = PrimalModuleSerial::new_empty(&model_graph.initializer, &model_graph);
    //     primal_module.growing_strategy = growing_strategy;
    //     primal_module.plugins = Arc::new(plugins);

    //     // try to work on a simple syndrom 
    //     let decoding_graph = DecodingHyperGraph::new_defects(model_graph, defect_vertices.clone());
    //     let interface_ptr = DualModuleInterfacePtr::new(decoding_graph.model_graph.clone());
    //     primal_module.solve_visualizer(
    //         &interface_ptr,
    //         decoding_graph.syndrome_pattern.clone(),
    //         &mut dual_module,
    //         Some(visualizer).as_mut(),
    //     );

    //     let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module);
    //     // visualizer.snapshot_combined(
    //     //             "subgraph".to_string(),
    //     //             vec![&interface_ptr, &dual_module, &subgraph, &weight_range],
    //     //         )
    //     //         .unwrap();
    //     // if let Some(visualizer) = Some(visualizer).as_mut() {
    //     //     visualizer
    //     //         .snapshot_combined(
    //     //             "subgraph".to_string(),
    //     //             vec![&interface_ptr, &dual_module, &subgraph, &weight_range],
    //     //         )
    //     //         .unwrap();
    //     // }
    //     assert!(
    //         decoding_graph
    //             .model_graph
    //             .matches_subgraph_syndrome(&subgraph, &defect_vertices),
    //         "the result subgraph is invalid"
    //     );
    //     assert_eq!(
    //         Rational::from_usize(final_dual).unwrap(),
    //         weight_range.upper,
    //         "unmatched sum dual variables"
    //     );
    //     assert_eq!(
    //         Rational::from_usize(final_dual).unwrap(),
    //         weight_range.lower,
    //         "unexpected final dual variable sum"
    //     );


    // }

    // #[allow(clippy::too_many_arguments)]
    // pub fn dual_module_serial_basic_standard_syndrome_optional_viz(
    //     _code: impl ExampleCode,
    //     defect_vertices: Vec<VertexIndex>,
    //     final_dual: Weight,
    //     plugins: PluginVec,
    //     growing_strategy: GrowingStrategy,
    //     mut dual_module: impl DualModuleImpl + MWPSVisualizer,
    //     model_graph: Arc<crate::model_hypergraph::ModelHyperGraph>,
    //     mut visualizer: Option<Visualizer>,
    // ) -> (
    //     DualModuleInterfacePtr,
    //     PrimalModuleSerial,
    //     impl DualModuleImpl + MWPSVisualizer,
    // ) {
    //     // create primal module
    //     let mut primal_module = PrimalModuleSerial::new_empty(&model_graph.initializer, &model_graph);
    //     primal_module.growing_strategy = growing_strategy;
    //     primal_module.plugins = Arc::new(plugins);
    //     // primal_module.config = serde_json::from_value(json!({"timeout":1})).unwrap();
    //     // try to work on a simple syndrome
    //     let decoding_graph = DecodingHyperGraph::new_defects(model_graph, defect_vertices.clone());
    //     let interface_ptr = DualModuleInterfacePtr::new(decoding_graph.model_graph.clone());
    //     primal_module.solve_visualizer(
    //         &interface_ptr,
    //         decoding_graph.syndrome_pattern.clone(),
    //         &mut dual_module,
    //         visualizer.as_mut(),
    //     );

    //     // // Question: should this be called here
    //     // // dual_module.update_dual_nodes(&interface_ptr.read_recursive().nodes);

    //     let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module);
    //     if let Some(visualizer) = visualizer.as_mut() {
    //         visualizer
    //             .snapshot_combined(
    //                 "subgraph".to_string(),
    //                 vec![&interface_ptr, &dual_module, &subgraph, &weight_range],
    //             )
    //             .unwrap();
    //     }
    //     assert!(
    //         decoding_graph
    //             .model_graph
    //             .matches_subgraph_syndrome(&subgraph, &defect_vertices),
    //         "the result subgraph is invalid"
    //     );
    //     // assert_eq!(
    //     //     Rational::from_usize(final_dual).unwrap(),
    //     //     weight_range.upper,
    //     //     "unmatched sum dual variables"
    //     // );
    //     // assert_eq!(
    //     //     Rational::from_usize(final_dual).unwrap(),
    //     //     weight_range.lower,
    //     //     "unexpected final dual variable sum"
    //     // );
    //     (interface_ptr, primal_module, dual_module)
    // }

    // pub fn dual_module_serial_basic_standard_syndrome(
    //     code: impl ExampleCode,
    //     visualize_filename: String,
    //     defect_vertices: Vec<VertexIndex>,
    //     final_dual: Weight,
    //     plugins: PluginVec,
    //     growing_strategy: GrowingStrategy,
    // ) -> (
    //     DualModuleInterfacePtr,
    //     PrimalModuleSerial,
    //     impl DualModuleImpl + MWPSVisualizer,
    // ) {
    //     println!("hi!");
    //     println!("{defect_vertices:?}");
    //     let visualizer = {
    //         let visualizer = Visualizer::new(
    //             Some(visualize_data_folder() + visualize_filename.as_str()),
    //             code.get_positions(),
    //             true,
    //         )
    //         .unwrap();
    //         print_visualize_link(visualize_filename.clone());
    //         visualizer
    //     };

    //     // create dual module
    //     let model_graph = code.get_model_graph();
    //     let initializer = &model_graph.initializer;
    //     let mut partition_config = PartitionConfig::new(initializer.vertex_num);
    //     partition_config.partitions = vec![
    //         VertexRange::new(0, 18),   // unit 0
    //         VertexRange::new(24, 42), // unit 1
    //     ];
    //     partition_config.fusions = vec![
    //                 (0, 1), // unit 2, by fusing 0 and 1
    //             ];
    //     let partition_info = partition_config.info();
    //     let mut dual_module: DualModuleParallel<DualModuleSerial> =
    //         DualModuleParallel::new_config(&initializer, &partition_info, DualModuleParallelConfig::default());
    //     // dual_module.static_fuse_all();

    //     // let partitioned_initializers = &dual_module.partitioned_initializers;
    //     // let model_graph = ModelHyperGraph::new_partitioned(&partitioned_initializers[unit_index]);

    //     dual_module_serial_basic_standard_syndrome_optional_viz(
    //         code,
    //         defect_vertices,
    //         final_dual,
    //         plugins,
    //         growing_strategy,
    //         dual_module,
    //         model_graph,
    //         Some(visualizer),
    //     )
    // }

    // pub fn graph_time_partition(initializer: &SolverInitializer, positions: &Vec<VisualizePosition>) -> PartitionConfig  {
    //     assert!(positions.len() > 0, "positive number of positions");
    //     let mut partition_config = PartitionConfig::new(initializer.vertex_num);
    //     let mut last_t = positions[0].t;
    //     let mut t_list: Vec<f64> = vec![];
    //     t_list.push(last_t);
    //     for position in positions {
    //         assert!(position.t >= last_t, "t not monotonically increasing, vertex reordering must be performed before calling this");
    //         if position.t != last_t {
    //             t_list.push(position.t);
    //         }
    //         last_t = position.t;
    //     }
            
    //     // pick the t value in the middle to split it
    //     let t_split = t_list[t_list.len()/2];
    //     // find the vertices indices
    //     let mut split_start_index = MAX;
    //     let mut split_end_index = MAX;
    //     for (vertex_index, position) in positions.iter().enumerate() {
    //         if split_start_index == MAX && position.t == t_split {
    //             split_start_index = vertex_index;
    //         }
    //         if position.t == t_split {
    //             split_end_index = vertex_index + 1;
    //         }
    //     }
    //     assert!(split_start_index != MAX);
    //     // partitions are found
    //     partition_config.partitions = vec![
    //         VertexRange::new(0, split_start_index),
    //         VertexRange::new(split_end_index, positions.len()),
    //     ];
    //     partition_config.fusions = vec![(0, 1)];
    //     partition_config
    // }

    // pub fn dual_module_parallel_evaluation_qec_playground_helper(
    //     code: impl ExampleCode,
    //     visualize_filename: String,
    //     defect_vertices: Vec<VertexIndex>,
    //     final_dual: Weight,
    //     plugins: PluginVec,
    //     growing_strategy: GrowingStrategy,
    // ) -> (
    //     DualModuleInterfacePtr,
    //     PrimalModuleSerial,
    //     impl DualModuleImpl + MWPSVisualizer,
    // ) {
    //     println!("{defect_vertices:?}");
    //     let visualizer = {
    //         let visualizer = Visualizer::new(
    //             Some(visualize_data_folder() + visualize_filename.as_str()),
    //             code.get_positions(),
    //             true,
    //         )
    //         .unwrap();
    //         print_visualize_link(visualize_filename.clone());
    //         visualizer
    //     };

    //     // create dual module
    //     let model_graph = code.get_model_graph();
    //     let initializer = &model_graph.initializer;
    //     let partition_config = graph_time_partition(&initializer, &code.get_positions());
    //     let partition_info = partition_config.info();
    //     let dual_module: DualModuleParallel<DualModuleSerial> =
    //         DualModuleParallel::new_config(&initializer, &partition_info, DualModuleParallelConfig::default());

    //     dual_module_serial_basic_standard_syndrome_optional_viz(
    //         code,
    //         defect_vertices,
    //         final_dual,
    //         plugins,
    //         growing_strategy,
    //         dual_module,
    //         model_graph,
    //         Some(visualizer),
    //     )
    // }

    // /// test a simple case
    // #[test]
    // fn dual_module_parallel_tentative_test_3() {
    //     // RUST_BACKTRACE=1 cargo test dual_module_parallel_tentative_test_3 -- --nocapture
    //     let weight = 1; // do not change, the data is hard-coded
    //     // let pxy = 0.0602828812732227;
    //     let code = CodeCapacityPlanarCode::new(7, 0.1, weight);
    //     // let code = CodeCapacityTailoredCode::new(7, 0., 0.01, 1);
    //     let defect_vertices = vec![3]; // 3, 29 works

    //     let visualize_filename = "dual_module_parallel_tentative_test_3.json".to_string();
    //     dual_module_serial_basic_standard_syndrome(
    //         code,
    //         visualize_filename,
    //         defect_vertices,
    //         4,
    //         vec![],
    //         GrowingStrategy::SingleCluster,
    //     );
    // }

    // #[test]
    // fn dual_module_parallel_evaluation_qec_playground() {
    //     // RUST_BACKTRACE=1 cargo test dual_module_parallel_evaluation_qec_playground -- --nocapture
    //     let config = json!({
    //         "code_type": qecp::code_builder::CodeType::RotatedPlanarCode
    //     });
        
    //     let code = QECPlaygroundCode::new(3, 0.1, config);
    //     let defect_vertices = vec![3, 7];

    //     let visualize_filename = "dual_module_parallel_evaluation_qec_playground.json".to_string();
    //     dual_module_parallel_evaluation_qec_playground_helper(
    //         code,
    //         visualize_filename,
    //         defect_vertices,
    //         4,
    //         vec![],
    //         GrowingStrategy::SingleCluster,
    //     );
    // }

}