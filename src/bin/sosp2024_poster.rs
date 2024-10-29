// cargo run --release --features qecp_integrate --bin sosp2024_poster

use mwpf::example_codes::*;
use mwpf::visualize::*;

fn code_capacity_example() {
    let visualize_filename = "sosp2024_code_capacity_example.json".to_string();
    let code = CodeCapacityTailoredCode::new(5, 0.001, 0.001, 100);
    code.sanity_check().unwrap();
    let mut visualizer = Visualizer::new(
        Some(visualize_data_folder() + visualize_filename.as_str()),
        code.get_positions(),
        true,
    )
    .unwrap();
    print_visualize_link(visualize_filename.clone());
    visualizer.snapshot("code".to_string(), &code).unwrap();
}

#[cfg(feature = "qecp_integrate")]
fn circuit_level_example() {
    let visualize_filename = "sosp2024_circuit_level_example.json".to_string();
    let code = QECPlaygroundCode::new(
        5,
        0.001,
        serde_json::json!({
            "nm": 4,
            "code_type": "RotatedPlanarCode",
            "noise_model": "StimNoiseModel",
            "max_weight": 100,
        }),
    );
    let mut visualizer = Visualizer::new(
        Some(visualize_data_folder() + visualize_filename.as_str()),
        code.get_positions(),
        true,
    )
    .unwrap();
    print_visualize_link(visualize_filename.clone());
    visualizer.snapshot("code".to_string(), &code).unwrap();
}

fn main() {
    assert!(
        cfg!(feature = "qecp_integrate"),
        "cargo run --release --features qecp_integrate --bin sosp2024_poster"
    );
    code_capacity_example();
    #[cfg(feature = "qecp_integrate")]
    circuit_level_example();
}
