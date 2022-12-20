//! Explore how to design the algorithm
//! 
//! This module will not be a part of the library.
//! I'll try several cases and practice myself to solve MWPS problems (or to figure out why it's impossible to solve it...).
//! After that I'll generalize the method and write code to automatically find MWPS
//! 

use prettytable::*;
use crate::util::*;
use crate::dual_module::*;
use std::collections::{BTreeMap, BTreeSet};
use crate::matrix_util::*;

#[derive(Clone, Debug)]
/// manipulate the constraints to get more information about how to create new dual variables
pub struct ExploreParityConstraints {
    /// the cluster we're building the constraints
    pub cluster: ExploreCluster,
    /// the edges that corresponds to the variables
    pub variable_edges: Vec<EdgeIndex>,
    /// the number of non-hair edges that should be dependent variable as much as possible
    pub num_non_hair_edges: usize,
    /// the vertices that correspond to the (initial) constraints;
    /// but after we adjust the constraints, they will not just corresponds to the vertices
    pub constraint_vertices: Vec<VertexIndex>,
    /// the constraints
    pub constraints: Vec<Vec<u8>>,
    /// whether the constraints have been modified, if so don't print constraint vertices
    pub is_initial_constraints: bool,
}

#[derive(Clone, Debug)]
pub struct ExploreCluster {
    /// all the edges that are fully grown in the cluster
    pub grown_edges: BTreeSet<EdgeIndex>,
    /// all the vertices incident to the grown edges
    pub touched_vertices: BTreeSet<VertexIndex>,
}

impl ExploreCluster {

    pub fn new(node_ptr: &DualNodePtr, dual_module: &mut impl DualModuleImpl) -> Self {
        // this is not correct: should consider every node that has been encountered!
        let node = node_ptr.read_recursive();
        let mut grown_edges = BTreeSet::<EdgeIndex>::new();
        let mut newly_touched_vertices = BTreeSet::<VertexIndex>::new();
        for &edge_index in node.internal_edges.iter().chain(node.hair_edges.iter()) {
            if dual_module.is_edge_tight(edge_index) {
                grown_edges.insert(edge_index);
                newly_touched_vertices.extend(dual_module.get_edge_neighbors(edge_index));
            }
        }
        let mut all_touched_vertices = newly_touched_vertices.clone();
        while !newly_touched_vertices.is_empty() {
            let mut new_touched_vertices = BTreeSet::new();
            for &vertex_index in newly_touched_vertices.iter() {
                let edges = dual_module.get_vertex_neighbors(vertex_index);
                for &edge_index in edges.iter() {
                    if !grown_edges.contains(&edge_index) && dual_module.is_edge_tight(edge_index) {
                        grown_edges.insert(edge_index);
                        new_touched_vertices.extend(dual_module.get_edge_neighbors(edge_index));
                    }
                }
            }
            all_touched_vertices.extend(&new_touched_vertices);
            std::mem::swap(&mut newly_touched_vertices, &mut new_touched_vertices);
        }
        Self {
            grown_edges,
            touched_vertices: all_touched_vertices,
        }
    }

    pub fn edges_excluding(&self, excluded_edges: &Vec<EdgeIndex>) -> Vec<EdgeIndex> {
        let mut excluded_edges_set = BTreeSet::new();
        excluded_edges_set.extend(excluded_edges.iter().cloned());
        let mut edges = vec![];
        for &edge_index in self.grown_edges.iter() {
            if !excluded_edges_set.contains(&edge_index) {
                edges.push(edge_index);
            }
        }
        edges
    }

    pub fn is_valid(&self, dual_module: &mut impl DualModuleImpl) -> bool {
        dual_module.is_valid_cluster(&self.grown_edges.iter().cloned().collect())
    }

}

impl ExploreParityConstraints {

