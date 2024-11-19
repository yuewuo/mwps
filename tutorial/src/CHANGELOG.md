# Change Log

## 0.1.1 -> 0.1.2

Rename several structures to better reveal their natures.

- change `DualModulePQ` to `DualModulePQGeneric` and defines `DualModulePQ = DualModulePQGeneric<...>`.
- remove `dual_module_serial.rs` since it's no longer used anywhere.
- remove `GrowingStrategy`
- rename `tuning_cluster_size_limit` to `cluster_node_limit`
- rename `PrimalDualType` to `SolverType`
- rename `MaxUpdateLength` to `Obstacle`
- rename `GroupMaxUpdateLength` to `DualReport`

## 0.1.2 -> 0.1.3

Fixed bug of bigint representation in the visualization tool.

## 0.1.3 -> 0.1.4

upgrade visualization engine to support multiple views in a single Jupyter notebook efficiently (by sharing WebGL renderers)

## 0.1.4 -> 0.1.5

fix critical bug when renaming `MaxUpdateLength` to `Obstacle`... which results in much higher logical error rates
