//! Matrix Definition
//!
//! A matrix has a fixed data layout which consists of multiple rows
//!
//! The only operations to change the basic matrix itself are
//! - add variable (column)
//! - add constraint (row)
//! - xor/swap rows
//!
//! Apart from the matrix itself, we can have a view of the matrix:
//! a view is defined as a list of columns `var_indices: Vec<usize>`
//! and a number of rows `rows_count` representing rows `0..rows_count`.
//!
//! A `var_index` is always referring to the original matrix, to improve performance
//! as well as to avoid confusion.
//!
//! Each variable (column) corresponds to an edge in the hypergraph, so we label the
//! columns in the basic matrix. When it comes to operating the matrix, we'll always
//! use the `var_index` to avoid duplicated translation (at least one translation is necessary).
//!

use crate::util::*;
use derivative::Derivative;
// use num_rational::Ratio;
use slp::BigInt;
use num_traits::{One, Zero};
use weak_table::PtrWeakHashSet;
use std::collections::BTreeSet;

#[cfg(feature = "pq")]
use crate::dual_module_pq::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};
#[cfg(feature = "non-pq")]
use crate::dual_module_serial::{EdgeWeak, VertexWeak, EdgePtr, VertexPtr};


pub type VarIndex = usize;
pub type RowIndex = usize;
pub type ColumnIndex = usize;

pub trait MatrixBasic {
    /// add an edge to the basic matrix, return the `var_index` if newly created
    fn add_variable(&mut self, edge_weak: EdgeWeak) -> Option<VarIndex>;

    /// add constraint will implicitly call `add_variable` if the edge is not added and return the indices of them
    fn add_constraint(
        &mut self,
        vertex_weak: VertexWeak,
        incident_edges: &[EdgeWeak],
        parity: bool,
    ) -> Option<Vec<VarIndex>>;

    /// row operations
    fn xor_row(&mut self, target: RowIndex, source: RowIndex);
    fn swap_row(&mut self, a: RowIndex, b: RowIndex);

    /// view the raw matrix
    fn get_lhs(&self, row: RowIndex, var_index: VarIndex) -> bool;
    fn get_rhs(&self, row: RowIndex) -> bool;

    /// get edge index from the var_index
    fn var_to_edge_index(&self, var_index: VarIndex) -> EdgeWeak;

    fn edge_to_var_index(&self, edge_weak: EdgeWeak) -> Option<VarIndex>;

    fn exists_edge(&self, edge_weak: EdgeWeak) -> bool {
        self.edge_to_var_index(edge_weak).is_some()
    }

    fn get_vertices(&self) -> BTreeSet<VertexPtr>;
}

pub trait MatrixView: MatrixBasic {
    /// the number of columns: to get the `var_index` of each column,
    /// use `column_to_var_index()`; here the mutable reference enables
    /// lazy update of the internal data structure
    fn columns(&mut self) -> usize;

    /// get the `var_index` in the basic matrix
    fn column_to_var_index(&self, column: ColumnIndex) -> VarIndex;

    fn column_to_edge_index(&self, column: ColumnIndex) -> EdgeWeak {
        let var_index = self.column_to_var_index(column);
        self.var_to_edge_index(var_index)
    }

    /// the number of rows: rows always have indices 0..rows
    fn rows(&mut self) -> usize;

    fn get_view_edges(&mut self) -> Vec<EdgeWeak> {
        (0..self.columns())
            .map(|column: usize| self.column_to_edge_index(column))
            .collect()
    }

    fn var_to_column_index(&mut self, var_index: VarIndex) -> Option<ColumnIndex> {
        (0..self.columns()).find(|&column| self.column_to_var_index(column) == var_index)
    }

    fn edge_to_column_index(&mut self, edge_weak: EdgeWeak) -> Option<ColumnIndex> {
        let var_index = self.edge_to_var_index(edge_weak)?;
        self.var_to_column_index(var_index)
    }
}

