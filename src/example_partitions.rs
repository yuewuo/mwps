//! Example Partition
//! 


use super::example_codes::*;
use super::util::*;
use clap::Parser;
use serde::Serialize;
use std::collections::VecDeque;

pub trait ExamplePartition {
    /// customize partition, note that this process may re-order the vertices in `code`
    fn build_apply(&mut self, code: &mut dyn ExampleCode) -> PartitionConfig {
        // first apply reorder
        if let Some(reordered_vertices) = self.build_reordered_vertices(code) {
            code.reorder_vertices(&reordered_vertices);
        }
        self.build_partition(code)
    }

    fn re_index_defect_vertices(&mut self, code: &dyn ExampleCode, defect_vertices: &[VertexIndex]) -> Vec<VertexIndex> {
        if let Some(reordered_vertices) = self.build_reordered_vertices(code) {
            translated_defect_to_reordered(&reordered_vertices, defect_vertices)
        } else {
            defect_vertices.into()
        }
    }

    /// build reorder vertices
    fn build_reordered_vertices(&mut self, _code: &dyn ExampleCode) -> Option<Vec<VertexIndex>> {
        None
    }

    /// build the partition, using the indices after reordered vertices
    fn build_partition(&mut self, code: &dyn ExampleCode) -> PartitionConfig;
}

impl PhenomenologicalPlanarCodeTimePartition {
    pub fn new_tree(
        d: VertexNum,
        noisy_measurements: VertexNum,
        partition_num: usize,
        enable_tree_fusion: bool,
        maximum_tree_leaf_size: usize,
    ) -> Self {
        Self {
            d,
            noisy_measurements,
            partition_num,
            enable_tree_fusion,
            maximum_tree_leaf_size,
        }
    }
    pub fn new(d: VertexNum, noisy_measurements: VertexNum, partition_num: usize) -> Self {
        Self::new_tree(d, noisy_measurements, partition_num, false, usize::MAX)
    }
}

impl ExamplePartition for PhenomenologicalPlanarCodeTimePartition {
    #[allow(clippy::unnecessary_cast)]
    fn build_partition(&mut self, code: &dyn ExampleCode) -> PartitionConfig {
        let (d, noisy_measurements, partition_num) = (self.d, self.noisy_measurements, self.partition_num);
        let round_vertex_num = d * (d + 1);
        let vertex_num = round_vertex_num * (noisy_measurements + 1);
        assert_eq!(code.vertex_num(), vertex_num, "code size incompatible");
        assert!(partition_num >= 1 && partition_num <= noisy_measurements as usize + 1);
        // do not use fixed partition_length, because it would introduce super long partition; do it on the fly
        let mut config = PartitionConfig::new(vertex_num);
        config.partitions.clear();
        for partition_index in 0..partition_num as VertexIndex {
            let start_round_index = partition_index * (noisy_measurements + 1) / partition_num as VertexNum;
            let end_round_index = (partition_index + 1) * (noisy_measurements + 1) / partition_num as VertexNum;
            assert!(end_round_index > start_round_index, "empty partition occurs");
            if partition_index == 0 {
                config.partitions.push(VertexRange::new(
                    start_round_index * round_vertex_num,
                    end_round_index * round_vertex_num,
                ));
            } else {
                config.partitions.push(VertexRange::new(
                    (start_round_index + 1) * round_vertex_num,
                    end_round_index * round_vertex_num,
                ));
            }
        }
        config.fusions.clear();
        if !self.enable_tree_fusion || self.maximum_tree_leaf_size == 1 {
            for unit_index in 0..partition_num {
                config.fusions.push((unit_index as usize, unit_index as usize + 1));
            }
        } 
        config
    }
}

#[cfg(test)]
pub mod tests {
    use super::super::visualize::*;
    use super::*;

    pub fn visualize_partition(
        code: &mut dyn ExampleCode,
        visualize_filename: Option<String>,
        mut defect_vertices: Vec<VertexIndex>,
        mut partition: impl ExamplePartition,
    ) {
        println!("defect_vertices: {}", defect_vertices);
        let partition_config = partition.build_apply(code);
        let mut visualizer = match visualize_filename.as_ref() {
            Some(visualize_filename) => {
                let visualizer = Visualizer::new(
                    Some(visualize_data_folder() + visualize_filename.as_str()),
                    code.get_positions(),
                    true,
                )
                .unwrap();
                print_visualize_link(visualize_filename.clone());
                Some(visualizer)
            }
            None => None,
        };
        let partition_info = partition_config.info();
        code.set_defect_vertices(&defect_vertices);
        
    }
}