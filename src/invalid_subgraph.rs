use crate::decoding_hypergraph::*;
use crate::derivative::Derivative;
use crate::dual_module::DualModuleImpl;
use crate::matrix::*;
use crate::plugin::EchelonMatrix;
use crate::util::*;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak};


/// an invalid subgraph $S = (V_S, E_S)$, also store the hair $\delta(S)$
#[derive(Clone, PartialEq, Eq, Derivative)]
#[derivative(Debug)]
pub struct InvalidSubgraph {
    /// the hash value calculated by other fields
    #[derivative(Debug = "ignore")]
    pub hash_value: u64,
    /// subset of vertex weak pointers, nota that the vertex struct is from dual_module_pq
    pub vertices: BTreeSet<VertexPtr>,
    /// subset of edge weak pointers, note that the edge struct is from dual_module_pq
    pub edges: BTreeSet<EdgePtr>,
    /// the hair of the invalid subgraph, to avoid repeated computation
    pub hair: BTreeSet<EdgePtr>,
}

impl Hash for InvalidSubgraph {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash_value.hash(state);
    }
}

impl Ord for InvalidSubgraph {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.hash_value != other.hash_value {
            self.hash_value.cmp(&other.hash_value)
        } else if self == other {
            Ordering::Equal
        } else {
            // rare cases: same hash value but different state
            (&self.vertices, &self.edges, &self.hair).cmp(&(&other.vertices, &other.edges, &other.hair))
            // // Compare vertices, then edges, then hair
            // let vertices_cmp = self.vertices.iter().cmp(other.vertices.iter());
            // if vertices_cmp != Ordering::Equal {
            //     return vertices_cmp;
            // }

            // let edges_cmp = self.edges.iter().cmp(other.edges.iter());
            // if edges_cmp != Ordering::Equal {
            //     return edges_cmp;
            // }

            // self.hair.iter().cmp(other.hair.iter())
        }
    }
}

impl PartialOrd for InvalidSubgraph {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl InvalidSubgraph {
    /// construct an invalid subgraph using only $E_S$, and constructing the $V_S$ by $\cup E_S$ for given dual_module
    /// the invalid subgraph generated is a local graph if the decoding_graph is a local graph
    /// delete decoding_graph: &DecodingHyperGraph when release, it is here merely to run sanity_check()
    #[allow(clippy::unnecessary_cast)]
    pub fn new(edges: &BTreeSet<EdgePtr>) -> Self {
        // println!("edges input: {:?}", edges);
        let mut vertices: BTreeSet<VertexPtr> = BTreeSet::new();
        for edge_ptr in edges.iter() {
            for vertex_ptr in edge_ptr.read_recursive().vertices.iter() {
                vertices.insert(vertex_ptr.upgrade_force().clone());
            }
        }
        // println!("vertices: {:?}", vertices);
        // for vertex in vertices.iter() {
        //     let vertex_index = vertex.read_recursive().vertex_index;
        // }
        Self::new_complete(&vertices, edges)
    }

