//! Relaxer Optimizer
//!
//! It's possible that two (or more) positive relaxers are bouncing, and the actual growth
//! is exponentially smaller but never reaching the optimal. In this case, we need
//! this module to optimize the positive relaxers. This only takes effect when such bouncing
//! is detected, and remains minimum in all other cases to avoid reduce time complexity.
//!

use crate::invalid_subgraph::*;
use crate::relaxer::*;
use crate::util::*;

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use derivative::Derivative;

use num_traits::{Signed, Zero};

#[cfg(feature = "slp")]
use num_traits::One;
use parking_lot::Mutex;

// FIXME: Add correct cfg flags
pub struct IncrLPSolution {
    pub dual_variables: BTreeMap<Arc<InvalidSubgraph>, Rational>,
    pub edge_slacks: BTreeMap<EdgeIndex, Rational>,
    pub edge_contributors: BTreeMap<EdgeIndex, BTreeSet<Arc<InvalidSubgraph>>>,
    pub solution: Option<highs::SolvedModel>,
    pub current_dual_variables_sum: Rational,
}

impl IncrLPSolution {
    pub fn constraints_len(&self) -> usize {
        self.dual_variables.len() + self.edge_slacks.len()
    }
}

unsafe impl Send for IncrLPSolution {}

#[derive(Default, Debug)]
pub enum OptimizerResult {
    #[default]
    Init,
    Optimized,     // normal
    EarlyReturned, // early return when the result is positive
    Skipped,       // when the `should_optimize` check returns false
}

impl OptimizerResult {
    pub fn or(&mut self, other: Self) {
        match self {
            OptimizerResult::EarlyReturned => {}
            _ => match other {
                OptimizerResult::Init => {}
                OptimizerResult::EarlyReturned => {
                    *self = OptimizerResult::EarlyReturned;
                }
                OptimizerResult::Skipped => {
                    *self = OptimizerResult::Skipped;
                }
                _ => {}
            },
        }
    }
}

#[derive(Derivative)]
#[derivative(Default(new = "true"))]
pub struct RelaxerOptimizer {
    /// the set of existing relaxers
    relaxers: BTreeSet<Relaxer>,
}

#[derive(Derivative)]
#[derivative(Default(new = "true"))]
pub struct ConstraintLine {
    pub lhs: Vec<(Rational, String)>,
    pub rhs: Rational,
}

fn rational_to_str(value: &Rational) -> String {
    format!("{}/{}", value.numer(), value.denom())
}

impl std::fmt::Display for ConstraintLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lhs_str_vec: Vec<String> = self
            .lhs
            .iter()
            .enumerate()
            .map(|(idx, (coefficient, var))| {
                let mut coefficient_str = rational_to_str(coefficient) + var;
                if idx > 0 && !coefficient_str.starts_with('-') {
                    coefficient_str = "+".to_string() + &coefficient_str;
                }
                coefficient_str
            })
            .collect();
        f.write_str(&(lhs_str_vec.join(" ") + " <= " + &rational_to_str(&self.rhs)))
    }
}

impl RelaxerOptimizer {
    /// moves all relaxer from other to here, when merging clusters
    pub fn append(&mut self, other: &mut RelaxerOptimizer) {
        self.relaxers.append(&mut other.relaxers);
    }

    pub fn insert(&mut self, relaxer: Relaxer) {
        self.relaxers.insert(relaxer);
    }

    pub fn should_optimize(&self, relaxer: &Relaxer) -> bool {
        // avoid calling optimizer on simple growing relaxer
        if relaxer.get_direction().len() < 2 {
            return false;
        }
        // self.relaxers.contains(relaxer)
        true
    }

