//! Relaxer Forest
//!
//! Maintain several lists of relaxers
//!

use crate::invalid_subgraph::*;
use crate::num_traits::Zero;
use crate::relaxer::*;
use crate::util::*;
use num_traits::Signed;
use weak_table::PtrWeakHashSet;
use weak_table::PtrWeakKeyHashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};

pub type RelaxerVec = Vec<Relaxer>;

/// a forest of relaxers that possibly depend on each other
pub struct RelaxerForest {
    /// keep track of the remaining tight edges for quick validation:
    /// these edges cannot grow unless untightened by some relaxers
    tight_edges: BTreeSet<EdgePtr>,
    /// keep track of the subgraphs that are allowed to shrink:
    /// these should be all positive dual variables, all others are yS = 0
    shrinkable_subgraphs: BTreeSet<Arc<InvalidSubgraph>>,
    /// each untightened edge corresponds to a relaxer with speed:
    /// to untighten the edge for a unit length, how much should a relaxer be executed
    edge_untightener: BTreeMap<EdgePtr, (Arc<Relaxer>, Rational)>,
    /// expanded relaxer results, as part of the dynamic programming:
    /// the expanded relaxer is a valid relaxer only growing of initial un-tight edges,
    /// not any edges untightened by other relaxers
    expanded_relaxers: BTreeMap<Arc<Relaxer>, Relaxer>,
}

pub const FOREST_ERR_MSG_GROW_TIGHT_EDGE: &str = "invalid relaxer: try to grow a tight edge";
pub const FOREST_ERR_MSG_UNSHRINKABLE: &str = "invalid relaxer: try to shrink a unshrinkable subgraph";

impl RelaxerForest {
    pub fn new<IterEdge, IterSubgraph>(tight_edges: IterEdge, shrinkable_subgraphs: IterSubgraph) -> Self
    where
        IterEdge: Iterator<Item = EdgeWeak>,
        IterSubgraph: Iterator<Item = Arc<InvalidSubgraph>>,
    {
        Self {
            tight_edges: tight_edges.map(|e| e.upgrade_force()).collect(),
            shrinkable_subgraphs: BTreeSet::from_iter(shrinkable_subgraphs),
            edge_untightener: BTreeMap::new(),
            expanded_relaxers: BTreeMap::new(),
        }
    }

    /// check if the proposed relaxers are indeed relaxers given the edges
    /// untightened by existing relaxers
    pub fn validate(&self, relaxer: &Relaxer) -> Result<(), String> {
        // non-negative overall speed and effectiveness check
        relaxer.sanity_check()?;
        // a relaxer cannot grow any tight edge
        for (edge_ptr, _) in relaxer.get_growing_edges().iter() {
            if self.tight_edges.contains(&edge_ptr) && !self.edge_untightener.contains_key(&edge_ptr) {
                return Err(format!("{FOREST_ERR_MSG_GROW_TIGHT_EDGE}: {:?}", edge_ptr.read_recursive().edge_index));
            }
        }
        // a relaxer cannot shrink any zero dual variable
        for (invalid_subgraph, grow_ratio) in relaxer.get_direction().iter() {
            if grow_ratio.is_negative() && !self.shrinkable_subgraphs.contains(invalid_subgraph) {
                return Err(format!("{FOREST_ERR_MSG_UNSHRINKABLE}: {invalid_subgraph:?}"));
            }
        }
        Ok(())
    }

    /// add a relaxer to the forest
    pub fn add(&mut self, relaxer: Arc<Relaxer>) {
        // validate only at debug mode to improve speed
        debug_assert_eq!(self.validate(&relaxer), Ok(()));
        // add this relaxer to the forest
        for (edge_ptr, speed) in relaxer.get_untighten_edges() {
            debug_assert!(speed.is_negative());
            if !self.edge_untightener.contains_key(&edge_ptr) {
                self.edge_untightener.insert(edge_ptr.clone(), (relaxer.clone(), -speed.recip()));
            }
        }
    }

