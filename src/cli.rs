use crate::clap;
use crate::clap::{Parser, Subcommand, ValueEnum};
use crate::example_codes::*;
use crate::mwps_solver::*;
use crate::util::*;
use crate::visualize::*;
use more_asserts::assert_le;
use num_traits::FromPrimitive;
use pbr::ProgressBar;
use rand::{thread_rng, Rng};
use serde::Serialize;
use std::env;

#[derive(Parser, Clone)]
#[clap(author = clap::crate_authors!(", "))]
#[clap(version = env!("CARGO_PKG_VERSION"))]
#[clap(about = "Minimum-Weight Parity Subgraph Algorithm for Quantum Error Correction Decoding")]
#[clap(color = clap::ColorChoice::Auto)]
#[clap(propagate_version = true)]
#[clap(subcommand_required = true)]
#[clap(arg_required_else_help = true)]
pub struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Clone)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// benchmark the speed (and also correctness if enabled)
    Benchmark(BenchmarkParameters),
}

#[derive(Parser, Clone)]
pub struct BenchmarkParameters {
    /// code distance
    #[clap(value_parser)]
    d: VertexNum,
    /// physical error rate: the probability of each edge to
    #[clap(value_parser)]
    p: f64,
    /// rounds of noisy measurement, valid only when multiple rounds
    #[clap(short = 'e', long, default_value_t = 0.)]
    pe: f64,
    /// rounds of noisy measurement, valid only when multiple rounds
    #[clap(short = 'n', long, default_value_t = 0)]
    noisy_measurements: VertexNum,
    /// maximum weight of edges
    #[clap(long, default_value_t = 1000)]
    max_weight: Weight,
    /// example code type
    #[clap(short = 'c', long, value_enum, default_value_t = ExampleCodeType::CodeCapacityTailoredCode)]
    code_type: ExampleCodeType,
    /// the configuration of the code builder
    #[clap(long, default_value_t = json!({}))]
    code_config: serde_json::Value,
    /// logging to the default visualizer file at visualize/data/visualizer.json
    #[clap(long, action)]
    enable_visualizer: bool,
    /// print syndrome patterns
    #[clap(long, action)]
    print_syndrome_pattern: bool,
    /// print error patterns
    #[clap(long, action)]
    print_error_pattern: bool,
    /// the method to verify the correctness of the decoding result
    #[clap(long, value_enum, default_value_t = Verifier::ActualError)]
    verifier: Verifier,
    /// the number of iterations to run
    #[clap(short = 'r', long, default_value_t = 1000)]
    total_rounds: usize,
    /// select the combination of primal and dual module
    #[clap(short = 'p', long, value_enum, default_value_t = PrimalDualType::UnionFind)]
    primal_dual_type: PrimalDualType,
    /// the configuration of primal and dual module
    #[clap(long, default_value_t = json!({}))]
    primal_dual_config: serde_json::Value,
    /// message on the progress bar
    #[clap(long, default_value_t = format!(""))]
    pb_message: String,
    /// use deterministic seed for debugging purpose
    #[clap(long, action)]
    use_deterministic_seed: bool,
    /// the benchmark profile output file path
    #[clap(long)]
    benchmark_profiler_output: Option<String>,
    /// skip some iterations, useful when debugging
    #[clap(long, default_value_t = 0)]
    starting_iteration: usize,
}

/// note that these code type is only for example, to test and demonstrate the correctness of the algorithm, but not for real QEC simulation;
/// for real simulation, please refer to <https://github.com/yuewuo/QEC-Playground>
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize, Debug)]
pub enum ExampleCodeType {
    /// quantum repetition code with perfect stabilizer measurement
    CodeCapacityRepetitionCode,
    /// planar code with perfect stabilizer measurement, only one type of stabilizer's decoding graph (thus normal graph)
    CodeCapacityPlanarCode,
    /// color code with perfect stabilizer measurement
    CodeCapacityColorCode,
    /// tailored surface code, which is essentially rotated planar code with depolarizing noise model
    CodeCapacityTailoredCode,
    /// read from error pattern file, generated using option `--primal-dual-type error-pattern-logger`
    ErrorPatternReader,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize, Debug)]
pub enum PrimalDualType {
    /// the solver from Union-Find decoder
    UnionFind,
    /// the sequential solver
    Serial,
    /// log error into a file for later fetch
    ErrorPatternLogger,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize, Debug)]