    /// complete definition of invalid subgraph $S = (V_S, E_S)$
    #[allow(clippy::unnecessary_cast)]
    pub fn new_complete(
        vertices: &BTreeSet<VertexPtr>,
        edges: &BTreeSet<EdgePtr>
    ) -> Self {
        // current implementation with using helper function 
        // println!("input vertex to new_complete: {:?}", vertices);
        let mut hair: BTreeSet<EdgePtr> = BTreeSet::new();
        for vertex_ptr in vertices.iter() {
            // println!("vertex index in new_complete: {:?}", vertex_ptr.read_recursive().vertex_index);
            for edge_ptr in vertex_ptr.get_edge_neighbors().iter() {
                // println!("edges near vertex {:?}", edge_ptr.upgrade_force().read_recursive().edge_index);
                if !edges.contains(&edge_ptr.upgrade_force()) {
                    hair.insert(edge_ptr.upgrade_force());
                }
            }
        }
        let invalid_subgraph = Self::new_raw(vertices, edges, &hair);
        // debug_assert_eq!(invalid_subgraph.sanity_check(decoding_graph), Ok(()));
        invalid_subgraph

        // previous implementation with directly finding the incident edges of a vertex
        // // println!("input vertex to new_complete: {:?}", vertices);
        // let mut hair: BTreeSet<EdgePtr> = BTreeSet::new();
        // for vertex_ptr in vertices.iter() {
        //     // println!("vertex index in new_complete: {:?}", vertex_ptr.read_recursive().vertex_index);
        //     for edge_ptr in vertex_ptr.read_recursive().edges.iter() {
        //         // println!("edges near vertex {:?}", edge_ptr.upgrade_force().read_recursive().edge_index);
        //         if !edges.contains(&edge_ptr.upgrade_force()) {
        //             hair.insert(edge_ptr.upgrade_force());
        //         }
        //     }
        // }
        // let invalid_subgraph = Self::new_raw(vertices, edges, &hair);
        // // debug_assert_eq!(invalid_subgraph.sanity_check(decoding_graph), Ok(()));
        // invalid_subgraph
    }

    /// create $S = (V_S, E_S)$ and $\delta(S)$ directly, without any checks
    pub fn new_raw(vertices: &BTreeSet<VertexPtr>, edges: &BTreeSet<EdgePtr>, hair: &BTreeSet<EdgePtr>) -> Self {
        let mut invalid_subgraph = Self {
            hash_value: 0,
            vertices: vertices.clone(),
            edges: edges.clone(),
            hair: hair.clone(),
        };
        invalid_subgraph.update_hash();
        invalid_subgraph
    }

    pub fn update_hash(&mut self) {
        let mut hasher = DefaultHasher::new();
        // let _ = self.vertices.iter().map(|e|e.hash(&mut hasher));
        // let _ = self.edges.iter().map(|e|e.hash(&mut hasher));
        // let _ = self.hair.iter().map(|e|e.hash(&mut hasher));
        self.vertices.hash(&mut hasher);
        self.edges.hash(&mut hasher);
        self.hair.hash(&mut hasher);
        self.hash_value = hasher.finish();
    }

    // check whether this invalid subgraph is indeed invalid, this is costly and should be disabled in release runs
    #[allow(clippy::unnecessary_cast)]
    pub fn sanity_check(&self, decoding_graph: &DecodingHyperGraph) -> Result<(), String> {
        if self.vertices.is_empty() {
            return Err("an invalid subgraph must contain at least one vertex".to_string());
        }
        // check if all vertices are valid
        for vertex_ptr in self.vertices.iter() {
            let vertex_index = vertex_ptr.read_recursive().vertex_index;
            if vertex_index >= decoding_graph.model_graph.initializer.vertex_num {
                return Err(format!("vertex {vertex_index} is not a vertex in the model graph"));
            }
        }
        // check if every edge is subset of its vertices
        for edge_ptr in self.edges.iter() {
            let edge = edge_ptr.read_recursive();
            let edge_index = edge.edge_index;
            if edge_index as usize >= decoding_graph.model_graph.initializer.weighted_edges.len() {
                return Err(format!("edge {edge_index} is not an edge in the model graph"));
            }
            // let hyperedge = &decoding_graph.model_graph.initializer.weighted_edges[edge_index as usize];
            for vertex_weak in edge.vertices.iter() {
                if !self.vertices.contains(&vertex_weak.upgrade_force()) {
                    return Err(format!(
                        "hyperedge {edge_index} connects vertices {:?}, \
                    but vertex {:?} is not in the invalid subgraph vertices {:?}", 
                        edge.vertices, vertex_weak.upgrade_force().read_recursive().vertex_index, self.vertices
                    ));
                }
            }
        }
        // check the edges indeed cannot satisfy the requirement of the vertices
        let mut matrix = Echelon::<CompleteMatrix>::new();
        for edge_ptr in self.edges.iter() {
            matrix.add_variable(edge_ptr.downgrade());
        }
        for vertex_ptr in self.vertices.iter() {
            let vertex = vertex_ptr.read_recursive();
            let incident_edges = &vertex.edges;
            let parity = vertex.is_defect;
            matrix.add_constraint(vertex_ptr.downgrade(), &incident_edges, parity);
        }
        if matrix.get_echelon_info().satisfiable {
            let temp = matrix.get_solution().unwrap().into_iter().map(|e| e.upgrade_force().read_recursive().edge_index).collect::<Vec<_>>();
            return Err(format!(
                "it's a valid subgraph because edges {:?} ⊆ {:?} can satisfy the parity requirement from vertices {:?}",
                temp,
                self.edges.iter().map(|e| e.upgradable_read().edge_index).collect::<Vec<_>>(),
                self.vertices.iter().map(|e| e.upgradable_read().vertex_index).collect::<Vec<_>>(),
            ));
        }
        Ok(())
    }