    fn compute_expanded(&mut self, relaxer: &Arc<Relaxer>) {
        if self.expanded_relaxers.contains_key(relaxer) {
            return;
        }
        let mut untightened_edges: BTreeMap<EdgePtr, Rational> = BTreeMap::new();
        let mut directions: BTreeMap<Arc<InvalidSubgraph>, Rational> = relaxer.get_direction().clone();
        // println!("relaxer.growing_edges: {:?}", relaxer.get_growing_edges());
        for (edge_ptr, speed) in relaxer.get_growing_edges().iter() {
            // println!("edge_ptr index: {:?}", edge_ptr.read_recursive().edge_index);
            // println!("speed: {:?}", speed);
            debug_assert!(speed.is_positive());
            if self.tight_edges.contains(&edge_ptr) {
                debug_assert!(self.edge_untightener.contains_key(&edge_ptr));
                // println!("untightened_edges: {:?}", untightened_edges);
                let require_speed = if let Some(existing_speed) = untightened_edges.get_mut(&edge_ptr) {
                    // println!("existing speed: {:?}", existing_speed);
                    if &*existing_speed >= speed {
                        *existing_speed -= speed;
                        Rational::zero()
                    } else {
                        let required_speed = speed - &*existing_speed;
                        existing_speed.set_zero();
                        required_speed
                    }
                } else {
                    speed.clone()
                };
                // println!("require_speed: {:?}", require_speed);
                if require_speed.is_positive() {
                    // we need to invoke another relaxer to untighten this edge
                    let edge_relaxer = self.edge_untightener.get(&edge_ptr).unwrap().0.clone();
                    self.compute_expanded(&edge_relaxer);
                    // println!("edge_ptr need to find is {:?}", edge_ptr);
                    // println!("self.edge_untightener: {:?}", self.edge_untightener);
                    let (edge_relaxer, speed_ratio) = self.edge_untightener.get(&edge_ptr).unwrap();
                    // println!("edge_relaxer found: {:?}", edge_relaxer);
                    // println!("speed_ratio: {:?}", speed_ratio);
                    debug_assert!(speed_ratio.is_positive());
                    // println!("edge_relaxer: {:?}", edge_relaxer);
                    // println!("self.expanded_relaxers: {:?}", self.expanded_relaxers);
                    let expanded_edge_relaxer = self.expanded_relaxers.get(edge_relaxer).unwrap();
                    for (subgraph, original_speed) in expanded_edge_relaxer.get_direction() {
                        let new_speed = original_speed * speed_ratio;
                        if let Some(speed) = directions.get_mut(subgraph) {
                            *speed += new_speed;
                        } else {
                            directions.insert(subgraph.clone(), new_speed);
                        }
                    }
                    for (edge_index, original_speed) in expanded_edge_relaxer.get_untighten_edges() {
                        debug_assert!(original_speed.is_negative());
                        let new_speed = -original_speed * speed_ratio;
                        // println!("untightened_edges: {:?}", untightened_edges);
                        // println!("edge_index: {:?}", edge_index);
                        // println!("new_speed: {:?}", new_speed);
                        // println!("original_speed: {:?}", original_speed);
                        // println!("speed ratio: {:?}", speed_ratio);
                        if let Some(speed) = untightened_edges.get_mut(&edge_index) {
                            *speed += new_speed;
                        } else {
                            untightened_edges.insert(edge_index.clone(), new_speed);
                        }
                    }
                    // println!("ungithtended_edges final: {:?}", untightened_edges);
                    // println!("left assert: edge ptr: {:?}", edge_ptr);
                    // println!("right assert: require speed: {:?}", require_speed);
                    debug_assert_eq!(untightened_edges.get(&edge_ptr), Some(&require_speed));
                    *untightened_edges.get_mut(&edge_ptr).unwrap() -= require_speed;
                }
            }
        }
        let expanded = Relaxer::new(directions);
        // an expanded relaxer will not rely on any non-tight edges
        debug_assert!(expanded
            .get_growing_edges()
            .iter()
            .all(|(edge_index, _)| !self.tight_edges.contains(&edge_index)));
        self.expanded_relaxers.insert(relaxer.clone(), expanded);
    }

