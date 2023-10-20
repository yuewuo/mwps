use slp::Solution::{self, Infeasible, Optimal, Unbounded};
use slp::SolverSettings;
use slp::*;
use std::f32::EPSILON;

#[derive(Debug)]
pub struct SolverOptions {
    pub parallel: bool,
}

fn check_ans(expected: Solution<f32>, actual: Solution<f32>) {
    match (&expected, &actual) {
        (Optimal(exp_obj, exp_model), Optimal(act_obj, act_model)) => {
            assert!(
                (exp_obj - act_obj).abs() < 10.0 * EPSILON,
                "Failed {} == {}",
                exp_obj,
                act_obj,
            );
            assert!(exp_model.len() == act_model.len(), "Model length not same");
            exp_model.iter().zip(act_model.iter()).for_each(|x| {
                assert!(
                    (x.0 - x.1).abs() < 10.0 * EPSILON,
                    "Failed {} == {}",
                    x.0,
                    x.1
                );
            });
        }
        (exp, act) => {
            if exp != act {
                assert!(false, "Expected {:?} but got {:?}", exp, act)
            }
        }
    }
}

#[test]
fn test_lp_1() {
    let input = "vars x>=0, y>=0, z>=0
        min 18x+60y+40z subject to
        2x+6y+2z>=2,
        x+5y+5z>=3";
    let mut solver: Solver<f32> = slp::parser::parse_lp_problem::<f32>(&input).unwrap().into();
    
    
    println!("entrei: {:?}", solver);
    let lp_aux: slp::LP<f32> = LP {
        n_constraints: 2,
        n_vars: 3,
        basic_indices: vec![0, 4, 5],
        tableau: vec![vec![0.0, 18.0, 60.0, 40.0, 0.0, 0.0], vec![-2.0, -2.0, -6.0, -2.0, 1.0, 0.0], vec![-3.0, -1.0, -5.0, -5.0, 0.0, 1.0]],
    };

    
    let mut solver: Solver<f32> =  slp::Solver::new_with_int_constraints(lp_aux.clone(), vec![false, false, false], true);

    /*pub(crate) fn new_with_int_constraints(
        lp: LP<N>,
        is_int_constraints: Vec<bool>,
        negate_objective: bool,
    ) -> Self {
        Solver {
            lp,
            options: SolverOptions { parallel: false },
            is_int_constraints,
            negate_objective,
        }
    }*/

    /*let mut solver_aux =  Solver {
        lp: lp_aux,
        options: SolverOptions { parallel: false },
        is_int_constraints: vec![false, false, false],
        negate_objective: true,
    };*/
    println!("entrei: {:?}", lp_aux);
    check_ans(Optimal(28.0, vec![0.0, 0.2, 0.4]), solver.solve());
}

#[test]
fn test_lp_1_par() {
    let input = "vars x>=0, y>=0, z>=0
        min 18x+60y+40z subject to
        2x+6y+2z>=2,
        x+5y+5z>=3";
    let mut solver: Solver<f32> = slp::parser::parse_lp_problem::<f32>(&input).unwrap().into();
    solver.setting(SolverSettings::EnableDataParallelism);
    check_ans(Optimal(28.0, vec![0.0, 0.2, 0.4]), solver.solve());
}

#[test]
fn test_lp_2() {
    let input = "vars x>=0, y>=0 min 6x+3y subject to x+y>=1, 2x-y>=1, 3y<=2";
    let mut solver: Solver<f32> = slp::parser::parse_lp_problem::<f32>(&input).unwrap().into();
    check_ans(Optimal(5.0, vec![2.0 / 3.0, 1.0 / 3.0]), solver.solve());
}

#[test]
fn test_lp_2_par() {
    let input = "vars x>=0, y>=0 min 6x+3y subject to x+y>=1, 2x-y>=1, 3y<=2";
    let mut solver: Solver<f32> = slp::parser::parse_lp_problem::<f32>(&input).unwrap().into();
    solver.setting(SolverSettings::EnableDataParallelism);
    check_ans(Optimal(5.0, vec![2.0 / 3.0, 1.0 / 3.0]), solver.solve());
}

#[test]
fn test_lp_3() {
    let input = "vars x>=0, y>=0 min 6x+3y subject to x+y>=1, 2x-y>=-5, 3y<=-1";
    let mut solver: Solver<f32> = slp::parser::parse_lp_problem::<f32>(&input).unwrap().into();
    check_ans(Infeasible, solver.solve());
}

#[test]
fn test_lp_3_par() {
    let input = "vars x>=0, y>=0 min 6x+3y subject to x+y>=1, 2x-y>=-5, 3y<=-1";
    let mut solver: Solver<f32> = slp::parser::parse_lp_problem::<f32>(&input).unwrap().into();
    solver.setting(SolverSettings::EnableDataParallelism);
    check_ans(Infeasible, solver.solve());
}

#[test]
fn test_lp_4() {
    let input = "vars x>=0, y>=0 max -6x-3y subject to x+y>=1, 2x-y>=-5, -3y<=-1";
    let mut solver: Solver<f32> = slp::parser::parse_lp_problem::<f32>(&input).unwrap().into();
    check_ans(Optimal(-3.0, vec![0.0, 1.0]), solver.solve());
}

#[test]
fn test_lp_4_par() {
    let input = "vars x>=0, y>=0 max -6x-3y subject to x+y>=1, 2x-y>=-5, -3y<=-1";
    let mut solver: Solver<f32> = slp::parser::parse_lp_problem::<f32>(&input).unwrap().into();
    solver.setting(SolverSettings::EnableDataParallelism);
    check_ans(Optimal(-3.0, vec![0.0, 1.0]), solver.solve());
}

#[test]
fn test_lp_5() {
    let input = "vars x>=0, y>=0 max 6x-3y subject to x+y>=1, 2x-y>=-5, -3y<=-1";
    let mut solver: Solver<f32> = slp::parser::parse_lp_problem::<f32>(&input).unwrap().into();
    check_ans(Unbounded, solver.solve());
}

#[test]
fn test_lp_5_par() {
    let input = "vars x>=0, y>=0 max 6x-3y subject to x+y>=1, 2x-y>=-5, -3y<=-1";
    let mut solver: Solver<f32> = slp::parser::parse_lp_problem::<f32>(&input).unwrap().into();
    solver.setting(SolverSettings::EnableDataParallelism);
    check_ans(Unbounded, solver.solve());
}