pub enum Verifier {
    /// disable verifier
    None,
    /// use the serial version of fusion blossom library to verify
    FusionSerial,
    /// if the actual error has smaller weight than the lower bound of the weight range then fail
    ActualError,
    /// if the actual error has smaller weight than the solved subgraph then fail
    StrictActualError,
}

impl Cli {
    pub fn run(self) {
        match self.command {
            Commands::Benchmark(BenchmarkParameters {
                d,
                p,
                pe,
                noisy_measurements,
                max_weight,
                code_type,
                enable_visualizer,
                verifier,
                total_rounds,
                primal_dual_type,
                pb_message,
                primal_dual_config,
                code_config,
                use_deterministic_seed,
                benchmark_profiler_output,
                print_syndrome_pattern,
                starting_iteration,
                print_error_pattern,
            }) => {
                // whether to disable progress bar, useful when running jobs in background
                let disable_progress_bar = env::var("DISABLE_PROGRESS_BAR").is_ok();
                let mut code: Box<dyn ExampleCode> = code_type.build(d, p, noisy_measurements, max_weight, code_config);
                if pe != 0. {
                    code.set_erasure_probability(pe);
                }
                if enable_visualizer {
                    // print visualizer file path only once
                    print_visualize_link(static_visualize_data_filename());
                }
                // create initializer and solver
                let initializer = code.get_initializer();
                let mut primal_dual_solver = primal_dual_type.build(&initializer, &*code, primal_dual_config);
                let mut result_verifier = verifier.build(&initializer);
                let mut benchmark_profiler = BenchmarkProfiler::new(noisy_measurements, benchmark_profiler_output);
                // prepare progress bar display
                let mut pb = if !disable_progress_bar {
                    let mut pb = ProgressBar::on(std::io::stderr(), total_rounds as u64);
                    pb.message(format!("{pb_message} ").as_str());
                    Some(pb)
                } else {
                    if !pb_message.is_empty() {
                        print!("{pb_message} ");
                    }
                    None
                };
                let mut rng = thread_rng();
                for round in (starting_iteration as u64)..(total_rounds as u64) {
                    pb.as_mut().map(|pb| pb.set(round));
                    let seed = if use_deterministic_seed { round } else { rng.gen() };
                    let (syndrome_pattern, error_pattern) = code.generate_random_errors(seed);
                    if print_syndrome_pattern {
                        println!("syndrome_pattern: {:?}", syndrome_pattern);
                    }
                    if print_error_pattern {
                        println!("error_pattern: {:?}", error_pattern);
                    }
                    // create a new visualizer each round
                    let mut visualizer = None;
                    if enable_visualizer {
                        let new_visualizer = Visualizer::new(
                            Some(visualize_data_folder() + static_visualize_data_filename().as_str()),
                            code.get_positions(),
                            true,
                        )
                        .unwrap();
                        visualizer = Some(new_visualizer);
                    }
                    benchmark_profiler.begin(&syndrome_pattern, &error_pattern);
                    primal_dual_solver.solve_visualizer(&syndrome_pattern, visualizer.as_mut());
                    benchmark_profiler.event("decoded".to_string());
                    result_verifier.verify(
                        &mut primal_dual_solver,
                        &syndrome_pattern,
                        &error_pattern,
                        visualizer.as_mut(),
                    );
                    benchmark_profiler.event("verified".to_string());
                    primal_dual_solver.clear(); // also count the clear operation
                    benchmark_profiler.end(Some(&*primal_dual_solver));
                    if let Some(pb) = pb.as_mut() {
                        if pb_message.is_empty() {
                            pb.message(format!("{} ", benchmark_profiler.brief()).as_str());
                        }
                    }
                }
                if disable_progress_bar {
                    // always print out brief
                    println!("{}", benchmark_profiler.brief());
                } else {
                    if let Some(pb) = pb.as_mut() {
                        pb.finish()
                    }
                    println!();
                }
            }
        }
    }
}

impl ExampleCodeType {
    fn build(
        &self,
        d: VertexNum,
        p: f64,
        _noisy_measurements: VertexNum,
        max_weight: Weight,
        mut code_config: serde_json::Value,
    ) -> Box<dyn ExampleCode> {
        match self {
            Self::CodeCapacityRepetitionCode => {
                assert_eq!(code_config, json!({}), "config not supported");
                Box::new(CodeCapacityRepetitionCode::new(d, p, max_weight))
            }
            Self::CodeCapacityPlanarCode => {
                assert_eq!(code_config, json!({}), "config not supported");
                Box::new(CodeCapacityPlanarCode::new(d, p, max_weight))
            }
            Self::CodeCapacityTailoredCode => {
                let mut pxy = 0.; // default to infinite bias
                let config = code_config.as_object_mut().expect("config must be JSON object");
                if let Some(value) = config.remove("pxy") {
                    pxy = value.as_f64().expect("code_count number");
                }
                Box::new(CodeCapacityTailoredCode::new(d, pxy, p, max_weight))
            }
            Self::CodeCapacityColorCode => {
                assert_eq!(code_config, json!({}), "config not supported");
                Box::new(CodeCapacityColorCode::new(d, p, max_weight))
            }
            Self::ErrorPatternReader => Box::new(ErrorPatternReader::new(code_config)),
        }
    }
}