    /// expand a relaxer
    pub fn expand(&mut self, relaxer: &Arc<Relaxer>) -> Relaxer {
        debug_assert_eq!(self.validate(relaxer), Ok(()));
        self.compute_expanded(relaxer);
        self.expanded_relaxers.get(relaxer).unwrap().clone()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use num_traits::{FromPrimitive, One};
    use crate::{pointers::*, relaxer};
    #[cfg(feature = "pq")]
    use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr, Edge, Vertex};
    #[cfg(feature = "non-pq")]
    use crate::dual_module_serial::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr, Edge, Vertex};


    #[test]
    fn relaxer_forest_example() {
        // cargo test relaxer_forest_example -- --nocapture
        // // create vertices 
        // let vertices: Vec<VertexPtr> = (0..parity_checks.len())
        //     .map(|vertex_index| {
        //         VertexPtr::new_value(Vertex {
        //             vertex_index,
        //             is_defect: false,
        //             edges: vec![],
        //         })
        //     })
        //     .collect();

        // create edges
        let edges: Vec<EdgePtr> = (0..11)
            .map(|edge_index| {
                EdgePtr::new_value(Edge {
                    edge_index: edge_index,
                    weight: Rational::zero(),
                    dual_nodes: vec![],
                    vertices: vec![],
                    last_updated_time: Rational::zero(),
                    growth_at_last_updated_time: Rational::zero(),
                    grow_rate: Rational::zero(),
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();

        let mut tight_edges = vec![];
        for edge_index in [0, 1, 2, 3, 4, 5, 6] {
            tight_edges.push(edges[edge_index].downgrade());
        }
        
        let mut local_hair_1 = BTreeSet::new();
        local_hair_1.insert(edges[1].clone());
        local_hair_1.insert(edges[2].clone());
        local_hair_1.insert(edges[3].clone());
        let mut local_hair_2 = BTreeSet::new();
        local_hair_2.insert(edges[4].clone());
        local_hair_2.insert(edges[5].clone());
        let mut local_vertice_1 = BTreeSet::new();
        let mut local_edge_1 = BTreeSet::new();
        let mut local_vertice_2 = BTreeSet::new();
        let mut local_edge_2 = BTreeSet::new();
        let shrinkable_subgraphs = [
            Arc::new(InvalidSubgraph::new_raw(&local_vertice_1, &local_edge_1, &local_hair_1)),
            Arc::new(InvalidSubgraph::new_raw(&local_vertice_2, &local_edge_2, &local_hair_2)),
        ];
        let mut relaxer_forest = RelaxerForest::new(tight_edges.into_iter(), shrinkable_subgraphs.iter().cloned());

        let mut local_hair_3 = BTreeSet::new();
        local_hair_3.insert(edges[7].clone());
        local_hair_3.insert(edges[8].clone());
        local_hair_3.insert(edges[9].clone()); 
        let local_vertice_3 = BTreeSet::new();
        let local_edge_3 = BTreeSet::new();
        let invalid_subgraph_1 = Arc::new(InvalidSubgraph::new_raw(&local_vertice_3, &local_edge_3, &local_hair_3));
        let relaxer_1 = Arc::new(Relaxer::new_raw(
            [
                (invalid_subgraph_1.clone(), Rational::one()),
                (shrinkable_subgraphs[0].clone(), -Rational::one()),
            ]
            .into(),
        ));
        let expanded_1 = relaxer_forest.expand(&relaxer_1);
        assert_eq!(expanded_1, *relaxer_1);
        relaxer_forest.add(relaxer_1);
        // now add a relaxer that is relying on relaxer_1
        let mut local_hair_4 = BTreeSet::new();
        local_hair_4.insert(edges[1].clone());
        local_hair_4.insert(edges[2].clone());
        local_hair_4.insert(edges[7].clone());
        let mut local_vertice_4 = BTreeSet::new();
        let mut local_edge_4 = BTreeSet::new();
        let invalid_subgraph_2 = Arc::new(InvalidSubgraph::new_raw(&local_vertice_4, &local_edge_4, &local_hair_4));
        let relaxer_2 = Arc::new(Relaxer::new_raw([(invalid_subgraph_2.clone(), Rational::one())].into()));
        let expanded_2 = relaxer_forest.expand(&relaxer_2);
        assert_eq!(
            expanded_2,
            Relaxer::new(
                [
                    (invalid_subgraph_1, Rational::one()),
                    (shrinkable_subgraphs[0].clone(), -Rational::one()),
                    (invalid_subgraph_2, Rational::one())
                ]
                .into()
            )
        );
        // println!("{expanded_2:#?}");
    }

    #[test]
    fn relaxer_forest_require_multiple() {
        // cargo test relaxer_forest_require_multiple -- --nocapture
        // create edges
        let edges: Vec<EdgePtr> = (0..11)
            .map(|edge_index| {
                EdgePtr::new_value(Edge {
                    edge_index: edge_index,
                    weight: Rational::zero(),
                    dual_nodes: vec![],
                    vertices: vec![],
                    last_updated_time: Rational::zero(),
                    growth_at_last_updated_time: Rational::zero(),
                    grow_rate: Rational::zero(),
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();

        let mut tight_edges = vec![];
        for edge_index in [0, 1, 2, 3, 4, 5, 6] {
            tight_edges.push(edges[edge_index].downgrade());
        }
        
        let mut local_hair_1 = BTreeSet::new();
        local_hair_1.insert(edges[1].clone());
        local_hair_1.insert(edges[2].clone());
        let mut local_hair_2 = BTreeSet::new();
        local_hair_2.insert(edges[3].clone());
        let mut local_vertice_1 = BTreeSet::new();
        let mut local_edge_1 = BTreeSet::new();
        let mut local_vertice_2 = BTreeSet::new();
        let mut local_edge_2 = BTreeSet::new();

        let shrinkable_subgraphs = [
            Arc::new(InvalidSubgraph::new_raw(&local_vertice_1, &local_edge_1, &local_hair_1)),
            Arc::new(InvalidSubgraph::new_raw(&local_vertice_2, &local_edge_2, &local_hair_2)),
        ];

        // println!("shrinkable_subgraphs: {:?}", shrinkable_subgraphs);
        let mut relaxer_forest = RelaxerForest::new(tight_edges.into_iter(), shrinkable_subgraphs.iter().cloned());

        let mut local_hair_3 = BTreeSet::new();
        local_hair_3.insert(edges[7].clone());
        local_hair_3.insert(edges[8].clone());
        local_hair_3.insert(edges[9].clone()); 
        let local_vertice_3 = BTreeSet::new();
        let local_edge_3 = BTreeSet::new();
        let invalid_subgraph_1 = Arc::new(InvalidSubgraph::new_raw(&local_vertice_3, &local_edge_3, &local_hair_3));
        let relaxer_1 = Arc::new(Relaxer::new_raw(
            [
                (invalid_subgraph_1.clone(), Rational::one()),
                (shrinkable_subgraphs[0].clone(), -Rational::one()),
            ]
            .into(),
        ));
        // println!("relaxer_1: {:?}", relaxer_1);
        relaxer_forest.add(relaxer_1);


        let mut local_hair_4 = BTreeSet::new();
        local_hair_4.insert(edges[1].clone());
        local_hair_4.insert(edges[2].clone());
        local_hair_4.insert(edges[7].clone());
        let mut local_vertice_4 = BTreeSet::new();
        let mut local_edge_4 = BTreeSet::new();
        let invalid_subgraph_2 = Arc::new(InvalidSubgraph::new_raw(&local_vertice_4, &local_edge_4, &local_hair_4));

        let mut local_hair_5 = BTreeSet::new();
        local_hair_5.insert(edges[2].clone());       
        let mut local_vertice_5 = BTreeSet::new();
        let mut local_edge_5 = BTreeSet::new();
        let invalid_subgraph_3 = Arc::new(InvalidSubgraph::new_raw(&local_vertice_5, &local_edge_5, &local_hair_5));
        let relaxer_2 = Arc::new(Relaxer::new_raw(
            [
                (invalid_subgraph_2.clone(), Rational::one()),
                (invalid_subgraph_3.clone(), Rational::one()),
            ]
            .into(),
        ));
        let expanded_2 = relaxer_forest.expand(&relaxer_2);
        let intended_relaxer = Relaxer::new(
            [
                (invalid_subgraph_2, Rational::one()),
                (invalid_subgraph_3, Rational::one()),
                (invalid_subgraph_1, Rational::from_usize(2).unwrap()),
                (shrinkable_subgraphs[0].clone(), -Rational::from_usize(2).unwrap()),
            ]
            .into()
        );
        println!("expanded_2: {:?}", expanded_2);
        println!("intended relaxer: {:?}", intended_relaxer);
        assert_eq!(
            expanded_2,
            intended_relaxer
        );
        // println!("{expanded_2:#?}");
    }

    // #[test]
    // fn relaxer_forest_relaxing_same_edge() {
    //     // cargo test relaxer_forest_relaxing_same_edge -- --nocapture
    //     let tight_edges = [0, 1, 2, 3, 4, 5, 6];
    //     let shrinkable_subgraphs = [
    //         Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [1, 2].into())),
    //         Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [2, 3].into())),
    //     ];
    //     let mut relaxer_forest = RelaxerForest::new(tight_edges.into_iter(), shrinkable_subgraphs.iter().cloned());
    //     let invalid_subgraph_1 = Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [7, 8, 9].into()));
    //     let relaxer_1 = Arc::new(Relaxer::new_raw(
    //         [
    //             (invalid_subgraph_1.clone(), Rational::one()),
    //             (shrinkable_subgraphs[0].clone(), -Rational::one()),
    //         ]
    //         .into(),
    //     ));
    //     relaxer_forest.add(relaxer_1);
    //     let invalid_subgraph_2 = Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [10, 11].into()));
    //     let relaxer_2 = Arc::new(Relaxer::new_raw(
    //         [
    //             (invalid_subgraph_2.clone(), Rational::one()),
    //             (shrinkable_subgraphs[1].clone(), -Rational::one()),
    //         ]
    //         .into(),
    //     ));
    //     relaxer_forest.add(relaxer_2);
    // }

    // #[test]
    // fn relaxer_forest_validate() {
    //     // cargo test relaxer_forest_validate -- --nocapture
    //     let tight_edges = [0, 1, 2, 3, 4, 5, 6];
    //     let shrinkable_subgraphs = [
    //         Arc::new(InvalidSubgraph::new_raw([1].into(), [].into(), [1, 2].into())),
    //         Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [].into())),
    //     ];
    //     let relaxer_forest = RelaxerForest::new(tight_edges.into_iter(), shrinkable_subgraphs.iter().cloned());
    //     println!("relaxer_forest: {:?}", relaxer_forest.shrinkable_subgraphs);
    //     // invalid relaxer is forbidden
    //     let invalid_relaxer = Relaxer::new_raw(
    //         [(
    //             Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [].into())),
    //             -Rational::one(),
    //         )]
    //         .into(),
    //     );
    //     let error_message = relaxer_forest.validate(&invalid_relaxer).expect_err("should panic");
    //     assert_eq!(
    //         &error_message[..RELAXER_ERR_MSG_NEGATIVE_SUMMATION.len()],
    //         RELAXER_ERR_MSG_NEGATIVE_SUMMATION
    //     );
    //     // relaxer that increases a tight edge is forbidden
    //     let relaxer = Relaxer::new_raw(
    //         [(
    //             Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [1].into())),
    //             Rational::one(),
    //         )]
    //         .into(),
    //     );
    //     let error_message = relaxer_forest.validate(&relaxer).expect_err("should panic");
    //     assert_eq!(
    //         &error_message[..FOREST_ERR_MSG_GROW_TIGHT_EDGE.len()],
    //         FOREST_ERR_MSG_GROW_TIGHT_EDGE
    //     );
    //     // relaxer that shrinks a zero dual variable is forbidden
    //     let relaxer = Relaxer::new_raw(
    //         [
    //             (
    //                 Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [9].into())),
    //                 Rational::one(),
    //             ),
    //             (
    //                 Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [2, 3].into())),
    //                 -Rational::one(),
    //             ),
    //         ]
    //         .into(),
    //     );
    //     let error_message = relaxer_forest.validate(&relaxer).expect_err("should panic");
    //     assert_eq!(
    //         &error_message[..FOREST_ERR_MSG_UNSHRINKABLE.len()],
    //         FOREST_ERR_MSG_UNSHRINKABLE
    //     );
    //     // otherwise a relaxer is ok
    //     let relaxer = Relaxer::new_raw(
    //         [(
    //             Arc::new(InvalidSubgraph::new_raw([].into(), [].into(), [9].into())),
    //             Rational::one(),
    //         )]
    //         .into(),
    //     );
    //     relaxer_forest.validate(&relaxer).unwrap();
    // }
}