pub trait MatrixTight: MatrixView {
    fn update_edge_tightness(&mut self, edge_weak: EdgeWeak, is_tight: bool);
    fn is_tight(&self, edge_weak: EdgeWeak) -> bool;

    fn add_variable_with_tightness(&mut self, edge_weak: EdgeWeak, is_tight: bool) {
        self.add_variable(edge_weak.clone());
        self.update_edge_tightness(edge_weak.clone(), is_tight);
    }

    fn add_tight_variable(&mut self, edge_weak: EdgeWeak) {
        self.add_variable_with_tightness(edge_weak, true)
    }
}

pub trait MatrixTail {
    fn get_tail_edges(&self) -> &BTreeSet<EdgePtr>;
    fn get_tail_edges_mut(&mut self) -> &mut BTreeSet<EdgePtr>;

    fn set_tail_edges<EdgeIter>(&mut self, edges: EdgeIter)
    where
        EdgeIter: Iterator<Item = EdgeWeak>,
    {
        let tail_edges = self.get_tail_edges_mut();
        tail_edges.clear();
        for edge_weak in edges {
            tail_edges.insert(edge_weak.upgrade_force());
        }
    }

    fn get_tail_edges_vec(&self) -> Vec<EdgeWeak> {
        let mut edges: Vec<EdgeWeak> = self.get_tail_edges().iter().map(|e| e.downgrade()).collect();
        edges.sort();
        edges
    }
}

pub trait MatrixEchelon: MatrixView {
    fn get_echelon_info(&mut self) -> &EchelonInfo;
    fn get_echelon_info_immutable(&self) -> &EchelonInfo;

    fn get_solution(&mut self) -> Option<Subgraph> {
        self.get_echelon_info(); // make sure it's in echelon form
        let info = self.get_echelon_info_immutable();
        if !info.satisfiable {
            return None; // no solution
        }
        let mut solution = vec![];
        for (row, row_info) in info.rows.iter().enumerate() {
            debug_assert!(row_info.has_leading());
            if self.get_rhs(row) {
                let column = row_info.column;
                let edge_weak = self.column_to_edge_index(column);
                solution.push(edge_weak.clone());
            }
        }

        Some(solution)
    }

    /// try every independent variables and try to minimize the total weight of the solution
    fn get_solution_local_minimum<F>(&mut self, weight_of: F) -> Option<Subgraph>
    where
        F: Fn(EdgeWeak) -> Rational,
    {
        self.get_echelon_info(); // make sure it's in echelon form
        let info = self.get_echelon_info_immutable();
        // println!("echelon info: {:?}", info);
        if !info.satisfiable {
            return None; // no solution
        }
        let mut solution: BTreeSet<EdgeWeak> = BTreeSet::new();
        for (row, row_info) in info.rows.iter().enumerate() {
            debug_assert!(row_info.has_leading());
            if self.get_rhs(row) {
                let column = row_info.column;
                let edge_index = self.column_to_edge_index(column);
                solution.insert(edge_index);
            }
        }
        let mut independent_columns = vec![];
        for (column, column_info) in info.columns.iter().enumerate() {
            if !column_info.is_dependent() {
                independent_columns.push(column);
            }
        }
        let mut total_weight = Rational::zero();
        for edge_index in solution.iter() {
            total_weight += weight_of(edge_index.clone());
        }
        let mut pending_flip_edge_indices = vec![];
        let mut is_local_minimum = false;
        while !is_local_minimum {
            is_local_minimum = true;
            // try every independent variable and find a local minimum
            for &column in independent_columns.iter() {
                pending_flip_edge_indices.clear();
                let var_index = self.column_to_var_index(column);
                let edge_weak = self.var_to_edge_index(var_index);
                let local_weight = weight_of(edge_weak.clone());
                let mut primal_delta =
                    (local_weight) * (if solution.contains(&edge_weak) { -Rational::one() } else { Rational::one() });
                pending_flip_edge_indices.push(edge_weak);
                for row in 0..info.rows.len() {
                    if self.get_lhs(row, var_index) {
                        debug_assert!(info.rows[row].has_leading());
                        let flip_column = info.rows[row].column;
                        debug_assert!(flip_column < column);
                        let flip_edge_index = self.column_to_edge_index(flip_column);
                        primal_delta += (weight_of(flip_edge_index.clone()))
                            * (if solution.contains(&flip_edge_index) { -Rational::one() } else { Rational::one() });
                        pending_flip_edge_indices.push(flip_edge_index);
                    }
                }
                if primal_delta < Rational::zero() {
                    total_weight = total_weight + primal_delta;
                    for edge_index in pending_flip_edge_indices.iter() {
                        if solution.contains(&edge_index) {
                            solution.remove(&edge_index);
                        } else {
                            solution.insert(edge_index.clone());
                        }
                    }
                    is_local_minimum = false;
                    break; // loop over again
                }
            }
        }
        Some(solution.into_iter().collect())
    }
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Default(new = "true"))]
pub struct EchelonInfo {
    /// whether it's a satisfiable matrix, only valid when `is_echelon_form` is true
    pub satisfiable: bool,
    /// (is_dependent, if dependent the only "1" position row)
    pub columns: Vec<ColumnInfo>,
    /// the number of effective rows
    pub effective_rows: usize,
    /// the leading "1" position column
    pub rows: Vec<RowInfo>,
}

