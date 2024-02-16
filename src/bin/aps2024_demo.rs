// cargo run --release --bin aps2024_demo

use mwpf::dual_module::*;
use mwpf::dual_module_serial::*;
use mwpf::example_codes::*;
use mwpf::invalid_subgraph::InvalidSubgraph;
use mwpf::model_hypergraph::*;
use mwpf::primal_module::*;
use mwpf::primal_module_serial::*;
use mwpf::util::*;
use mwpf::visualize::*;
use num_traits::cast::FromPrimitive;
use std::sync::Arc;
use sugar::*;

fn debug_demo() {
    for is_example in [true, false] {
        let visualize_filename = format!("aps2024_debug_demo{}.json", if is_example { "_ex" } else { "" });
        let mut code = CodeCapacityTailoredCode::new(3, 0., 0.01, 1);
        let initializer = code.get_initializer();
        let model_graph = Arc::new(ModelHyperGraph::new(Arc::new(initializer.clone())));
        let mut dual_module = DualModuleSerial::new_empty(&initializer);
        let interface_ptr = DualModuleInterfacePtr::new(model_graph.clone());
        code.set_physical_errors(&[4]);
        let syndrome_pattern = Arc::new(code.get_syndrome());
        let mut visualizer = Visualizer::new(
            Some(visualize_data_folder() + visualize_filename.as_str()),
            code.get_positions(),
            true,
        )
        .unwrap();
        print_visualize_link(visualize_filename.clone());
        if is_example {
            visualizer.snapshot_combined("code".to_string(), vec![&code]).unwrap();
            let mut primal_module = PrimalModuleSerial::new_empty(&initializer);
            primal_module.growing_strategy = GrowingStrategy::SingleCluster;
            primal_module.plugins = Arc::new(vec![]);
            primal_module.solve_visualizer(&interface_ptr, syndrome_pattern, &mut dual_module, Some(&mut visualizer));
            let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module);
            visualizer
                .snapshot_combined(
                    "subgraph".to_string(),
                    vec![&interface_ptr, &dual_module, &subgraph, &weight_range],
                )
                .unwrap();
        } else {
            // manually solve it to have fine control
            interface_ptr.write().decoding_graph.set_syndrome(syndrome_pattern.clone());
            for vertex_index in syndrome_pattern.defect_vertices.iter() {
                dual_module.vertices[*vertex_index].write().is_defect = true;
            }
            visualizer
                .snapshot_combined("begin".to_string(), vec![&interface_ptr, &dual_module])
                .unwrap();
            let decoding_graph = interface_ptr.read_recursive().decoding_graph.clone();
            let s0 = Arc::new(InvalidSubgraph::new_complete(btreeset! {3}, btreeset! {}, &decoding_graph));
            let (_, s0_ptr) = interface_ptr.find_or_create_node(&s0, &mut dual_module);
            dual_module.set_grow_rate(&s0_ptr, Rational::from_usize(1).unwrap());
            for _ in 0..3 {
                dual_module.grow(Rational::new_raw(1.into(), 3.into()));
                visualizer
                    .snapshot_combined("grow 1/3".to_string(), vec![&interface_ptr, &dual_module])
                    .unwrap();
            }
            // create another node
            let s1 = Arc::new(InvalidSubgraph::new_complete(btreeset! {6}, btreeset! {}, &decoding_graph));
            let (_, s1_ptr) = interface_ptr.find_or_create_node(&s1, &mut dual_module);
            dual_module.set_grow_rate(&s0_ptr, -Rational::from_usize(1).unwrap());
            dual_module.set_grow_rate(&s1_ptr, Rational::from_usize(1).unwrap());
            for _ in 0..3 {
                dual_module.grow(Rational::new_raw(1.into(), 3.into()));
                visualizer
                    .snapshot_combined("grow 1/3".to_string(), vec![&interface_ptr, &dual_module])
                    .unwrap();
            }
            visualizer
                .snapshot_combined(
                    "subgraph".to_string(),
                    vec![&interface_ptr, &dual_module, &Subgraph::from(vec![4])],
                )
                .unwrap();
        }
    }
}

fn simple_demo() {
    for is_example in [true, false] {
        let visualize_filename = format!("aps2024_simple_demo{}.json", if is_example { "_ex" } else { "" });
        let mut code = CodeCapacityTailoredCode::new(3, 0., 0.01, 1);
        let initializer = code.get_initializer();
        let model_graph = Arc::new(ModelHyperGraph::new(Arc::new(initializer.clone())));
        let mut dual_module = DualModuleSerial::new_empty(&initializer);
        let interface_ptr = DualModuleInterfacePtr::new(model_graph.clone());
        code.set_physical_errors(&[4]);
        let syndrome_pattern = Arc::new(code.get_syndrome());
        let mut visualizer = Visualizer::new(
            Some(visualize_data_folder() + visualize_filename.as_str()),
            code.get_positions(),
            true,
        )
        .unwrap();
        print_visualize_link(visualize_filename.clone());
        if is_example {
            visualizer.snapshot_combined("code".to_string(), vec![&code]).unwrap();
            let mut primal_module = PrimalModuleSerial::new_empty(&initializer);
            primal_module.growing_strategy = GrowingStrategy::SingleCluster;
            primal_module.plugins = Arc::new(vec![]);
            primal_module.solve_visualizer(&interface_ptr, syndrome_pattern, &mut dual_module, Some(&mut visualizer));
            let (subgraph, weight_range) = primal_module.subgraph_range(&interface_ptr, &mut dual_module);
            visualizer
                .snapshot_combined(
                    "subgraph".to_string(),
                    vec![&interface_ptr, &dual_module, &subgraph, &weight_range],
                )
                .unwrap();
        } else {
            // manually solve it to have fine control
            interface_ptr.write().decoding_graph.set_syndrome(syndrome_pattern.clone());
            for vertex_index in syndrome_pattern.defect_vertices.iter() {
                dual_module.vertices[*vertex_index].write().is_defect = true;
            }
            visualizer
                .snapshot_combined("begin".to_string(), vec![&interface_ptr, &dual_module])
                .unwrap();
            let decoding_graph = interface_ptr.read_recursive().decoding_graph.clone();
            let s0 = Arc::new(InvalidSubgraph::new_complete(btreeset! {3}, btreeset! {}, &decoding_graph));
            let (_, s0_ptr) = interface_ptr.find_or_create_node(&s0, &mut dual_module);
            dual_module.set_grow_rate(&s0_ptr, Rational::from_usize(1).unwrap());
            visualizer
                .snapshot_combined("create s0".to_string(), vec![&interface_ptr, &dual_module])
                .unwrap();
            for _ in 0..1 {
                dual_module.grow(Rational::new_raw(1.into(), 1.into()));
                visualizer
                    .snapshot_combined("grow 1".to_string(), vec![&interface_ptr, &dual_module])
                    .unwrap();
            }
            visualizer
                .snapshot_combined(
                    "subgraph".to_string(),
                    vec![&interface_ptr, &dual_module, &Subgraph::from(vec![4])],
                )
                .unwrap();
        }
    }
}

fn main() {
    debug_demo();
    simple_demo();
}