    /// it will first find all the connected cluster, and put the hair edges of this node at the end so that
    ///  the row echelon form automatically make most of those hair edges independent variables
    pub fn new(node_ptr: &DualNodePtr, dual_module: &mut impl DualModuleImpl) -> Self {
        let cluster = ExploreCluster::new(node_ptr, dual_module);
        let node = node_ptr.read_recursive();
        // create variables for all the edges that are not hair edges of this dual node
        assert!(cluster.grown_edges.len() > 0, "cannot create constraints with no variable");
        let mut variable_edges = Vec::with_capacity(cluster.grown_edges.len());
        let mut local_indices = BTreeMap::<EdgeIndex, usize>::new();
        for &edge_index in cluster.grown_edges.iter() {
            if !node.hair_edges.contains(&edge_index) {
                local_indices.insert(edge_index, variable_edges.len());
                variable_edges.push(edge_index);
            }
        }
        let num_non_hair_edges = variable_edges.len();
        for &edge_index in node.hair_edges.iter() {
            if cluster.grown_edges.contains(&edge_index) {
                local_indices.insert(edge_index, variable_edges.len());
                variable_edges.push(edge_index);
            }
        }
        let mut constraints = Vec::with_capacity(cluster.touched_vertices.len());
        for &vertex_index in cluster.touched_vertices.iter() {
            let is_defect = dual_module.is_vertex_defect(vertex_index);
            let edges = dual_module.get_vertex_neighbors(vertex_index);
            let mut constraint = vec![0; variable_edges.len()];
            for &edge_index in edges.iter() {
                if let Some(local_index) = local_indices.get(&edge_index) {
                    constraint[*local_index] = 1;
                }
            }
            constraint.push(if is_defect { 1 } else { 0 });
            constraints.push(constraint);
        }
        Self {
            variable_edges: variable_edges,
            num_non_hair_edges: num_non_hair_edges,
            constraint_vertices: cluster.touched_vertices.iter().cloned().collect(),
            constraints: constraints,
            is_initial_constraints: true,
            cluster: cluster,
        }
    }

    /// only support print to std directly
    pub fn print(&self) {
        let mut table = Table::new();
        let table_format = table.get_format();
        table_format.padding(0, 0);
        table_format.column_separator('\u{254E}');
        let mut title_row = Row::empty();
        for (local_idx, edge_index) in self.variable_edges.iter().enumerate() {
            if local_idx == self.num_non_hair_edges {
                title_row.add_cell(Cell::new("+"));
            }
            title_row.add_cell(Cell::new(format!("{edge_index}").as_str()));
        }
        title_row.add_cell(Cell::new(format!("=").as_str()));
        if self.is_initial_constraints {
            title_row.add_cell(Cell::new(format!("vertex").as_str()));
        }
        table.set_titles(title_row);
        for (idx, constraint) in self.constraints.iter().enumerate() {
            let mut row = Row::empty();
            for (local_idx, v) in constraint.iter().enumerate() {
                if local_idx == self.num_non_hair_edges {
                    row.add_cell(Cell::new("+"));
                }
                row.add_cell(Cell::new(if *v == 0 { " " } else { "1" }));
            }
            if self.is_initial_constraints {
                row.add_cell(Cell::new(format!("{}", self.constraint_vertices[idx]).as_str()));
            }
            table.add_row(row);
        }
        println!("{table}");
    }

    pub fn to_row_echelon_form(&mut self) {
        modular_2_row_echelon_form(&mut self.constraints);
        self.is_initial_constraints = false;
    }

    /// find a small set of hair edges so that removing them will invalidate the existence of a solution
    fn echelon_form_find_necessary_hair_edge_set(&self, con_idx_start: usize) -> Vec<EdgeIndex> {
        for con_idx in (con_idx_start..self.constraints.len()).rev() {
            let target = self.constraints[con_idx][self.variable_edges.len()];
            if target == 1 {
                let constraint = &self.constraints[con_idx];
                let mut necessary_hair_edges = vec![];
                for var_idx in self.num_non_hair_edges..self.variable_edges.len() {
                    if constraint[var_idx] == 1 {
                        necessary_hair_edges.push(self.variable_edges[var_idx]);
                    }
                }
                return necessary_hair_edges
            }
        }
        unreachable!("the hair is not required");
    }