    #[cfg(not(feature = "float_lp"))]
    pub fn optimize(
        &mut self,
        relaxer: Relaxer,
        edge_slacks: BTreeMap<EdgeIndex, Rational>,
        mut dual_variables: BTreeMap<Arc<InvalidSubgraph>, Rational>,
    ) -> (Relaxer, bool) {
        for invalid_subgraph in relaxer.get_direction().keys() {
            if !dual_variables.contains_key(invalid_subgraph) {
                dual_variables.insert(invalid_subgraph.clone(), Rational::zero());
            }
        }
        // look at all existing invalid subgraphs and propose a best direction
        // each invalid subgraph corresponds to a variable
        // each edge_slack or dual_variable correspond to a constraint
        // the objective function is the summation of all dual variables
        let mut x_vars = vec![];
        let mut y_vars = vec![];
        let mut constraints = vec![];
        let mut invalid_subgraphs = Vec::with_capacity(dual_variables.len());
        let mut edge_contributor: BTreeMap<EdgeIndex, Vec<usize>> =
            edge_slacks.keys().map(|&edge_index| (edge_index, vec![])).collect();
        for (var_index, (invalid_subgraph, dual_variable)) in dual_variables.iter().enumerate() {
            // slp only allows >= 0 variables, make this adaption
            let var_x = format!("x{var_index}");
            let var_y = format!("y{var_index}");
            x_vars.push(var_x.clone());
            y_vars.push(var_y.clone());
            // constraint of the dual variable >= 0
            let mut constraint = ConstraintLine::new();
            constraint.lhs.push((-Rational::one(), var_x.clone()));
            constraint.lhs.push((Rational::one(), var_y.clone()));
            constraint.rhs = dual_variable.clone();
            constraints.push(constraint);
            invalid_subgraphs.push(invalid_subgraph);
            for &edge_index in invalid_subgraph.hair.iter() {
                edge_contributor.get_mut(&edge_index).unwrap().push(var_index);
            }
        }
        for (&edge_index, slack) in edge_slacks.iter() {
            // constraint of edge: sum(y_S) <= weight
            let mut constraint = ConstraintLine::new();
            for &var_index in edge_contributor[&edge_index].iter() {
                constraint.lhs.push((Rational::one(), x_vars[var_index].clone()));
                constraint.lhs.push((-Rational::one(), y_vars[var_index].clone()));
            }
            constraint.rhs = slack.clone();
            constraints.push(constraint);
        }
        let vars_line = "vars ".to_string()
            + &x_vars
                .iter()
                .chain(y_vars.iter())
                .map(|var| format!("{var}>=0"))
                .collect::<Vec<_>>()
                .join(", ");
        let max_line = "max ".to_string() + &x_vars.to_vec().join("+") + "-" + &y_vars.to_vec().join("-");
        let input = vars_line
            + "\n"
            + &max_line
            + "\n"
            + "subject to\n"
            + &constraints
                .iter()
                .map(|constraint| constraint.to_string())
                .collect::<Vec<_>>()
                .join(",\n");

        // println!("\n input:\n {}\n", input);

        let mut solver = slp::Solver::<slp::Ratio<slp::BigInt>>::new(&input);
        let solution = solver.solve();
        let mut direction: BTreeMap<Arc<InvalidSubgraph>, Rational> = BTreeMap::new();
        match solution {
            slp::Solution::Optimal(optimal_objective, model) => {
                if !optimal_objective.is_positive() {
                    return (relaxer, true);
                }
                for (var_index, (invalid_subgraph, _)) in dual_variables.into_iter().enumerate() {
                    let overall_growth = model[var_index].clone() - model[var_index + x_vars.len()].clone();
                    if !overall_growth.is_zero() {
                        // println!("overall_growth: {:?}", overall_growth);
                        direction.insert(invalid_subgraph, overall_growth);
                    }
                }
            }
            _ => unreachable!(),
        }
        self.relaxers.insert(relaxer);
        (Relaxer::new(direction), false)
    }

