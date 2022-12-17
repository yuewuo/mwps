//! Serial Dual Module
//! 
//! A serial implementation of the dual module
//!

use std::collections::{BTreeSet, BTreeMap};
use num_traits::FromPrimitive;
use crate::util::*;
use crate::num_traits::sign::Signed;
use crate::num_traits::{Zero, ToPrimitive};
use crate::derivative::Derivative;
use crate::dual_module::*;
use crate::visualize::*;
use crate::pointers::*;
use crate::matrix_util::*;


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
    #[derivative(Debug="ignore")]
    pub edges: Vec<EdgeWeak>,
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
    #[derivative(Debug="ignore")]
    pub vertices: Vec<VertexWeak>,
    /// growth value, growth <= weight
    pub growth: Rational,
    /// the dual nodes that contributes to this edge
    pub dual_nodes: Vec<DualNodeWeak>,
}

pub type EdgePtr = ArcRwLock<Edge>;
pub type EdgeWeak = WeakRwLock<Edge>;

impl std::fmt::Debug for EdgePtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let edge = self.read_recursive();
        write!(f, "{}", edge.edge_index)
    }
}

impl std::fmt::Debug for EdgeWeak {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let edge_ptr = self.upgrade_force();
        let edge = edge_ptr.read_recursive();
        write!(f, "{}", edge.edge_index)
    }
}

impl DualModuleImpl for DualModuleSerial {

    /// initialize the dual module, which is supposed to be reused for multiple decoding tasks with the same structure
    fn new_empty(initializer: &SolverInitializer) -> Self {
        initializer.sanity_check().unwrap();
        // create vertices
        let vertices: Vec<VertexPtr> = (0..initializer.vertex_num).map(|vertex_index| VertexPtr::new_value(Vertex {
            vertex_index,
            is_defect: false,
            edges: vec![],
        })).collect();
        // set edges
        let mut edges = Vec::<EdgePtr>::new();
        for (vertex_indices, weight) in initializer.weighted_edges.iter() {
            let edge_ptr = EdgePtr::new_value(Edge {
                edge_index: edges.len() as EdgeIndex,
                growth: Rational::zero(),
                weight: Rational::from_usize(*weight).unwrap(),
                dual_nodes: vec![],
                vertices: vertex_indices.iter().map(|i| vertices[*i].downgrade()).collect::<Vec<_>>(),
            });
            for &vertex_index in vertex_indices.iter() {
                vertices[vertex_index].write().edges.push(edge_ptr.downgrade());
            }
            edges.push(edge_ptr);
        }
        Self {
            vertices,
            edges,
            active_edges: BTreeSet::new(),
            active_nodes: BTreeSet::new(),
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

    fn add_dual_node(&mut self, dual_node_ptr: &DualNodePtr) {
        let mut node = dual_node_ptr.write();
        assert!(!node.internal_edges.is_empty() || !node.internal_vertices.is_empty(), "invalid dual node");
        if node.internal_edges.is_empty() {
            assert!(node.internal_vertices.len() == 1, "defect node (without internal edges) should only work on a single vertex, for simplicity");
            let vertex_index = node.internal_vertices.iter().next().unwrap();
            let mut vertex = self.vertices[*vertex_index].write();
            assert!(!vertex.is_defect, "defect should not be added twice");
            vertex.is_defect = true;
        } else {
            debug_assert!(!self.is_valid_cluster(&node.internal_edges), "cannot create dual node out of a valid cluster");
            if node.internal_vertices.is_empty() {
                let mut internal_vertices = BTreeSet::new();
                // fill in with all vertices incident to the internal edges
                for &edge_index in node.internal_edges.iter() {
                    let edge = self.edges[edge_index].read_recursive();
                    for vertex_weak in edge.vertices.iter() {
                        internal_vertices.insert(vertex_weak.upgrade_force().read_recursive().vertex_index);
                    }
                }
                std::mem::swap(&mut node.internal_vertices, &mut internal_vertices);
            }
        }
        // calculate hair edges
        let mut hair_edges = BTreeSet::new();
        for &vertex_index in node.internal_vertices.iter() {
            let vertex = self.vertices[vertex_index].read_recursive();
            for edge_weak in vertex.edges.iter() {
                let edges_index = edge_weak.upgrade_force().read_recursive().edge_index;
                if !node.internal_edges.contains(&edges_index) {
                    hair_edges.insert(edges_index);
                }
            }
        }
        for &edge_index in hair_edges.iter() {
            let mut edge = self.edges[edge_index].write();
            edge.dual_nodes.push(dual_node_ptr.downgrade());
        }
        assert!(node.hair_edges.is_empty(), "hair edge should not be provided");
        std::mem::swap(&mut node.hair_edges, &mut hair_edges);
        let grow_rate = node.grow_rate.clone();
        drop(node);
        self.set_grow_rate(dual_node_ptr, grow_rate);  // make sure the active edges are set
    }

    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        let mut dual_node = dual_node_ptr.write();
        dual_node.grow_rate = grow_rate;
        drop(dual_node);
        let dual_node = dual_node_ptr.read_recursive();
        for &edge_index in dual_node.hair_edges.iter() {
            let edge = self.edges[edge_index].read_recursive();
            let mut aggregated_grow_rate = Rational::zero();
            for node_weak in edge.dual_nodes.iter() {
                let node_ptr = node_weak.upgrade_force();
                aggregated_grow_rate += node_ptr.read_recursive().grow_rate.clone();
            }
            if aggregated_grow_rate.is_zero() {
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

    #[allow(clippy::collapsible_else_if)]
    fn compute_maximum_update_length_dual_node(&mut self, dual_node_ptr: &DualNodePtr, simultaneous_update: bool) -> MaxUpdateLength {
        let node = dual_node_ptr.read_recursive();
        let mut max_update_length = MaxUpdateLength::new();
        for &edge_index in node.hair_edges.iter() {
            let edge = self.edges[edge_index].read_recursive();
            let mut grow_rate = Rational::zero();
            if simultaneous_update {  // consider all dual nodes
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
                                break
                            }
                        }
                        assert!(found, "unreachable");
                    }
                } else {
                    max_update_length.merge(MaxUpdateLength::ValidGrow(- edge.growth.clone() / grow_rate));
                }
            }
        }
        max_update_length
    }