    pub fn get_single_hair_solution_or_necessary_edge_set(&mut self) -> Result<Vec<EdgeIndex>, Vec<EdgeIndex>> {
        self.to_row_echelon_form();
        // ignore those non-hair edges and focus on the hair edges, is there a single edge variable that can 
        assert!(self.num_non_hair_edges < self.variable_edges.len(), "there is no hair edge");
        let con_idx_start = if self.num_non_hair_edges == 0 {
            0
        } else {  // as long as the last non-hair edge is zero, it should be considered
            let mut one_con_idx = self.constraints.len() - 1;
            while self.constraints[one_con_idx][self.num_non_hair_edges-1] == 0 && one_con_idx > 0 {
                one_con_idx -= 1;
            }
            if self.constraints[one_con_idx][self.num_non_hair_edges-1] == 0 {
                0
            } else {
                one_con_idx + 1
            }
        };
        let mut var_idx_candidates = BTreeSet::new();
        for var_idx in self.num_non_hair_edges..self.variable_edges.len() {
            var_idx_candidates.insert(var_idx);
        }
        for con_idx in con_idx_start..self.constraints.len() {
            let target = self.constraints[con_idx][self.variable_edges.len()];
            let mut new_var_idx_candidates = BTreeSet::new();
            let constraint = &self.constraints[con_idx];
            for &var_idx in var_idx_candidates.iter() {
                if constraint[var_idx] == target {
                    new_var_idx_candidates.insert(var_idx);
                }
            }
            if new_var_idx_candidates.is_empty() {
                return Err(self.echelon_form_find_necessary_hair_edge_set(con_idx_start))
            }
            std::mem::swap(&mut new_var_idx_candidates, &mut var_idx_candidates);
        }
        Ok(var_idx_candidates.into_iter().map(|var_idx| self.variable_edges[var_idx]).collect())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::example_codes::*;
    use crate::dual_module_serial::*;
    use crate::visualize::*;
    use crate::num_traits::{FromPrimitive, ToPrimitive, Zero, One};

    pub fn take_snapshot(visualizer: &mut Option<Visualizer>, name: String, interface_ptr: &DualModuleInterfacePtr, dual_module: &DualModuleSerial) {
        if let Some(visualizer) = visualizer.as_mut() {
            visualizer.snapshot_combined(name, vec![interface_ptr, dual_module]).unwrap();
        }
    }

    pub fn explore_primal_module_method<F>(mut code: impl ExampleCode, visualize_filename: Option<String>
            , defect_vertices: Vec<VertexIndex>, final_dual: Weight, mut func: F) where F: FnMut(&DualModuleInterfacePtr, &mut DualModuleSerial, &mut Option<Visualizer>) -> Subgraph {
        let mut visualizer = match visualize_filename.as_ref() {
            Some(visualize_filename) => {
                let visualizer = Visualizer::new(Some(visualize_data_folder() + visualize_filename.as_str()), code.get_positions(), true).unwrap();
                print_visualize_link(visualize_filename.clone());
                Some(visualizer)
            }, None => None
        };
        // create dual module
        let initializer = code.get_initializer();
        let mut dual_module = DualModuleSerial::new_empty(&initializer);
        // try to work on a simple syndrome
        code.set_defect_vertices(&defect_vertices);
        let interface_ptr = DualModuleInterfacePtr::new_empty();
        interface_ptr.load(&code.get_syndrome(), &mut dual_module);
        take_snapshot(&mut visualizer, "initial".to_string(), &interface_ptr, &dual_module);
        // manual operations
        let subgraph = func(&interface_ptr, &mut dual_module, &mut visualizer);
        // verify result
        let weight_range = WeightRange::new(interface_ptr.sum_dual_variables(), Rational::from_usize(initializer.get_subgraph_total_weight(&subgraph)).unwrap());
        if let Some(visualizer) = visualizer.as_mut() {
            visualizer.snapshot_combined("perfect matching and subgraph".to_string(), vec![&interface_ptr, &dual_module, &subgraph, &weight_range]).unwrap();
        }
        assert!(initializer.matches_subgraph_syndrome(&subgraph, &defect_vertices), "the result subgraph is invalid");
        assert_eq!(Rational::from_usize(final_dual).unwrap(), weight_range.upper, "unmatched sum dual variables");
        assert_eq!(Rational::from_usize(final_dual).unwrap(), weight_range.lower, "unexpected final dual variable sum");
        println!("weight range: [{}, {}]", weight_range.lower.to_i64().unwrap(), weight_range.upper.to_i64().unwrap());
    }

    /// test a simple case
    #[test]
    fn explore_primal_module_1() {  // cargo test explore_primal_module_1 -- --nocapture
        let visualize_filename = format!("explore_primal_module_1.json");
        let defect_vertices = vec![3, 12];
        let code = CodeCapacityColorCode::new(7, 0.01, 1);
        explore_primal_module_method(code, Some(visualize_filename), defect_vertices, 2, |interface_ptr, dual_module, visualizer| {
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            println!("{group_max_update_length:?}");
            Subgraph::new(vec![15, 20])
        });
    }

    #[test]
    fn explore_primal_module_2() {  // cargo test explore_primal_module_2 -- --nocapture
        let visualize_filename = format!("explore_primal_module_2.json");
        let mut code = CodeCapacityTailoredCode::new(11, 0., 0.01, 1);
        code.apply_errors(&[49, 59]);
        let defect_vertices = code.get_syndrome().defect_vertices;
        explore_primal_module_method(code, Some(visualize_filename), defect_vertices, 2, |interface_ptr, dual_module, visualizer| {
            // use single grow mode
            for i in 1..6 {
                dual_module.set_grow_rate(&interface_ptr.get_node(i).unwrap(), Rational::zero());
            }
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // second stage
            dual_module.set_grow_rate(&interface_ptr.get_node(0).unwrap(), Rational::zero());
            interface_ptr.create_cluster_node(vec![37, 38, 48, 49].into_iter().collect(), dual_module);
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // then analyze what is the final subgraph
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(6).unwrap(), dual_module);
            constraints.print();
            constraints.to_row_echelon_form();
            constraints.print();
            Subgraph::new(vec![49, 59])
        });
    }