#[derive(Clone, Copy, Derivative, PartialEq, Eq)]
#[derivative(Default(new = "true"))]
pub struct ColumnInfo {
    pub row: RowIndex,
}

impl ColumnInfo {
    pub fn not_dependent() -> Self {
        Self { row: RowIndex::MAX }
    }
    pub fn set(&mut self, row: RowIndex) {
        debug_assert!(row != RowIndex::MAX);
        self.row = row;
    }
    pub fn is_dependent(&self) -> bool {
        self.row != RowIndex::MAX
    }
    pub fn set_not_dependent(&mut self) {
        self.row = RowIndex::MAX;
    }
}

impl std::fmt::Debug for ColumnInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if !self.is_dependent() {
            write!(f, "Row(*)")
        } else {
            write!(f, "Row({})", self.row)
        }
    }
}

#[derive(Clone, Copy, Derivative, PartialEq, Eq)]
#[derivative(Default(new = "true"))]
pub struct RowInfo {
    pub column: ColumnIndex,
}

impl RowInfo {
    pub fn no_leading() -> Self {
        Self {
            column: ColumnIndex::MAX,
        }
    }
    pub fn set(&mut self, column: ColumnIndex) {
        debug_assert!(column != ColumnIndex::MAX);
        self.column = column;
    }
    pub fn has_leading(&self) -> bool {
        self.column != ColumnIndex::MAX
    }
    pub fn set_no_leading(&mut self) {
        self.column = ColumnIndex::MAX;
    }
}

