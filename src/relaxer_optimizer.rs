//! Relaxer Optimizer
//!
//! It's possible that two (or more) positive relaxers are bouncing, and the actual growth
//! is exponentially smaller but never reaching the optimal. In this case, we need
//! this module to optimize the positive relaxers. This only takes effect when such bouncing
//! is detected, and remains minimum in all other cases to avoid reduce time complexity.
//!

use crate::dual_module;
use crate::dual_module::DualModuleImpl;
use crate::dual_module::DualModuleInterfacePtr;
use crate::invalid_subgraph::*;
use crate::pointers::RwLockPtr;
use crate::primal_module;
use crate::primal_module::PrimalModuleImpl;
use crate::primal_module_serial::PrimalCluster;
use crate::primal_module_serial::PrimalClusterPtr;
use crate::primal_module_serial::PrimalModuleSerialNode;
use crate::primal_module_serial::PrimalModuleSerialNodePtr;
use crate::relaxer::*;
use crate::util::*;
use derivative::Derivative;
use num_traits::Signed;
use num_traits::{One, Zero};
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

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

    pub fn optimize(
        &mut self,
        relaxer: Relaxer,
        edge_slacks: BTreeMap<EdgeIndex, Rational>,
        mut dual_variables: BTreeMap<Arc<InvalidSubgraph>, Rational>,
    ) -> Relaxer {
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
        let mut solver = slp::Solver::<slp::Ratio<slp::BigInt>>::new(&input);
        let solution = solver.solve();
        let mut direction: BTreeMap<Arc<InvalidSubgraph>, Rational> = BTreeMap::new();
        match solution {
            slp::Solution::Optimal(optimal_objective, model) => {
                if !optimal_objective.is_positive() {
                    return relaxer;
                }
                for (var_index, (invalid_subgraph, _)) in dual_variables.iter().enumerate() {
                    let overall_growth = model[var_index].clone() - model[var_index + x_vars.len()].clone();
                    if !overall_growth.is_zero() {
                        direction.insert(
                            invalid_subgraph.clone(),
                            Rational::from_str(&overall_growth.numer().to_string()).unwrap()
                                / Rational::from_str(&overall_growth.denom().to_string()).unwrap(),
                        );
                    }
                }
            }
            _ => unreachable!(),
        }
        self.relaxers.insert(relaxer);
        Relaxer::new(direction)
    }

    pub fn optimize_tune<D: DualModuleImpl>(
        &mut self,
        relaxer: Relaxer,
        edge_slacks: BTreeMap<EdgeIndex, Rational>,
        mut dual_variables: BTreeMap<Arc<InvalidSubgraph>, Rational>,
        dual_module: &mut D,
        interface_ptr: &DualModuleInterfacePtr,
    ) -> Relaxer {
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
        let mut solver = slp::Solver::<slp::Ratio<slp::BigInt>>::new(&input);
        let solution = solver.solve();
        let mut direction: BTreeMap<Arc<InvalidSubgraph>, Rational> = BTreeMap::new();
        match solution {
            slp::Solution::Optimal(optimal_objective, model) => {
                if !optimal_objective.is_positive() {
                    return relaxer;
                }
                for (var_index, (invalid_subgraph, _)) in dual_variables.into_iter().enumerate() {
                    let overall_growth = model[var_index].clone() - model[var_index + x_vars.len()].clone();
                    if !overall_growth.is_zero() {
                        // let (existing, dual_node_ptr) = interface_ptr.find_or_create_node(invalid_subgraph, dual_module);
                        // if !existing {
                        // //create the corresponding primal node and add it to cluster
                        // let primal_node_ptr = PrimalModuleSerialNodePtr::new_value(PrimalModuleSerialNode {
                        //     dual_node_ptr: dual_node_ptr.clone(),
                        //     cluster_weak: cluster_ptr.downgrade(),
                        // });
                        // cluster.nodes.push(primal_node_ptr.clone());
                        // primal_module.push_node(primal_node_ptr);
                        // println!("created dual_node: {:?}", dual_node_ptr.read_recursive().index);

                        // // return it out
                        // }

                        // println!("Overall growth: {}/{}", overall_growth.numer(), overall_growth.denom());
                        direction.insert(
                            // NOTE: Should care during benchmark, how much time does it take to serialize it to string and convert it back? Seems Redundant
                            invalid_subgraph,
                            Rational::from_str(&overall_growth.numer().to_string()).unwrap()
                                / Rational::from_str(&overall_growth.denom().to_string()).unwrap(),
                        );

                        // let dual_node_write = dual_node_ptr.write();
                        // dual_node_write.dual_variable_at_last_updated_time =
                        //     Rational::from_str(&final_val.numer().to_string()).unwrap()
                        //         / Rational::from_str(&final_val.denom().to_string()).unwrap();

                        // dual_module.get_edges_for_node(dual_node_ptr)
                        // for edge_index in dual_node_write.invalid_subgraph.hair.iter() {
                        //     let edge_slack = edge_slacks.get(edge_index).unwrap();
                        //     dual_node_write.edge_slacks.insert(edge_index.clone(), edge_slack.clone());
                        // }
                    }
                }
            }
            _ => unreachable!(),
        }
        // println!("Old Directions:");
        // for (invalid_subgraph, speed) in relaxer.get_direction().iter() {
        //     println!("{:?}: {}/{}", invalid_subgraph, speed.numer(), speed.denom());
        // }
        self.relaxers.insert(relaxer);
        // print out each direction on a single line
        // println!("Directions:");
        // for (invalid_subgraph, speed) in direction.iter() {
        //     println!("{:?}: {}/{}", invalid_subgraph, speed.numer(), speed.denom());
        // }
        Relaxer::new(direction)
    }
}

#[cfg(test)]
#[cfg(feature = "highs")]
pub mod tests {
    // use super::*;
    use highs::{ColProblem, HighsModelStatus, Model, Sense};

    // #[test]
    // fn relaxer_optimizer_simple() {
    //     // cargo test relaxer_optimizer_simple -- --nocapture
    //     let mut relaxer_optimizer = RelaxerOptimizer::new();
    // }

    #[test]
    fn lp_solver_simple() {
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
        let mut solver = slp::Solver::<slp::Rational>::new(input);
        let solution = solver.solve();
        assert_eq!(
            solution,
            slp::Solution::Optimal(
                slp::Rational::from_integer(28),
                vec![slp::Rational::from_integer(5), slp::Rational::from_integer(6)]
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

    #[test]
    fn highs_simple() {
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
}