    #[cfg(feature = "float_lp")]
    // the same method, but with f64 weight
    pub fn optimize(
        &mut self,
        relaxer: Relaxer,
        edge_slacks: BTreeMap<EdgeIndex, Rational>,
        mut dual_variables: BTreeMap<Arc<InvalidSubgraph>, Rational>,
    ) -> (Relaxer, bool) {
        use highs::{HighsModelStatus, RowProblem, Sense};
        use num_traits::ToPrimitive;

        use crate::ordered_float::OrderedFloat;

        for invalid_subgraph in relaxer.get_direction().keys() {
            if !dual_variables.contains_key(invalid_subgraph) {
                dual_variables.insert(invalid_subgraph.clone(), OrderedFloat::zero());
            }
        }

        let mut model = RowProblem::default().optimise(Sense::Maximise);
        model.set_option("time_limit", 5.0); // stop after 30 seconds

        let mut x_vars = vec![];
        let mut y_vars = vec![];
        let mut invalid_subgraphs = Vec::with_capacity(dual_variables.len());
        let mut edge_contributor: BTreeMap<EdgeIndex, Vec<usize>> =
            edge_slacks.keys().map(|&edge_index| (edge_index, vec![])).collect();

        for (var_index, (invalid_subgraph, dual_variable)) in dual_variables.iter().enumerate() {
            // constraint of the dual variable >= 0
            let x = model.add_col(1.0, 0.0.., []);
            let y = model.add_col(-1.0, 0.0.., []);
            x_vars.push(x);
            y_vars.push(y);

            // constraint for xs ys <= dual_variable
            model.add_row(
                ..dual_variable.to_f64().unwrap(),
                [(x_vars[var_index], -1.0), (y_vars[var_index], 1.0)],
            );
            invalid_subgraphs.push(invalid_subgraph.clone());

            for &edge_index in invalid_subgraph.hair.iter() {
                edge_contributor.get_mut(&edge_index).unwrap().push(var_index);
            }
        }

        for (&edge_index, &slack) in edge_slacks.iter() {
            let mut row_entries = vec![];
            for &var_index in edge_contributor[&edge_index].iter() {
                row_entries.push((x_vars[var_index], 1.0));
                row_entries.push((y_vars[var_index], -1.0));
            }

            // constraint of edge: sum(y_S) <= weight
            model.add_row(..=slack.to_f64().unwrap(), row_entries);
        }

        let solved = model.solve();

        let mut direction: BTreeMap<Arc<InvalidSubgraph>, OrderedFloat> = BTreeMap::new();
        if solved.status() == HighsModelStatus::Optimal {
            let solution = solved.get_solution();

            // calculate the objective function
            let mut res = OrderedFloat::new(0.0);
            let cols = solution.columns();
            for i in 0..x_vars.len() {
                res += OrderedFloat::new(cols[2 * i] - cols[2 * i + 1]);
            }

            // check positivity of the objective
            if !(res.is_positive()) {
                return (relaxer, true);
            }

            for (var_index, invalid_subgraph) in invalid_subgraphs.iter().enumerate() {
                let overall_growth = cols[2 * var_index] - cols[2 * var_index + 1];
                if !overall_growth.is_zero() {
                    direction.insert(invalid_subgraph.clone(), OrderedFloat::from(overall_growth));
                }
            }
        } else {
            println!("solved status: {:?}", solved.status());
            unreachable!();
        }

        self.relaxers.insert(relaxer);
        (Relaxer::new(direction), false)
    }