impl std::fmt::Debug for RowInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if !self.has_leading() {
            write!(f, "Col(*)")
        } else {
            write!(f, "Col({})", self.column)
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::super::*;
    use super::*;
    use num_traits::{FromPrimitive, Zero};
    use weak_table::PtrWeakKeyHashMap;
    use crate::dual_module_pq::{EdgePtr, Edge, VertexPtr, Vertex};
    use crate::pointers::*;
    use std::collections::HashSet;

    type TightMatrix = Tight<BasicMatrix>;

    #[test]
    fn matrix_interface_simple() {
        // cargo test --features=colorful matrix_interface_simple -- --nocapture
        let mut matrix = TightMatrix::new();

        // create vertices 
        let vertices: Vec<VertexPtr> = (0..3)
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

        // create edges
        let edges: Vec<EdgePtr> = vec![233, 14, 68, 75, 666].into_iter()
            .map(|edge_index| {
                EdgePtr::new_value(Edge {
                    edge_index: edge_index,
                    weight: Rational::zero(),
                    dual_nodes: vec![],
                    vertices: vec![],
                    last_updated_time: Rational::zero(),
                    growth_at_last_updated_time: Rational::zero(),
                    grow_rate: Rational::zero(),
                    unit_index: None,
                    connected_to_boundary_vertex: false,
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();

        matrix.add_tight_variable(edges[0].downgrade());
        matrix.add_tight_variable(edges[1].downgrade());
        matrix.add_variable(edges[2].downgrade());
        matrix.add_tight_variable(edges[3].downgrade());
        matrix.printstd();
        assert_eq!(matrix.get_view_edges().iter().map(|e| e.upgrade_force().read_recursive().edge_index).collect::<Vec<_>>(), [233, 14, 75]);
        assert_eq!(matrix.var_to_column_index(0), Some(0));
        assert_eq!(matrix.var_to_column_index(1), Some(1));
        assert_eq!(matrix.var_to_column_index(2), None);
        assert_eq!(matrix.var_to_column_index(3), Some(2));
        assert_eq!(matrix.edge_to_column_index(edges[0].downgrade()), Some(0));
        assert_eq!(matrix.edge_to_column_index(edges[1].downgrade()), Some(1));
        assert_eq!(matrix.edge_to_column_index(edges[2].downgrade()), None);
        assert_eq!(matrix.edge_to_column_index(edges[3].downgrade()), Some(2));
        assert_eq!(matrix.edge_to_column_index(edges[4].downgrade()), None);

        drop(vertices);
        drop(edges);
    }

    #[test]
    fn matrix_interface_echelon_info() {
        // cargo test matrix_interface_echelon_info -- --nocapture
        let mut column_info = ColumnInfo::new();
        column_info.set(13);
        assert_eq!(format!("{column_info:?}"), "Row(13)");
        column_info.set_not_dependent();
        assert_eq!(format!("{column_info:?}"), "Row(*)");
        assert_eq!(format!("{:?}", column_info.clone()), "Row(*)");
        let mut row_info = RowInfo::new();
        row_info.set(13);
        assert_eq!(format!("{row_info:?}"), "Col(13)");
        row_info.set_no_leading();
        assert_eq!(format!("{row_info:?}"), "Col(*)");
        assert_eq!(format!("{:?}", row_info.clone()), "Col(*)");
        let echelon_info = EchelonInfo::new();
        println!("echelon_info: {echelon_info:?}");
    }

    #[derive(Default)]
    struct TestEdgeWeights {
        pub weights: PtrWeakKeyHashMap<EdgeWeak, Rational>,
    }

    impl TestEdgeWeights {
        fn new(weights: &[(EdgeWeak, Rational)]) -> Self {
            let mut result: TestEdgeWeights = Default::default();
            for (edge_index, weight) in weights.iter() {
                result.weights.insert(edge_index.upgrade_force(), *weight);
            }
            result
        }
        fn get_solution_local_minimum(&self, matrix: &mut Echelon<Tail<BasicMatrix>>) -> Option<Subgraph> {
            matrix.get_solution_local_minimum(|edge_weak| {
                if let Some(weight) = self.weights.get(&edge_weak.upgrade_force()) {
                    *weight
                } else {
                    Rational::one()
                }
            })
        }
    }

    #[test]
    fn matrix_interface_echelon_solution() {
        // cargo test --features=colorful matrix_interface_echelon_solution -- --nocapture
        /* 0,1,2: vertices; (0),(1),(2): edges; !n!: odd vertex
               1 (1) 0
              (2)   (0)
         3 (3) 2 (8)!7!
        (4)   (7)   (9)
        !4!(5) 5 (6) 6
            */
        let mut matrix = Echelon::<Tail<BasicMatrix>>::new();
        let parity_checks = vec![
            (vec![0, 1], false),
            (vec![1, 2], false),
            (vec![2, 3, 7, 8], false),
            (vec![3, 4], false),
            (vec![4, 5], true),
            (vec![5, 6, 7], false),
            (vec![6, 9], false),
            (vec![0, 8, 9], true),
        ];

        // create vertices 
        let vertices: Vec<VertexPtr> = (0..parity_checks.len())
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
                    unit_index: None,
                    connected_to_boundary_vertex: false,
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();


        for (vertex_index, (incident_edges, parity)) in parity_checks.iter().enumerate() {
            let local_incident_edges: Vec<EdgeWeak> = incident_edges.iter().map(|&i| edges[i].downgrade()).collect();
            matrix.add_constraint(vertices[vertex_index].downgrade(), &local_incident_edges, *parity);
        }
        matrix.printstd();
        // assert_eq!(matrix.get_solution().unwrap().iter().map(|e| e.upgrade_force().read_recursive().edge_index).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
        let weights = TestEdgeWeights::new(&[(edges[3].downgrade(), Rational::from_i64(10).unwrap()), (edges[9].downgrade(), Rational::from_i64(10).unwrap())]);
        let matrix_vertices: HashSet<_> = weights.get_solution_local_minimum(&mut matrix).unwrap().iter().map(|e| e.upgrade_force().read_recursive().edge_index).collect();
        assert_eq!(matrix_vertices, [5, 7, 8].into());
        let weights = TestEdgeWeights::new(&[(edges[7].downgrade(), Rational::from_i64(10).unwrap()), (edges[9].downgrade(), Rational::from_i64(10).unwrap())]);
        assert_eq!(weights.get_solution_local_minimum(&mut matrix).unwrap().iter().map(|e| e.upgrade_force().read_recursive().edge_index).collect::<Vec<_>>(), vec![3, 4, 8]);
        let weights = TestEdgeWeights::new(&[(edges[3].downgrade(), Rational::from_i64(10).unwrap()), (edges[4].downgrade(), Rational::from_i64(10).unwrap()), (edges[7].downgrade(), Rational::from_i64(10).unwrap())]);
        assert_eq!(weights.get_solution_local_minimum(&mut matrix).unwrap().iter().map(|e| e.upgrade_force().read_recursive().edge_index).collect::<Vec<_>>(), vec![5, 6, 9]);
        drop(vertices);
        drop(edges);
    }

    #[test]
    fn matrix_interface_echelon_no_solution() {
        // cargo test --quiet matrix_interface_echelon_no_solution -- --nocapture
        let mut matrix = Echelon::<Tail<BasicMatrix>>::new();
        let parity_checks = vec![(vec![0, 1], false), (vec![0, 1], true)];

        // create vertices 
        let vertices: Vec<VertexPtr> = (0..parity_checks.len())
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

        // create edges
        let edges: Vec<EdgePtr> = (0..2)
            .map(|edge_index| {
                EdgePtr::new_value(Edge {
                    edge_index: edge_index,
                    weight: Rational::zero(),
                    dual_nodes: vec![],
                    vertices: vec![],
                    last_updated_time: Rational::zero(),
                    growth_at_last_updated_time: Rational::zero(),
                    grow_rate: Rational::zero(),
                    unit_index: None,
                    connected_to_boundary_vertex: false,
                    #[cfg(feature = "incr_lp")]
                    cluster_weights: hashbrown::HashMap::new(),
                })
            }).collect();

        for (vertex_index, (incident_edges, parity)) in parity_checks.iter().enumerate() {
            let local_incident_edges: Vec<EdgeWeak> = incident_edges.iter().map(|&i| edges[i].downgrade()).collect();
            matrix.add_constraint(vertices[vertex_index].downgrade(), &local_incident_edges, *parity);
        }
        assert_eq!(matrix.get_solution(), None);
        let weights = TestEdgeWeights::new(&[]);
        assert_eq!(weights.get_solution_local_minimum(&mut matrix), None);

        drop(vertices);
        drop(edges);
    }
}
