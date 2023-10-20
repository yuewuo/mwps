use slp::*;
use slp::{Rational64, Solution, SolverSettings};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name="slp", about=env!("CARGO_PKG_DESCRIPTION"), version=env!("CARGO_PKG_VERSION"),
setting=structopt::clap::AppSettings::ColoredHelp)]
struct Opt {
    /// Input file
    #[structopt(parse(from_os_str))]
    file: PathBuf,
    /// Enable data parallelism
    #[structopt(short, long)]
    parallel: bool,
    /// Use Rational64
    #[structopt(short, long)]
    rat64: bool,
}

impl Opt {
    fn print(&self) {
        // TODO: Improve printing, add verbosity
        // println!("Data Parallelism: {}", self.parallel);
        // println!("Type: {}", if self.rat64 { "Rational<i64>" } else { "f64" });
    }
}

fn print_solution<T: std::fmt::Display>(solution: Solution<T>) {
    use slp::Solution::*;
    match solution {
        Infeasible => println!("INFEASIBLE"),
        Unbounded => println!("UNBOUNDED"),
        Optimal(obj, model) => {
            println!("OPTIMAL {}", obj);
            print!("SOLUTION");
            for v in model {
                print!(" {}", v);
            }
            println!();
        }
    }
}

fn run_f64(opt: &Opt) {
    let unparsed_input = std::fs::read_to_string(&opt.file).unwrap();
    let solver = slp::parser::parse_lp_problem::<f64>(&unparsed_input);
    if let Err(e) = solver {
        println!(
            "Error parsing input file: {}\n{}",
            opt.file.to_str().unwrap(),
            e
        );
    } else {
        let mut solver: Solver<f64> = solver.unwrap().into();

        if opt.parallel {
            solver.setting(SolverSettings::EnableDataParallelism);
        }

        print_solution(solver.solve());
    }
}

fn run_r64(opt: &Opt) {
    let unparsed_input = std::fs::read_to_string(&opt.file).unwrap();
    let solver = slp::parser::parse_lp_problem::<Rational64>(&unparsed_input);
    if let Err(e) = solver {
        println!(
            "Error parsing input file: {}\n{}",
            opt.file.to_str().unwrap(),
            e
        );
    } else {
        let mut solver: Solver<Rational64> = solver.unwrap().into();

        if opt.parallel {
            solver.setting(SolverSettings::EnableDataParallelism);
        }

        print_solution(solver.solve());
    }
}

fn main() {
    let opt = Opt::from_args();
    opt.print();

    if opt.rat64 {
        run_r64(&opt);
    } else {
        run_f64(&opt);
    }
}