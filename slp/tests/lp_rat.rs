use slp::Solution::{self, Infeasible, Optimal, Unbounded};
use slp::*;
use slp::{Rational64, SolverSettings};

fn check_ans(expected: Solution<Rational64>, actual: Solution<Rational64>) {
    assert!(
        expected == actual,
        "Expected {:?} but got {:?}",
        expected,
        actual
    );
}

#[test]
fn test_lp_1() {
    let input = "vars x>=0, y>=0, z>=0
        min 18x+60y+40z subject to
        2x+6y+2z>=2,
        x+5y+5z>=3";
    let mut solver: Solver<Rational64> = slp::parser::parse_lp_problem::<Rational64>(&input)
        .unwrap()
        .into();
    println!("entrei: {:?}", solver);
    check_ans(
        Optimal(
            Rational64::from_integer(28),
            vec![
                Rational64::from_integer(0),
                Rational64::new(1, 5),
                Rational64::new(2, 5),
            ],
        ),
        solver.solve(),
    );
}

#[test]
fn test_lp_1_par() {
    let input = "vars x>=0, y>=0, z>=0
        min 18x+60y+40z subject to
        2x+6y+2z>=2,
        x+5y+5z>=3";
    let mut solver: Solver<Rational64> = slp::parser::parse_lp_problem::<Rational64>(&input)
        .unwrap()
        .into();
    solver.setting(SolverSettings::EnableDataParallelism);
    check_ans(
        Optimal(
            Rational64::from_integer(28),
            vec![
                Rational64::from_integer(0),
                Rational64::new(1, 5),
                Rational64::new(2, 5),
            ],
        ),
        solver.solve(),
    );
}

#[test]
fn test_lp_2() {
    let input = "vars x>=0, y>=0 min 6x+3y subject to x+y>=1, 2x-y>=1, 3y<=2";
    let mut solver: Solver<Rational64> = slp::parser::parse_lp_problem::<Rational64>(&input)
        .unwrap()
        .into();
    check_ans(
        Optimal(
            Rational64::from_integer(5),
            vec![Rational64::new(2, 3), Rational64::new(1, 3)],
        ),
        solver.solve(),
    );
}

#[test]
fn test_lp_2_par() {
    let input = "vars x>=0, y>=0 min 6x+3y subject to x+y>=1, 2x-y>=1, 3y<=2";
    let mut solver: Solver<Rational64> = slp::parser::parse_lp_problem::<Rational64>(&input)
        .unwrap()
        .into();
    solver.setting(SolverSettings::EnableDataParallelism);
    check_ans(
        Optimal(
            Rational64::from_integer(5),
            vec![Rational64::new(2, 3), Rational64::new(1, 3)],
        ),
        solver.solve(),
    );
}

#[test]
fn test_lp_3() {
    let input = "vars x>=0, y>=0 min 6x+3y subject to x+y>=1, 2x-y>=-5, 3y<=-1";
    let mut solver: Solver<Rational64> = slp::parser::parse_lp_problem::<Rational64>(&input)
        .unwrap()
        .into();
    check_ans(Infeasible, solver.solve());
}

#[test]
fn test_lp_3_par() {
    let input = "vars x>=0, y>=0 min 6x+3y subject to x+y>=1, 2x-y>=-5, 3y<=-1";
    let mut solver: Solver<Rational64> = slp::parser::parse_lp_problem::<Rational64>(&input)
        .unwrap()
        .into();
    solver.setting(SolverSettings::EnableDataParallelism);
    check_ans(Infeasible, solver.solve());
}

#[test]
fn test_lp_4() {
    let input = "vars x>=0, y>=0 max -6x-3y subject to x+y>=1, 2x-y>=-5, -3y<=-1";
    let mut solver: Solver<Rational64> = slp::parser::parse_lp_problem::<Rational64>(&input)
        .unwrap()
        .into();
    check_ans(
        Optimal(
            Rational64::from_integer(-3),
            vec![Rational64::from_integer(0), Rational64::from_integer(1)],
        ),
        solver.solve(),
    );
}

#[test]
fn test_lp_4_par() {
    let input = "vars x>=0, y>=0 max -6x-3y subject to x+y>=1, 2x-y>=-5, -3y<=-1";
    let mut solver: Solver<Rational64> = slp::parser::parse_lp_problem::<Rational64>(&input)
        .unwrap()
        .into();
    solver.setting(SolverSettings::EnableDataParallelism);
    check_ans(
        Optimal(
            Rational64::from_integer(-3),
            vec![Rational64::from_integer(0), Rational64::from_integer(1)],
        ),
        solver.solve(),
    );
}

#[test]
fn test_lp_5() {
    let input = "vars x>=0, y>=0 max 6x-3y subject to x+y>=1, 2x-y>=-5, -3y<=-1";
    let mut solver: Solver<Rational64> = slp::parser::parse_lp_problem::<Rational64>(&input)
        .unwrap()
        .into();
    check_ans(Unbounded, solver.solve());
}

#[test]
fn test_lp_5_par() {
    let input = "vars x>=0, y>=0 max 6x-3y subject to x+y>=1, 2x-y>=-5, -3y<=-1";
    let mut solver: Solver<Rational64> = slp::parser::parse_lp_problem::<Rational64>(&input)
        .unwrap()
        .into();
    solver.setting(SolverSettings::EnableDataParallelism);
    check_ans(Unbounded, solver.solve());
}