    #[cfg(feature = "float_lp")]
    // the same method, but with f64 weight
    pub fn optimize_incr(
        &mut self,
        relaxer: Relaxer,
        edge_free_weights: BTreeMap<EdgeIndex, Rational>,
        mut dual_variables: BTreeMap<Arc<InvalidSubgraph>, Rational>, //fixme: Why do we need the raional here?
        incr_lp_solution: &mut Option<Arc<Mutex<IncrLPSolution>>>,
    ) -> (Relaxer, bool) {
        use highs::{HighsModelStatus, RowProblem, Sense};
        use num_traits::ToPrimitive;

        use crate::{dual_module_pq::Edge, invalid_subgraph, ordered_float::OrderedFloat};

        // Maybe should be here, or before the calculation of edge_free_weights
        for invalid_subgraph in relaxer.get_direction().keys() {
            if !dual_variables.contains_key(invalid_subgraph) {
                dual_variables.insert(invalid_subgraph.clone(), OrderedFloat::zero());
            }
        }

        return match incr_lp_solution {
            Some(incr_lp_solution) => {
                panic!();
                let mut incr_lp_solution_ptr = incr_lp_solution.lock();
                let model: highs::Model = incr_lp_solution_ptr.solution.take().unwrap().into();

                let mut new_dual_variables = BTreeMap::new();
                let mut update_dual_variables = BTreeMap::new();
                let mut edge_contributors: BTreeMap<EdgeIndex, BTreeSet<Arc<InvalidSubgraph>>> = BTreeMap::new();

                for (invalid_subgraph, dual_variable) in dual_variables.iter() {
                    match incr_lp_solution_ptr.dual_variables.get(invalid_subgraph) {
                        Some(_dual_variable) => {
                            if _dual_variable != dual_variable {
                                update_dual_variables.insert(invalid_subgraph.clone(), dual_variable.clone());
                            }
                        }
                        None => {
                            new_dual_variables.insert(invalid_subgraph.clone(), dual_variable.clone());
                        }
                    }
                    for &edge_index in invalid_subgraph.hair.iter() {
                        edge_contributors
                            .get_mut(&edge_index)
                            .unwrap()
                            .insert(invalid_subgraph.clone());
                    }
                }

                let mut new_edges = BTreeSet::new();
                let mut update_edges_slack = BTreeSet::new();
                let mut update_edges_contributors = BTreeSet::new();

                // fixme:
                for edge_index in edge_free_weights.keys() {
                    match incr_lp_solution_ptr.edge_slacks.get(edge_index) {
                        Some(_slack) => {
                            if _slack != edge_free_weights[edge_index] {
                                update_edges_slack.insert(edge_index.clone());
                            }
                            if let Some(_edge_contributors) = incr_lp_solution_ptr.edge_contributors.get(edge_index) {
                                if _edge_contributors != &edge_contributors[edge_index] {
                                    update_edges_contributors.insert(edge_index.clone());
                                    println!("Actually Here ....");
                                }
                            }
                        }
                        None => {
                            new_edges.insert(edge_index.clone());
                        }
                    }
                }

                // get the difference between the constraints and update them accordingly
                todo!()
            }
            None => {
                /*
                    let mut model = RowProblem::default().optimise(Sense::Maximise);
                    model.set_option("time_limit", 30.0); // stop after 30 seconds

                    let mut x_vars = vec![];
                    let mut y_vars = vec![];
                    let mut invalid_subgraphs = Vec::with_capacity(dual_variables.len());
                    let mut edge_contributor: BTreeMap<EdgeIndex, Vec<usize>> =
                        edge_slacks.keys().map(|&edge_index| (edge_index, vec![])).collect();

                    let mut edge_contributors: BTreeMap<EdgeIndex, BTreeSet<Arc<InvalidSubgraph>>> =
                        edge_slacks.keys().map(|&edge_index| (edge_index, BTreeSet::new())).collect();

                    for (var_index, (invalid_subgraph, dual_variable)) in dual_variables.iter().enumerate() {
                        // constraint of the dual variable >= 0
                        let x = model.add_col(1.0, 0.0.., []);
                        let y = model.add_col(-1.0, 0.0.., []);
                        x_vars.push(x);
                        y_vars.push(y);

                        // constraint for xs ys <= dual_variable
                        model.add_row(
                            ..dual_variable.to_f64().unwrap(),
                            [(x_vars[var_index], -1.0), (y_vars[var_index], 1.0)],
                        );
                        invalid_subgraphs.push(invalid_subgraph.clone());

                        for &edge_index in invalid_subgraph.hair.iter() {
                            edge_contributor.get_mut(&edge_index).unwrap().push(var_index);
                            edge_contributors
                                .get_mut(&edge_index)
                                .unwrap()
                                .insert(invalid_subgraph.clone());
                        }
                    }

                    for (&edge_index, &slack) in edge_slacks.iter() {
                        let mut row_entries = vec![];
                        for &var_index in edge_contributor[&edge_index].iter() {
                            row_entries.push((x_vars[var_index], 1.0));
                            row_entries.push((y_vars[var_index], -1.0));
                        }

                        // constraint of edge: sum(y_S) <= weight
                        model.add_row(..=slack.to_f64().unwrap(), row_entries);
                    }

                    let solved = model.solve();

                    let mut direction: BTreeMap<Arc<InvalidSubgraph>, OrderedFloat> = BTreeMap::new();
                    if solved.status() == HighsModelStatus::Optimal {
                        let solution = solved.get_solution();

                        // calculate the objective function
                        let mut res = OrderedFloat::new(0.0);
                        let cols = solution.columns();
                        for i in 0..x_vars.len() {
                            res += OrderedFloat::new(cols[2 * i] - cols[2 * i + 1]);
                        }

                        // check positivity of the objective
                        if !(res.is_positive()) {
                            return (relaxer, true);
                        }

                        for (var_index, invalid_subgraph) in invalid_subgraphs.iter().enumerate() {
                            let overall_growth = cols[2 * var_index] - cols[2 * var_index + 1];
                            if !overall_growth.is_zero() {
                                direction.insert(invalid_subgraph.clone(), OrderedFloat::from(overall_growth));
                            }
                        }
                    } else {
                        println!("solved status: {:?}", solved.status());
                        unreachable!();
                    }

                    // *incr_lp_solution = Some(Arc::new(Mutex::new(IncrLPSolution {
                    //     dual_variables: dual_variables.clone(),
                    //     edge_contributors,
                    //     edge_slacks: edge_slacks.clone(),
                    //     solution: Some(solved),
                    //     current_dual_variables_sum: dual_variables.values().sum(),
                    // })));

                    self.relaxers.insert(relaxer);
                    (Relaxer::new(direction), false)

                */

                let mut model = RowProblem::default().optimise(Sense::Maximise);
                model.set_option("time_limit", 30.0); // stop after 30 seconds
                                                      // model.set_option("parallel", "off"); // do not use multiple cores

                let mut x_vars = vec![];
                let mut invalid_subgraphs = Vec::with_capacity(dual_variables.len());
                let mut edge_contributor: BTreeMap<EdgeIndex, Vec<usize>> =
                    edge_free_weights.keys().map(|&edge_index| (edge_index, vec![])).collect();

                for (var_index, (invalid_subgraph, dual_variable)) in dual_variables.iter().enumerate() {
                    // constraint of the dual variable >= 0
                    let x = model.add_col(1.0, 0.0.., []);
                    x_vars.push(x);

                    invalid_subgraphs.push(invalid_subgraph.clone());

                    for &edge_index in invalid_subgraph.hair.iter() {
                        edge_contributor.get_mut(&edge_index).unwrap().push(var_index);
                    }
                }

                for (&edge_index, &free_weight) in edge_free_weights.iter() {
                    let mut row_entries = vec![];
                    for &var_index in edge_contributor[&edge_index].iter() {
                        row_entries.push((x_vars[var_index], 1.0));
                    }

                    // constraint of edge: sum(y_S) <= weight
                    model.add_row(..=free_weight.to_f64().unwrap(), row_entries);
                }

                let solved = model.solve();

                let mut direction: BTreeMap<Arc<InvalidSubgraph>, OrderedFloat> = BTreeMap::new();
                if solved.status() == HighsModelStatus::Optimal {
                    let solution = solved.get_solution();

                    // calculate the objective function
                    let mut new_dual_variable_sum = OrderedFloat::new(0.0);
                    let cols = solution.columns();
                    for i in 0..x_vars.len() {
                        new_dual_variable_sum += OrderedFloat::new(cols[i]);
                    }

                    let delta: OrderedFloat = new_dual_variable_sum - dual_variables.values().sum::<OrderedFloat>();

                    // check positivity of the objective
                    if !(delta.is_positive()) {
                        return (relaxer, true);
                    }

                    for (var_index, invalid_subgraph) in invalid_subgraphs.iter().enumerate() {
                        let overall_growth =
                            OrderedFloat::from(cols[var_index]) - dual_variables.get(invalid_subgraph).unwrap();
                        if !overall_growth.is_zero() {
                            // println!("inserting: {:?}, {:?}", invalid_subgraph, OrderedFloat::from(overall_growth));
                            direction.insert(invalid_subgraph.clone(), OrderedFloat::from(overall_growth));
                        }
                    }
                } else {
                    println!("solved status: {:?}", solved.status());
                    unreachable!();
                }

                // *incr_lp_solution = Some(Arc::new(Mutex::new(IncrLPSolution {
                //     dual_variables: dual_variables.clone(),
                //     edge_contributors,
                //     edge_slacks: edge_slacks.clone(),
                //     solution: Some(solved),
                //     current_dual_variables_sum: dual_variables.values().sum(),
                // })));

                self.relaxers.insert(relaxer);
                (Relaxer::new(direction), false)
            }
        };
    }
}