    pub fn generate_matrix(&self) -> EchelonMatrix {
        let mut matrix = EchelonMatrix::new();
        for edge_ptr in self.hair.iter() {
            matrix.add_variable(edge_ptr.downgrade());
        }
        for vertex_ptr in self.vertices.iter() {
            let vertex = vertex_ptr.read_recursive();
            let incident_edges = &vertex.edges;
            let parity = vertex.is_defect;
            matrix.add_constraint(vertex_ptr.downgrade(), &incident_edges, parity);
        }
        matrix
    }
}

// shortcuts for easier code writing at debugging
impl InvalidSubgraph {
    pub fn new_ptr(edges: &BTreeSet<EdgePtr>) -> Arc<Self> {
        Arc::new(Self::new(edges))
    }
    pub fn new_vec_ptr(edges: &[EdgePtr]) -> Arc<Self> {
        let strong_edges: BTreeSet<EdgePtr> = edges.iter().cloned().collect();
        Self::new_ptr(&strong_edges)
    }
    pub fn new_complete_ptr(
        vertices: &BTreeSet<VertexPtr>,
        edges: &BTreeSet<EdgePtr>
    ) -> Arc<Self> {
        Arc::new(Self::new_complete(vertices, edges))
    }
    pub fn new_complete_vec_ptr(
        vertices: &BTreeSet<VertexPtr>,
        edges: &[EdgePtr],
    ) -> Arc<Self> {
        // let strong_edges = edges.iter()
        // .filter_map(|weak_edge| weak_edge.upgrade())
        // .collect();
        let strong_edges: BTreeSet<EdgePtr> = edges.iter().cloned().collect();
        Self::new_complete_ptr(
            vertices,
            &strong_edges
        )
    }
}

/// below are the original test based on indices, now we cannot test invalid subgraph alone since any invalid subgraph requires
/// the VertexPtr and EdgePtr created at the initialization of dual_module_pq. 

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::decoding_hypergraph::tests::*;
    use num_traits::Zero;
    use crate::dual_module_pq::{EdgePtr, Edge, VertexPtr, Vertex};
    use crate::pointers::*;
    use crate::num_traits::FromPrimitive;
    use std::collections::HashSet;