    fn compute_maximum_update_length(&mut self) -> GroupMaxUpdateLength {
        let mut group_max_update_length = GroupMaxUpdateLength::new();
        for &edge_index in self.active_edges.iter() {
            let edge = self.edges[edge_index].read_recursive();
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
                    group_max_update_length.add(MaxUpdateLength::ValidGrow(- edge.growth.clone() / grow_rate));
                }
            }
        }
        for node_ptr in self.active_nodes.iter() {
            let node = node_ptr.read_recursive();
            if node.grow_rate.is_negative() && !node.dual_variable.is_positive() {
                group_max_update_length.add(MaxUpdateLength::ShrinkProhibited(node_ptr.clone()));
            }
        }
        group_max_update_length
    }

    fn grow_dual_node(&mut self, dual_node_ptr: &DualNodePtr, length: Rational) {
        if length.is_zero() {
            eprintln!("[warning] calling `grow_dual_node` with zero length, nothing to do");
            return
        }
        let node = dual_node_ptr.read_recursive();
        let grow_amount = length * node.grow_rate.clone();
        for &edge_index in node.hair_edges.iter() {
            let mut edge = self.edges[edge_index].write();
            edge.growth += grow_amount.clone();
            assert!(!edge.growth.is_negative(), "edge {} over-shrunk: the new growth is {:?}", edge_index, edge.growth);
            assert!(edge.growth <= edge.weight, "edge {} over-grown: the new growth is {:?}, weight is {:?}", edge_index, edge.growth, edge.weight);
        }
        drop(node);
        // update dual variable
        dual_node_ptr.write().dual_variable += grow_amount;
    }

    fn grow(&mut self, length: Rational) {
        debug_assert!(length.is_positive(), "growth should be positive; if desired, please set grow rate to negative for shrinking");
        // update the active edges
        for &edge_index in self.active_edges.iter() {
            let mut edge = self.edges[edge_index].write();
            let mut grow_rate = Rational::zero();
            for node_weak in edge.dual_nodes.iter() {
                grow_rate += node_weak.upgrade_force().read_recursive().grow_rate.clone();
            }
            edge.growth += length.clone() * grow_rate;
            assert!(!edge.growth.is_negative(), "edge {} over-shrunk: the new growth is {:?}", edge_index, edge.growth);
            assert!(edge.growth <= edge.weight, "edge {} over-grown: the new growth is {:?}, weight is {:?}", edge_index, edge.growth, edge.weight);
        }
        // update dual variables
        for node_ptr in self.active_nodes.iter() {
            let mut node = node_ptr.write();
            node.dual_variable = node.dual_variable.clone() + length.clone() * node.grow_rate.clone();
        }
    }

    fn find_valid_subgraph(&self, internal_edges: &BTreeSet<EdgeIndex>) -> Option<Subgraph> {
        assert!(!internal_edges.is_empty(), "finding subgraph without any internal edges is infeasible");
        let mut internal_vertices = BTreeSet::new();
        let mut variable_indices = BTreeMap::new();  // edge_index -> variable_index
        let mut edge_indices = vec![];  // variable_index -> edge_index
        // fill in with all vertices incident to the internal edges
        for (variable_index, &edge_index) in internal_edges.iter().enumerate() {
            edge_indices.push(edge_index);
            variable_indices.insert(edge_index, variable_index);
            let edge = self.edges[edge_index].read_recursive();
            for vertex_weak in edge.vertices.iter() {
                internal_vertices.insert(vertex_weak.upgrade_force().read_recursive().vertex_index);
            }
        }
        // use Gaussian elimination on a modular 2 linear system (i.e. there is only 0 and 1 elements)
        let height = internal_vertices.len();  // number of constraints
        let width = internal_edges.len() + 1;  // number of variables
        let mut matrix = Vec::<Vec<u8>>::with_capacity(height);
        for &vertex_index in internal_vertices.iter() {
            let mut row = vec![0; width];
            let vertex = self.vertices[vertex_index].read_recursive();
            for edge_weak in vertex.edges.iter() {
                let edge_index = edge_weak.upgrade_force().read_recursive().edge_index;
                if internal_edges.contains(&edge_index) {
                    row[variable_indices[&edge_index]] = 1;
                }
            }
            row[width - 1] = if vertex.is_defect { 1 } else { 0 };
            matrix.push(row);
        }
        modular_2_row_echelon_form(&mut matrix);
        // find the first non-zero value on the right column from bottom to top
        for i in (0..height).rev() {
            if matrix[i][width - 1] != 0 {
                // check if all the previous elements are 0, if so then it's unsolvable and thus invalid
                let mut all_zero = true;
                let row = &matrix[i];
                for j in 0..width-1 {
                    if row[j] == 1 {
                        all_zero = false;
                        break
                    }
                }
                if all_zero {
                    return None;
                }
                break
            }
        }
        let mut subgraph_edges = vec![];
        let mut lead = 0;
        for i in 0..height {
            let row = &matrix[i];
            while row[lead] == 0 {
                if lead >= width-2 {
                    break  // cannot find a lead element
                }
                lead += 1;
            }
            if row[lead] == 0 {
                break  // cannot find a lead element
            }
            if row[width - 1] == 1 {
                subgraph_edges.push(edge_indices[lead]);
            }
        }
        Some(Subgraph::new(subgraph_edges))
    }
    
    fn get_edge_nodes(&self, edge_index: EdgeIndex) -> Vec<DualNodePtr> {
        self.edges[edge_index].read_recursive().dual_nodes.iter().map(|x| x.upgrade_force()).collect()
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
        let mut vertices: Vec::<serde_json::Value> = vec![];
        for vertex_ptr in self.vertices.iter() {
            let vertex = vertex_ptr.read_recursive();
            vertices.push(json!({
                if abbrev { "s" } else { "is_defect" }: i32::from(vertex.is_defect),
            }));
        }
        let mut edges: Vec::<serde_json::Value> = vec![];
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
    use super::super::example_codes::*;

    #[test]
    fn dual_module_serial_basics_1() {  // cargo test dual_module_serial_basics_1 -- --nocapture
        let visualize_filename = format!("dual_module_serial_basics_1.json");
        let weight = 1000;
        let mut code = CodeCapacityColorCode::new(7, 0.1, weight);
        let mut visualizer = Visualizer::new(Some(visualize_data_folder() + visualize_filename.as_str()), code.get_positions(), true).unwrap();
        print_visualize_link(visualize_filename.clone());
        // create dual module
        let initializer = code.get_initializer();
        let mut dual_module = DualModuleSerial::new_empty(&initializer);
        // try to work on a simple syndrome
        code.vertices[3].is_defect = true;
        code.vertices[12].is_defect = true;
        let interface_ptr = DualModuleInterfacePtr::new_load(&code.get_syndrome(), &mut dual_module);
        visualizer.snapshot_combined(format!("syndrome"), vec![&interface_ptr, &dual_module]).unwrap();
        // grow them each by half
        let dual_node_3_ptr = interface_ptr.read_recursive().nodes[0].clone();
        let dual_node_12_ptr = interface_ptr.read_recursive().nodes[1].clone();
        dual_module.grow_dual_node(&dual_node_3_ptr, Rational::from_usize(weight/2).unwrap());
        dual_module.grow_dual_node(&dual_node_12_ptr, Rational::from_usize(weight/2).unwrap());
        visualizer.snapshot_combined(format!("grow"), vec![&interface_ptr, &dual_module]).unwrap();
        // cluster becomes solved
        dual_module.grow_dual_node(&dual_node_3_ptr, Rational::from_usize(weight/2).unwrap());
        dual_module.grow_dual_node(&dual_node_12_ptr, Rational::from_usize(weight/2).unwrap());
        visualizer.snapshot_combined(format!("solved"), vec![&interface_ptr, &dual_module]).unwrap();
        // the result subgraph
        let subgraph = Subgraph::new(vec![15, 20]);
        visualizer.snapshot_combined(format!("subgraph"), vec![&interface_ptr, &dual_module, &subgraph]).unwrap();
    }

    #[test]
    fn dual_module_serial_basics_2() {  // cargo test dual_module_serial_basics_2 -- --nocapture
        let visualize_filename = format!("dual_module_serial_basics_2.json");
        let weight = 1000;
        let mut code = CodeCapacityTailoredCode::new(7, 0., 0.1, weight);
        let mut visualizer = Visualizer::new(Some(visualize_data_folder() + visualize_filename.as_str()), code.get_positions(), true).unwrap();
        print_visualize_link(visualize_filename.clone());
        // create dual module
        let initializer = code.get_initializer();
        let mut dual_module = DualModuleSerial::new_empty(&initializer);
        // try to work on a simple syndrome
        code.vertices[23].is_defect = true;
        code.vertices[24].is_defect = true;
        code.vertices[29].is_defect = true;
        code.vertices[30].is_defect = true;
        let interface_ptr = DualModuleInterfacePtr::new_load(&code.get_syndrome(), &mut dual_module);
        visualizer.snapshot_combined(format!("syndrome"), vec![&interface_ptr, &dual_module]).unwrap();
        // grow them each by half
        let dual_node_23_ptr = interface_ptr.read_recursive().nodes[0].clone();
        let dual_node_24_ptr = interface_ptr.read_recursive().nodes[1].clone();
        let dual_node_29_ptr = interface_ptr.read_recursive().nodes[2].clone();
        let dual_node_30_ptr = interface_ptr.read_recursive().nodes[3].clone();
        dual_module.grow_dual_node(&dual_node_23_ptr, Rational::from_usize(weight/4).unwrap());
        dual_module.grow_dual_node(&dual_node_24_ptr, Rational::from_usize(weight/4).unwrap());
        dual_module.grow_dual_node(&dual_node_29_ptr, Rational::from_usize(weight/4).unwrap());
        dual_module.grow_dual_node(&dual_node_30_ptr, Rational::from_usize(weight/4).unwrap());
        visualizer.snapshot_combined(format!("solved"), vec![&interface_ptr, &dual_module]).unwrap();
        // the result subgraph
        let subgraph = Subgraph::new(vec![24]);
        visualizer.snapshot_combined(format!("subgraph"), vec![&interface_ptr, &dual_module, &subgraph]).unwrap();
    }

    #[test]
    fn dual_module_serial_basics_3() {  // cargo test dual_module_serial_basics_3 -- --nocapture
        let visualize_filename = format!("dual_module_serial_basics_3.json");
        let weight = 600;  // do not change, the data is hard-coded
        let pxy = 0.0602828812732227;
        let mut code = CodeCapacityTailoredCode::new(7, pxy, 0.1, weight);  // do not change probabilities: the data is hard-coded
        let mut visualizer = Visualizer::new(Some(visualize_data_folder() + visualize_filename.as_str()), code.get_positions(), true).unwrap();
        print_visualize_link(visualize_filename.clone());
        // create dual module
        let initializer = code.get_initializer();
        let mut dual_module = DualModuleSerial::new_empty(&initializer);
        // try to work on a simple syndrome
        code.vertices[17].is_defect = true;
        code.vertices[23].is_defect = true;
        code.vertices[29].is_defect = true;
        code.vertices[30].is_defect = true;
        let interface_ptr = DualModuleInterfacePtr::new_load(&code.get_syndrome(), &mut dual_module);
        visualizer.snapshot_combined(format!("syndrome"), vec![&interface_ptr, &dual_module]).unwrap();
        // grow them each by half
        let dual_node_17_ptr = interface_ptr.read_recursive().nodes[0].clone();
        let dual_node_23_ptr = interface_ptr.read_recursive().nodes[1].clone();
        let dual_node_29_ptr = interface_ptr.read_recursive().nodes[2].clone();
        let dual_node_30_ptr = interface_ptr.read_recursive().nodes[3].clone();
        dual_module.grow_dual_node(&dual_node_17_ptr, Rational::from_i64(160).unwrap());
        dual_module.grow_dual_node(&dual_node_23_ptr, Rational::from_i64(160).unwrap());
        dual_module.grow_dual_node(&dual_node_29_ptr, Rational::from_i64(160).unwrap());
        dual_module.grow_dual_node(&dual_node_30_ptr, Rational::from_i64(160).unwrap());
        visualizer.snapshot_combined(format!("grow"), vec![&interface_ptr, &dual_module]).unwrap();
        // create cluster
        interface_ptr.create_cluster_node(vec![24].into_iter().collect(), &mut dual_module);
        let dual_node_cluster_ptr = interface_ptr.read_recursive().nodes[4].clone();
        dual_module.grow_dual_node(&dual_node_17_ptr, Rational::from_i64(160).unwrap());
        dual_module.grow_dual_node(&dual_node_cluster_ptr, Rational::from_i64(160).unwrap());
        visualizer.snapshot_combined(format!("grow"), vec![&interface_ptr, &dual_module]).unwrap();
        // create bigger cluster
        interface_ptr.create_cluster_node(vec![18, 23, 24, 31].into_iter().collect(), &mut dual_module);
        let dual_node_bigger_cluster_ptr = interface_ptr.read_recursive().nodes[5].clone();
        dual_module.grow_dual_node(&dual_node_bigger_cluster_ptr, Rational::from_i64(120).unwrap());
        visualizer.snapshot_combined(format!("solved"), vec![&interface_ptr, &dual_module]).unwrap();
        // the result subgraph
        let subgraph = Subgraph::new(vec![82, 24]);
        visualizer.snapshot_combined(format!("subgraph"), vec![&interface_ptr, &dual_module, &subgraph]).unwrap();
    }

    #[test]
    fn dual_module_serial_find_valid_subgraph_1() {  // cargo test dual_module_serial_find_valid_subgraph_1 -- --nocapture
        let visualize_filename = format!("dual_module_serial_find_valid_subgraph_1.json");
        let weight = 1000;
        let mut code = CodeCapacityColorCode::new(7, 0.1, weight);
        let mut visualizer = Visualizer::new(Some(visualize_data_folder() + visualize_filename.as_str()), code.get_positions(), true).unwrap();
        print_visualize_link(visualize_filename.clone());
        // create dual module
        let initializer = code.get_initializer();
        let mut dual_module = DualModuleSerial::new_empty(&initializer);
        // try to work on a simple syndrome
        code.vertices[3].is_defect = true;
        code.vertices[12].is_defect = true;
        let interface_ptr = DualModuleInterfacePtr::new_load(&code.get_syndrome(), &mut dual_module);
        visualizer.snapshot_combined(format!("syndrome"), vec![&interface_ptr, &dual_module]).unwrap();
        // invalid clusters
        assert!(!dual_module.is_valid_cluster(&vec![20].into_iter().collect()));
        assert!(!dual_module.is_valid_cluster(&vec![9, 20].into_iter().collect()));
        assert!(!dual_module.is_valid_cluster(&vec![15].into_iter().collect()));
        assert!(dual_module.is_valid_cluster(&vec![15, 20].into_iter().collect()));
        // the result subgraph
        let subgraph = dual_module.find_valid_subgraph(&vec![9, 15, 20, 21].into_iter().collect()).unwrap();
        visualizer.snapshot_combined(format!("subgraph"), vec![&interface_ptr, &dual_module, &subgraph]).unwrap();
    }

}