#[cfg(test)]
pub mod tests {
    // use super::*;

    // #[test]
    // fn relaxer_optimizer_simple() {
    //     // cargo test relaxer_optimizer_simple -- --nocapture
    //     let mut relaxer_optimizer = RelaxerOptimizer::new();
    // }

    #[cfg(feature = "slp")]
    #[test]
    fn lp_solver_simple() {
        use crate::util::Rational;

        // cargo test lp_solver_simple -- --nocapture
        // https://docs.rs/slp/latest/slp/
        let input = "
        vars x1>=0, y2>=0
        max 2x1+3y2
        subject to
            2x1 +  y2 <= 18,
            6x1 + 5y2 <= 60,
            2x1 + 5y2 <= 40
        ";
        let mut solver = slp::Solver::<Rational>::new(input);
        let solution = solver.solve();
        assert_eq!(
            solution,
            slp::Solution::Optimal(
                Rational::from_integer(28),
                vec![Rational::from_integer(5), Rational::from_integer(6)]
            )
        );
        match solution {
            slp::Solution::Infeasible => println!("INFEASIBLE"),
            slp::Solution::Unbounded => println!("UNBOUNDED"),
            slp::Solution::Optimal(obj, model) => {
                println!("OPTIMAL {}", obj);
                print!("SOLUTION");
                for v in model {
                    print!(" {}", v);
                }
                println!();
            }
        }
    }

