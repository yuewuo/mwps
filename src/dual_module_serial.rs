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
    #[derivative(Debug = "ignore")]
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
        for (vertex_indices, weight) in initializer.weighted_edges.iter() {
            let edge_ptr = EdgePtr::new_value(Edge {
                edge_index: edges.len() as EdgeIndex,
                growth: Rational::zero(),
                weight: Rational::from_usize(*weight).unwrap(),
                dual_nodes: vec![],
                vertices: vertex_indices
                    .iter()
                    .map(|i| vertices[*i].downgrade())
                    .collect::<Vec<_>>(),
                grow_rate: Rational::zero(),
            });
            for &vertex_index in vertex_indices.iter() {
                vertices[vertex_index]
                    .write()
                    .edges
                    .push(edge_ptr.downgrade());
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
        // make sure the active edges are set
        let dual_node_weak = dual_node_ptr.downgrade();
        let dual_node = dual_node_ptr.read_recursive();
        if dual_node.invalid_subgraph.edges.is_empty() {
            assert!(
                dual_node.invalid_subgraph.vertices.len() == 1,
                "defect node (without edges) should only work on a single vertex, for simplicity"
            );
            let vertex_index = dual_node.invalid_subgraph.vertices.iter().next().unwrap();
            let mut vertex = self.vertices[*vertex_index].write();
            assert!(!vertex.is_defect, "defect should not be added twice");
            vertex.is_defect = true;
        }
        for &edge_index in dual_node.invalid_subgraph.hairs.iter() {
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
    }

    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        let mut dual_node = dual_node_ptr.write();
        let grow_rate_diff = grow_rate.clone() - &dual_node.grow_rate;
        dual_node.grow_rate = grow_rate;
        drop(dual_node);
        let dual_node = dual_node_ptr.read_recursive();
        for &edge_index in dual_node.invalid_subgraph.hairs.iter() {
            let mut edge = self.edges[edge_index].write();
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

    #[allow(clippy::collapsible_else_if)]
    fn compute_maximum_update_length_dual_node(
        &mut self,
        dual_node_ptr: &DualNodePtr,
        simultaneous_update: bool,
    ) -> MaxUpdateLength {
        let node = dual_node_ptr.read_recursive();
        let mut max_update_length = MaxUpdateLength::new();
        for &edge_index in node.invalid_subgraph.hairs.iter() {
            let edge = self.edges[edge_index].read_recursive();
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
                        max_update_length
                            .merge(MaxUpdateLength::ShrinkProhibited(dual_node_ptr.clone()));
                    } else {
                        // find a negatively growing edge
                        let mut found = false;
                        for node_weak in edge.dual_nodes.iter() {
                            let node_ptr = node_weak.upgrade_force();
                            if node_ptr.read_recursive().grow_rate.is_negative() {
                                max_update_length
                                    .merge(MaxUpdateLength::ShrinkProhibited(node_ptr));
                                found = true;
                                break;
                            }
                        }
                        assert!(found, "unreachable");
                    }
                } else {
                    max_update_length
                        .merge(MaxUpdateLength::ValidGrow(-edge.growth.clone() / grow_rate));
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
                if node.grow_rate.is_negative() {
                    // y >= 0
                    group_max_update_length.add(MaxUpdateLength::ValidGrow(
                        -node.dual_variable.clone() / node.grow_rate.clone(),
                    ));
                }
                grow_rate += node.grow_rate.clone();
            }
            if grow_rate.is_positive() {
                let edge_remain = edge.weight.clone() - edge.growth.clone();
                if edge_remain.is_zero() {
                    group_max_update_length.add(MaxUpdateLength::Conflicting(edge_index));
                } else {
                    group_max_update_length
                        .add(MaxUpdateLength::ValidGrow(edge_remain / grow_rate));
                }
            } else if grow_rate.is_negative() {
                if edge.growth.is_zero() {
                    // it will be reported when iterating active dual nodes
                } else {
                    group_max_update_length
                        .add(MaxUpdateLength::ValidGrow(-edge.growth.clone() / grow_rate));
                }
            }
        }
        for node_ptr in self.active_nodes.iter() {
            let node = node_ptr.read_recursive();
            if node.grow_rate.is_negative() {
                if node.dual_variable.is_positive() {
                    group_max_update_length.add(MaxUpdateLength::ValidGrow(
                        -node.dual_variable.clone() / node.grow_rate.clone(),
                    ));
                } else {
                    group_max_update_length
                        .add(MaxUpdateLength::ShrinkProhibited(node_ptr.clone()));
                }
            }
        }
        group_max_update_length
    }

    fn grow_dual_node(&mut self, dual_node_ptr: &DualNodePtr, length: Rational) {
        if length.is_zero() {
            eprintln!("[warning] calling `grow_dual_node` with zero length, nothing to do");
            return;
        }
        let node = dual_node_ptr.read_recursive();
        let grow_amount = length * node.grow_rate.clone();
        for &edge_index in node.invalid_subgraph.hairs.iter() {
            let mut edge = self.edges[edge_index].write();
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
        dual_node_ptr.write().dual_variable += grow_amount;
    }

    fn grow(&mut self, length: Rational) {
        debug_assert!(
            length.is_positive(),
            "growth should be positive; if desired, please set grow rate to negative for shrinking"
        );
        // update the active edges
        for &edge_index in self.active_edges.iter() {
            let mut edge = self.edges[edge_index].write();
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
            node.dual_variable =
                node.dual_variable.clone() + length.clone() * node.grow_rate.clone();
        }
    }

    fn get_edge_nodes(&self, edge_index: EdgeIndex) -> Vec<DualNodePtr> {
        self.edges[edge_index]
            .read_recursive()
            .dual_nodes
            .iter()
            .map(|x| x.upgrade_force())
            .collect()
    }

    fn is_edge_tight(&self, edge_index: EdgeIndex) -> bool {
        let edge = self.edges[edge_index].read_recursive();
        edge.growth == edge.weight
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
    use super::super::example_codes::*;
    use super::*;
    use crate::framework::*;

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
        let decoding_graph = HyperDecodingGraph::new_defects(model_graph, vec![3, 12]);
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
        let subgraph = Subgraph::new(vec![15, 20]);
        visualizer
            .snapshot_combined(
                "subgraph".to_string(),
                vec![&interface_ptr, &dual_module, &subgraph],
            )
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
        let decoding_graph = HyperDecodingGraph::new_defects(model_graph, vec![23, 24, 29, 30]);
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
        let subgraph = Subgraph::new(vec![24]);
        visualizer
            .snapshot_combined(
                "subgraph".to_string(),
                vec![&interface_ptr, &dual_module, &subgraph],
            )
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
        let decoding_graph = HyperDecodingGraph::new_defects(model_graph, vec![17, 23, 29, 30]);
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
        dual_module.grow_dual_node(
            &dual_node_bigger_cluster_ptr,
            Rational::from_i64(120).unwrap(),
        );
        visualizer
            .snapshot_combined("solved".to_string(), vec![&interface_ptr, &dual_module])
            .unwrap();
        // the result subgraph
        let subgraph = Subgraph::new(vec![82, 24]);
        visualizer
            .snapshot_combined(
                "subgraph".to_string(),
                vec![&interface_ptr, &dual_module, &subgraph],
            )
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
        let decoding_graph = HyperDecodingGraph::new_defects(model_graph, vec![3, 12]);
        let interface_ptr =
            DualModuleInterfacePtr::new_load(decoding_graph.clone(), &mut dual_module);
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
            .snapshot_combined(
                "subgraph".to_string(),
                vec![&interface_ptr, &dual_module, &subgraph],
            )
            .unwrap();
    }
}
