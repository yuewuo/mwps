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