    #[cfg(feature = "highs")]
    #[test]
    fn highs_simple() {
        use highs::{ColProblem, HighsModelStatus, Model, Sense};

        let mut model = ColProblem::default().optimise(Sense::Maximise);
        let row1 = model.add_row(..=6., []); // x*3 + y*1 <= 6
        let row2 = model.add_row(..=7., []); // y*1 + z*2 <= 7
        let _x = model.add_col(1., (0.).., [(row1, 3.)]);
        let y = model.add_col(2., (0.).., [(row1, 1.), (row2, 1.)]);
        let _z = model.add_col(1., (0.).., [(row2, 2.)]);

        model.set_option("time_limit", 30.0); // stop after 30 seconds
        model.set_option("parallel", "off"); // do not use multiple cores

        let solved = model.solve();

        assert_eq!(solved.status(), HighsModelStatus::Optimal);

        let solution = solved.get_solution();
        // The expected solution is x=0  y=6  z=0.5
        assert_eq!(solution.columns(), vec![0., 6., 0.5]);
        // All the constraints are at their maximum
        assert_eq!(solution.rows(), vec![6., 7.]);

        // this does nothing but just mark the model as unsolved
        // so that we can modify the problem
        let mut model: Model = solved.into();
        let v = model.add_col(1., (0.)..10., []);

        let _row3 = model.add_row(..=10., [(y, 1.), (v, 2.0)]); // y*1 + v*2 <= 10

        let solved = model.solve();
        assert_eq!(solved.status(), HighsModelStatus::Optimal);

        let solution = solved.get_solution();
        // The expected solution is x=0  y=6  z=0.5
        assert_eq!(solution.columns(), vec![0., 6., 0.5, 2.]);
        // All the constraints are at their maximum
        assert_eq!(solution.rows(), vec![6., 7., 10.]);
        // model.add_row(..=6, row_factors);
    }

    #[cfg(feature = "highs")]
    #[test]
    fn highs_change_incr() {
        use highs::{ColProblem, HighsModelStatus, Model, Sense};
        // max: x + 2y + z
        // under constraints:
        // c1: 3x +  y      <= 6
        // c2:       y + 2z <= 7

        let mut model = ColProblem::default().optimise(Sense::Maximise);
        let c1 = model.add_row(..6., []);
        let c2 = model.add_row(..7., []);
        // x
        model.add_col(1., (0.).., [(c1, 3.)]);
        // y
        model.add_col(2., (0.).., [(c1, 1.), (c2, 1.)]);
        // z
        model.add_col(1., (0.).., [(c2, 2.)]);

        let solved = model.solve();

        assert_eq!(solved.status(), HighsModelStatus::Optimal);

        let solution = solved.get_solution();
        // The expected solution is x=0  y=6  z=0.5
        assert_eq!(solution.columns(), vec![0., 6., 0.5]);
        // All the constraints are at their maximum
        assert_eq!(solution.rows(), vec![6., 7.]);

        // Now we want to change the problem and solve it on top of it
        let mut model: Model = solved.into();

        // modify row c2 to be y + 2z <= 10
        // Now:
        //      max: x + 2y + z
        //      under constraints:
        //      c1: 3x +  y      <= 6
        //      c2:       y + 2z <= 10
        model.change_row_bounds(c2, ..10.);

        let solved = model.solve();

        assert_eq!(solved.status(), HighsModelStatus::Optimal);

        let solution = solved.get_solution();
        // The expected solution is x=0  y=6  z=2
        assert_eq!(solution.columns(), vec![0., 6., 2.]);
        // All the constraints are at their maximum
        assert_eq!(solution.rows(), vec![6., 10.]);
    }
}