impl PrimalDualType {
    fn build(
        &self,
        initializer: &SolverInitializer,
        code: &dyn ExampleCode,
        primal_dual_config: serde_json::Value,
    ) -> Box<dyn PrimalDualSolver> {
        match self {
            Self::UnionFind => {
                assert_eq!(primal_dual_config, json!({}));
                Box::new(SolverUnionFind::new(initializer))
            }
            Self::Serial => {
                unimplemented!()
                // assert_eq!(primal_dual_config, json!({}));
                // Box::new(SolverSerial::new(initializer))
            }
            Self::ErrorPatternLogger => Box::new(SolverErrorPatternLogger::new(initializer, code, primal_dual_config)),
        }
    }
}

impl Verifier {
    fn build(&self, initializer: &SolverInitializer) -> Box<dyn ResultVerifier> {
        match self {
            Self::None => Box::new(VerifierNone {}),
            Self::FusionSerial => Box::new(VerifierFusionSerial {
                initializer: initializer.clone(),
            }),
            Self::ActualError => Box::new(VerifierActualError {
                initializer: initializer.clone(),
                is_strict: false,
            }),
            Self::StrictActualError => Box::new(VerifierActualError {
                initializer: initializer.clone(),
                is_strict: true,
            }),
        }
    }
}

trait ResultVerifier {
    fn verify(
        &mut self,
        primal_dual_solver: &mut Box<dyn PrimalDualSolver>,
        syndrome_pattern: &SyndromePattern,
        error_pattern: &Subgraph,
        visualizer: Option<&mut Visualizer>,
    );
}

struct VerifierNone {}

impl ResultVerifier for VerifierNone {
    fn verify(
        &mut self,
        _primal_dual_solver: &mut Box<dyn PrimalDualSolver>,
        _syndrome_pattern: &SyndromePattern,
        _error_pattern: &Subgraph,
        _visualizer: Option<&mut Visualizer>,
    ) {
    }
}

struct VerifierFusionSerial {
    pub initializer: SolverInitializer,
}

impl ResultVerifier for VerifierFusionSerial {
    fn verify(
        &mut self,
        _primal_dual_solver: &mut Box<dyn PrimalDualSolver>,
        _syndrome_pattern: &SyndromePattern,
        _error_pattern: &Subgraph,
        _visualizer: Option<&mut Visualizer>,
    ) {
        println!("{}", self.initializer.vertex_num);
        unimplemented!()
    }
}

struct VerifierActualError {
    initializer: SolverInitializer,
    pub is_strict: bool,
}

impl ResultVerifier for VerifierActualError {
    fn verify(
        &mut self,
        primal_dual_solver: &mut Box<dyn PrimalDualSolver>,
        syndrome_pattern: &SyndromePattern,
        error_pattern: &Subgraph,
        visualizer: Option<&mut Visualizer>,
    ) {
        if !syndrome_pattern.erasures.is_empty() {
            unimplemented!()
        }
        let actual_weight = Rational::from_usize(self.initializer.get_subgraph_total_weight(error_pattern)).unwrap();
        let (subgraph, weight_range) = primal_dual_solver.subgraph_range_visualizer(visualizer);
        assert!(
            self.initializer
                .matches_subgraph_syndrome(&subgraph, &syndrome_pattern.defect_vertices),
            "bug: the result subgraph does not match the syndrome"
        );
        assert_le!(
            weight_range.lower,
            actual_weight,
            "the lower bound of weight range is larger than the actual weight"
        );
        if self.is_strict {
            let subgraph_weight = Rational::from_usize(self.initializer.get_subgraph_total_weight(&subgraph)).unwrap();
            assert_le!(subgraph_weight, actual_weight, "it's not a minimum-weight parity subgraph: the actual error pattern has smaller weight, range: {weight_range:?}");
            assert_eq!(
                weight_range.lower, weight_range.upper,
                "the weight range must be optimal: lower = upper"
            );
        }
    }
}