    #[test]
    fn explore_primal_module_3() {  // cargo test explore_primal_module_3 -- --nocapture
        let visualize_filename = format!("explore_primal_module_3.json");
        let mut code = CodeCapacityTailoredCode::new(11, 0., 0.01, 1);
        code.apply_errors(&[60, 61, 71, 72]);
        let defect_vertices = code.get_syndrome().defect_vertices;
        explore_primal_module_method(code, Some(visualize_filename), defect_vertices, 4, |interface_ptr, dual_module, visualizer| {
            // use single grow mode
            for i in 1..4 {
                dual_module.set_grow_rate(&interface_ptr.get_node(i).unwrap(), Rational::zero());
            }
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // second stage
            dual_module.set_grow_rate(&interface_ptr.get_node(0).unwrap(), Rational::zero());
            interface_ptr.create_cluster_node(vec![48, 49, 59, 60].into_iter().collect(), dual_module);
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // then analyze what is the final subgraph
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(4).unwrap(), dual_module);
            constraints.to_row_echelon_form(); constraints.print();
            let single_hair_solution = constraints.get_single_hair_solution_or_necessary_edge_set();
            println!("single hair solution: {:?}", single_hair_solution);
            let internal_edges = constraints.cluster.edges_excluding(&single_hair_solution.unwrap_err());
            println!("internal_edges: {internal_edges:?}");
            // no hair edge can alone satisfy all the parity requirements, thus we find a minimum set of edges that are not satisfiable
            // now we try to add as many hair edges to internal edges as possible to make it invalid cluster
            println!("valid: {}", dual_module.is_valid_cluster(&internal_edges.iter().cloned().collect()));
            interface_ptr.create_cluster_node(internal_edges.iter().cloned().collect(), dual_module);
            dual_module.set_grow_rate(&interface_ptr.get_node(4).unwrap(), -Rational::one());
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // now the new fully-grown edges are not valid cluster, so we can grow them safely
            dual_module.set_grow_rate(&interface_ptr.get_node(5).unwrap(), Rational::zero());
            dual_module.set_grow_rate(&interface_ptr.get_node(4).unwrap(), Rational::zero());
            let cluster = ExploreCluster::new(&interface_ptr.get_node(5).unwrap(), dual_module);
            let internal_edges = cluster.grown_edges.iter().cloned().collect();
            println!("internal_edges: {internal_edges:?}");
            println!("valid: {}", dual_module.is_valid_cluster(&internal_edges));
            interface_ptr.create_cluster_node(internal_edges.iter().cloned().collect(), dual_module);
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // analyze again for those non-zero dual variables
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(6).unwrap(), dual_module);
            constraints.to_row_echelon_form(); constraints.print();
            let single_hair_solution = constraints.get_single_hair_solution_or_necessary_edge_set();
            println!("single hair solution: {:?}", single_hair_solution);
            let internal_edges = constraints.cluster.edges_excluding(&single_hair_solution.unwrap_err());
            println!("internal_edges: {internal_edges:?}");
            // again, there is no single hair edge that can satisfy the requirement
            println!("valid: {}", dual_module.is_valid_cluster(&internal_edges.iter().cloned().collect()));
            interface_ptr.create_cluster_node(internal_edges.iter().cloned().collect(), dual_module);
            dual_module.set_grow_rate(&interface_ptr.get_node(6).unwrap(), -Rational::one());
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // then the fully grown edges are not valid cluster, grow it
            let cluster = ExploreCluster::new(&interface_ptr.get_node(6).unwrap(), dual_module);
            let internal_edges = cluster.grown_edges.iter().cloned().collect();
            println!("internal_edges: {internal_edges:?}");
            println!("valid: {}", dual_module.is_valid_cluster(&internal_edges));
            dual_module.set_grow_rate(&interface_ptr.get_node(6).unwrap(), Rational::zero());
            dual_module.set_grow_rate(&interface_ptr.get_node(7).unwrap(), Rational::zero());
            interface_ptr.create_cluster_node(internal_edges.iter().cloned().collect(), dual_module);
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // now every non-zero dual variable can solve a single edge
            let mut subgraph_edges = vec![];
            for node_index in [0, 5, 7, 8] {
                let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(node_index).unwrap(), dual_module);
                let single_hair_solution = constraints.get_single_hair_solution_or_necessary_edge_set();
                println!("single hair solution: {:?}", single_hair_solution);
                let single_hair_edges = single_hair_solution.unwrap();
                assert!(single_hair_edges.len() == 1, "haven't thought about how to handle multiple hair edge cases...");
                subgraph_edges.push(single_hair_edges[0])
            }
            Subgraph::new(subgraph_edges)
        });
    }

    #[test]
    fn explore_primal_module_4() {  // cargo test explore_primal_module_4 -- --nocapture
        let visualize_filename = format!("explore_primal_module_4.json");
        let mut code = CodeCapacityTailoredCode::new(11, 0., 0.01, 1);
        code.apply_errors(&[48, 50, 60, 70, 72]);
        let defect_vertices = code.get_syndrome().defect_vertices;
        explore_primal_module_method(code, Some(visualize_filename), defect_vertices, 5, |interface_ptr, dual_module, visualizer| {
            // use single grow mode
            for i in 1..12 {
                dual_module.set_grow_rate(&interface_ptr.get_node(i).unwrap(), Rational::zero());
            }
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // check constraint
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(0).unwrap(), dual_module);
            constraints.to_row_echelon_form(); constraints.print();
            assert!(!constraints.cluster.is_valid(dual_module));
            dual_module.set_grow_rate(&interface_ptr.get_node(0).unwrap(), Rational::zero());
            interface_ptr.create_cluster_node(constraints.cluster.grown_edges.clone(), dual_module);
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // check constraint
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(12).unwrap(), dual_module);
            constraints.to_row_echelon_form(); constraints.print();
            assert!(!constraints.cluster.is_valid(dual_module));
            dual_module.set_grow_rate(&interface_ptr.get_node(12).unwrap(), Rational::zero());
            interface_ptr.create_cluster_node(constraints.cluster.grown_edges.clone(), dual_module);
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // check constraint: valid cluster
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(13).unwrap(), dual_module);
            constraints.to_row_echelon_form(); constraints.print();
            assert!(constraints.cluster.is_valid(dual_module));
            // then check whether each cluster has single hair solution
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(13).unwrap(), dual_module);
            let single_hair_solution = constraints.get_single_hair_solution_or_necessary_edge_set();
            println!("single hair solution: {:?}", single_hair_solution);
            let internal_edges = constraints.cluster.edges_excluding(&single_hair_solution.unwrap_err());
            println!("internal_edges: {internal_edges:?}");
            interface_ptr.create_cluster_node(internal_edges.iter().cloned().collect(), dual_module);
            dual_module.set_grow_rate(&interface_ptr.get_node(13).unwrap(), -Rational::one());
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // then create another cluster
            let cluster = ExploreCluster::new(&interface_ptr.get_node(13).unwrap(), dual_module);
            let internal_edges = cluster.grown_edges.iter().cloned().collect();
            println!("internal_edges: {internal_edges:?}");
            println!("valid: {}", dual_module.is_valid_cluster(&internal_edges));
            dual_module.set_grow_rate(&interface_ptr.get_node(13).unwrap(), Rational::zero());
            dual_module.set_grow_rate(&interface_ptr.get_node(14).unwrap(), Rational::zero());
            interface_ptr.create_cluster_node(internal_edges.iter().cloned().collect(), dual_module);
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // check constraint
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(15).unwrap(), dual_module);
            let single_hair_solution = constraints.get_single_hair_solution_or_necessary_edge_set();
            println!("single hair solution: {:?}", single_hair_solution);
            let internal_edges = constraints.cluster.edges_excluding(&single_hair_solution.unwrap_err());
            println!("internal_edges: {internal_edges:?}");
            interface_ptr.create_cluster_node(internal_edges.iter().cloned().collect(), dual_module);
            dual_module.set_grow_rate(&interface_ptr.get_node(15).unwrap(), -Rational::one());
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // then create another cluster
            let cluster = ExploreCluster::new(&interface_ptr.get_node(16).unwrap(), dual_module);
            let internal_edges = cluster.grown_edges.iter().cloned().collect();
            println!("internal_edges: {internal_edges:?}");
            println!("valid: {}", dual_module.is_valid_cluster(&internal_edges));
            dual_module.set_grow_rate(&interface_ptr.get_node(15).unwrap(), Rational::zero());
            dual_module.set_grow_rate(&interface_ptr.get_node(16).unwrap(), Rational::zero());
            interface_ptr.create_cluster_node(internal_edges.iter().cloned().collect(), dual_module);
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // now every non-zero dual variable can solve a single edge
            let mut subgraph_edges = vec![];
            for node_index in [0, 12, 14, 16, 17] {
                let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(node_index).unwrap(), dual_module);
                let single_hair_solution = constraints.get_single_hair_solution_or_necessary_edge_set();
                println!("single hair solution: {:?}", single_hair_solution);
                let single_hair_edges = single_hair_solution.unwrap();
                assert!(single_hair_edges.len() == 1, "haven't thought about how to handle multiple hair edge cases...");
                subgraph_edges.push(single_hair_edges[0])
            }
            Subgraph::new(subgraph_edges)
        });
    }

    // what about degeneracy? i.e. multiple possible paths, how to find a (in fact, any) proper one?
    #[test]
    fn explore_primal_module_5() {  // cargo test explore_primal_module_5 -- --nocapture
        let visualize_filename = format!("explore_primal_module_5.json");
        let mut code = CodeCapacityPlanarCode::new(11, 0.01, 1);
        code.apply_errors(&[88, 89, 101]);
        let defect_vertices = code.get_syndrome().defect_vertices;
        explore_primal_module_method(code, Some(visualize_filename), defect_vertices, 5, |interface_ptr, dual_module, visualizer| {
            // use single grow mode
            for i in 1..2 {
                dual_module.set_grow_rate(&interface_ptr.get_node(i).unwrap(), Rational::zero());
            }
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // check constraint
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(0).unwrap(), dual_module);
            constraints.to_row_echelon_form(); constraints.print();
            assert!(!constraints.cluster.is_valid(dual_module));
            dual_module.set_grow_rate(&interface_ptr.get_node(0).unwrap(), Rational::zero());
            interface_ptr.create_cluster_node(constraints.cluster.grown_edges.clone(), dual_module);
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // check constraint
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(2).unwrap(), dual_module);
            constraints.to_row_echelon_form(); constraints.print();
            assert!(!constraints.cluster.is_valid(dual_module));
            dual_module.set_grow_rate(&interface_ptr.get_node(2).unwrap(), Rational::zero());
            interface_ptr.create_cluster_node(constraints.cluster.grown_edges.clone(), dual_module);
            let group_max_update_length = dual_module.compute_maximum_update_length();
            dual_module.grow(group_max_update_length.get_valid_growth().unwrap());
            take_snapshot(visualizer, "grow".to_string(), interface_ptr, &dual_module);
            // check constraint
            let mut constraints = ExploreParityConstraints::new(&interface_ptr.get_node(3).unwrap(), dual_module);
            constraints.to_row_echelon_form(); constraints.print();
            assert!(constraints.cluster.is_valid(dual_module));

            let mut subgraph_edges = vec![];
            Subgraph::new(subgraph_edges)
        });
    }

}
