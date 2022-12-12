//! Serial Dual Module
//! 
//! A serial implementation of the dual module
//!

use std::collections::BTreeSet;
use std::ops::Mul;

use num_traits::FromPrimitive;

use crate::util::*;
use crate::num_traits::sign::Signed;
use crate::num_traits::{Zero, ToPrimitive};
use crate::derivative::Derivative;
use crate::dual_module::*;
use crate::visualize::*;
use crate::pointers::*;


pub struct DualModuleSerial {
    /// all vertices including virtual ones
    pub vertices: Vec<VertexPtr>,
    /// keep edges, which can also be accessed in [`Self::vertices`]
    pub edges: Vec<EdgePtr>,
    /// maintain an active list to optimize for average cases: most defect vertices have already been matched, and we only need to work on a few remained;
    /// note that this list may contain duplicate nodes
    pub active_edges: BTreeSet<EdgeWeak>,
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
                weight: Rational::from_i64(*weight).unwrap(),
                dual_nodes: vec![],
                vertices: vertex_indices.iter().map(|i| vertices[*i].downgrade()).collect::<Vec<_>>(),
            });
            for vertex_index in vertex_indices.iter() {
                vertices[*vertex_index].write().edges.push(edge_ptr.downgrade());
            }
            edges.push(edge_ptr);
        }
        Self {
            vertices,
            edges,
            active_edges: BTreeSet::new(),
        }
    }

    /// clear all growth and existing dual nodes
    fn clear(&mut self) {
        self.active_edges.clear();
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
            for vertex_index in node.internal_vertices.iter() {
                let mut vertex = self.vertices[*vertex_index].write();
                assert!(!vertex.is_defect, "defect should not be added twice");
                vertex.is_defect = true;
            }
        } else {
            if node.internal_vertices.is_empty() {
                let mut internal_vertices = BTreeSet::new();
                // fill in with all vertices incident to the internal edges
                for edge_index in node.internal_edges.iter() {
                    let edge = self.edges[*edge_index].read_recursive();
                    for vertex_weak in edge.vertices.iter() {
                        internal_vertices.insert(vertex_weak.upgrade_force().read_recursive().vertex_index);
                    }
                }
                std::mem::swap(&mut node.internal_vertices, &mut internal_vertices);
            }
        }
        // calculate hair edges
        let mut hair_edges = BTreeSet::new();
        for vertex_index in node.internal_vertices.iter() {
            let vertex = self.vertices[*vertex_index].read_recursive();
            for edge_weak in vertex.edges.iter() {
                let edges_index = edge_weak.upgrade_force().read_recursive().edge_index;
                if !node.internal_edges.contains(&edges_index) {
                    hair_edges.insert(edges_index);
                }
            }
        }
        assert!(node.hair_edges.is_empty(), "hair edge should not be provided");
        std::mem::swap(&mut node.hair_edges, &mut hair_edges);
        let grow_rate = node.grow_rate.clone();
        drop(node);
        self.set_grow_rate(dual_node_ptr, grow_rate);  // make sure the active edges are set
    }

    fn set_grow_rate(&mut self, dual_node_ptr: &DualNodePtr, grow_rate: Rational) {
        let dual_node = dual_node_ptr.write();
        
    }

    #[allow(clippy::collapsible_else_if)]
    fn compute_maximum_update_length_dual_node(&mut self, dual_node_ptr: &DualNodePtr, is_grow: bool, simultaneous_update: bool) -> MaxUpdateLength {
        MaxUpdateLength::Unbounded
    }

    fn compute_maximum_update_length(&mut self) -> GroupMaxUpdateLength {
        let mut group_max_update_length = GroupMaxUpdateLength::new();

        group_max_update_length
    }

    fn grow_dual_node(&mut self, dual_node_ptr: &DualNodePtr, length: Rational) {
        if length.is_zero() {
            eprintln!("[warning] calling `grow_dual_node` with zero length, nothing to do");
            return
        }
        let node = dual_node_ptr.read_recursive();
        let grow_amount = length * node.grow_rate.clone();
        for edge_index in node.hair_edges.iter() {
            let mut edge = self.edges[*edge_index].write();
            edge.growth += grow_amount.clone();
            assert!(!edge.growth.is_negative(), "edge over-shrunk: the new growth is {:?}", edge.growth);
            assert!(edge.growth <= edge.weight, "edge over-grown: the new growth is {:?}, weight is {:?}", edge.growth, edge.weight);
        }
    }

    fn grow(&mut self, mut length: Rational) {
        debug_assert!(length.is_positive(), "only positive growth is supported");
        // TODO
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
            edges.push(json!({
                if abbrev { "w" } else { "weight" }: edge.weight.to_f64(),
                if abbrev { "v" } else { "vertices" }: edge.vertices.iter().map(|x| x.upgrade_force().read_recursive().vertex_index).collect::<Vec<_>>(),
                if abbrev { "g" } else { "growth" }: edge.growth.to_f64(),
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
        let half_weight = 500;
        let mut code = CodeCapacityColorCode::new(7, 0.1, half_weight * 2);
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
        dual_module.grow_dual_node(&dual_node_3_ptr, Rational::from_i64(half_weight).unwrap());
        dual_module.grow_dual_node(&dual_node_12_ptr, Rational::from_i64(half_weight).unwrap());
        visualizer.snapshot_combined(format!("grow"), vec![&interface_ptr, &dual_module]).unwrap();
        // cluster becomes solved
        dual_module.grow_dual_node(&dual_node_3_ptr, Rational::from_i64(half_weight).unwrap());
        dual_module.grow_dual_node(&dual_node_12_ptr, Rational::from_i64(half_weight).unwrap());
        visualizer.snapshot_combined(format!("solved"), vec![&interface_ptr, &dual_module]).unwrap();
    }

}