    #[test]
    fn invalid_subgraph_good() {
        // cargo test invalid_subgraph_good -- --nocapture
        let visualize_filename = "invalid_subgraph_good.json".to_string();
        let (decoding_graph, ..) = color_code_5_decoding_graph(vec![7, 1], visualize_filename);
        let initializer = &decoding_graph.model_graph.initializer;
        // create vertices
        let vertices: Vec<VertexPtr> = (0..initializer.vertex_num)
            .map(|vertex_index| {
                VertexPtr::new_value(Vertex {
                    vertex_index,
                    is_defect: false,
                    edges: vec![],
                    is_mirror: false,
                    fusion_done: false,
                    mirrored_vertices: vec![],
                })
            })
            .collect();
        // set defect vertices 
        vertices[7].write().is_defect = true;
        vertices[1].write().is_defect = true;

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
                unit_index: None,
                #[cfg(feature = "incr_lp")]
                cluster_weights: hashbrown::HashMap::new(),
            });
            for &vertex_index in hyperedge.vertices.iter() {
                vertices[vertex_index as usize].write().edges.push(edge_ptr.downgrade());
            }
            edges.push(edge_ptr);
        }

        let mut invalid_subgraph_edges = BTreeSet::new();
        invalid_subgraph_edges.insert(edges[13].clone());

        let invalid_subgraph_1 = InvalidSubgraph::new(&invalid_subgraph_edges);
        println!("invalid_subgraph_1: {invalid_subgraph_1:?}");

        let temp_vertices: HashSet<_> = invalid_subgraph_1.vertices.into_iter().map(|v| v.read_recursive().vertex_index).collect();
        let temp_edges: HashSet<_> = invalid_subgraph_1.edges.into_iter().map(|e| e.read_recursive().edge_index).collect();
        let temp_hair: HashSet<_> = invalid_subgraph_1.hair.into_iter().map(|e| e.read_recursive().edge_index).collect();

        assert_eq!(temp_vertices, [2, 6, 7].into());
        assert_eq!(temp_edges, [13].into());
        assert_eq!(
            temp_hair,
            [5, 6, 9, 10, 11, 12, 14, 15, 16, 17].into()
        );
    }

//     #[test]
//     #[should_panic]
//     fn invalid_subgraph_bad() {
//         // cargo test invalid_subgraph_bad -- --nocapture
//         let visualize_filename = "invalid_subgraph_bad.json".to_string();
//         let (decoding_graph, ..) = color_code_5_decoding_graph(vec![7, 1], visualize_filename);
//         let invalid_subgraph = InvalidSubgraph::new(vec![6, 10].into_iter().collect(), decoding_graph.as_ref());
//         println!("invalid_subgraph: {invalid_subgraph:?}"); // should not print because it panics
//     }

//     pub fn get_default_hash_value(object: &impl Hash) -> u64 {
//         let mut hasher = DefaultHasher::new();
//         object.hash(&mut hasher);
//         hasher.finish()
//     }

//     #[test]
//     fn invalid_subgraph_hash() {
//         // cargo test invalid_subgraph_hash -- --nocapture
//         let vertices: BTreeSet<VertexIndex> = [1, 2, 3].into();
//         let edges: BTreeSet<EdgeIndex> = [4, 5].into();
//         let hair: BTreeSet<EdgeIndex> = [6, 7, 8].into();
//         let invalid_subgraph_1 = InvalidSubgraph::new_raw(vertices.clone(), edges.clone(), hair.clone());
//         let invalid_subgraph_2 = InvalidSubgraph::new_raw(vertices.clone(), edges.clone(), hair.clone());
//         assert_eq!(invalid_subgraph_1, invalid_subgraph_2);
//         // they should have the same hash value
//         assert_eq!(
//             get_default_hash_value(&invalid_subgraph_1),
//             get_default_hash_value(&invalid_subgraph_1.hash_value)
//         );
//         assert_eq!(
//             get_default_hash_value(&invalid_subgraph_1),
//             get_default_hash_value(&invalid_subgraph_2)
//         );
//         // the pointer should also have the same hash value
//         let ptr_1 = Arc::new(invalid_subgraph_1.clone());
//         let ptr_2 = Arc::new(invalid_subgraph_2);
//         assert_eq!(get_default_hash_value(&ptr_1), get_default_hash_value(&ptr_1.hash_value));
//         assert_eq!(get_default_hash_value(&ptr_1), get_default_hash_value(&ptr_2));
//         // any different value would generate a different invalid subgraph
//         assert_ne!(
//             invalid_subgraph_1,
//             InvalidSubgraph::new_raw([1, 2].into(), edges.clone(), hair.clone())
//         );
//         assert_ne!(
//             invalid_subgraph_1,
//             InvalidSubgraph::new_raw(vertices.clone(), [4, 5, 6].into(), hair.clone())
//         );
//         assert_ne!(
//             invalid_subgraph_1,
//             InvalidSubgraph::new_raw(vertices.clone(), edges.clone(), [6, 7].into())
//         );
//     }
}
