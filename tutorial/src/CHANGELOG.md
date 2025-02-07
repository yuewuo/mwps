# Change Log

## 0.1.2

Rename several structures to better reveal their natures.

- change `DualModulePQ` to `DualModulePQGeneric` and defines `DualModulePQ = DualModulePQGeneric<...>`.
- remove `dual_module_serial.rs` since it's no longer used anywhere.
- remove `GrowingStrategy`
- rename `tuning_cluster_size_limit` to `cluster_node_limit`
- rename `PrimalDualType` to `SolverType`
- rename `MaxUpdateLength` to `Obstacle`
- rename `GroupMaxUpdateLength` to `DualReport`

## 0.1.3

Fixed bug of bigint representation in the visualization tool.

## 0.1.3 -> 0.1.4

Optimized performance of multiple visualizations in a single Jupyter notebook, by using a single WebGLRenderer and share
it among all the canvases that are currently in the viewport.

## 0.1.4 -> 0.1.5

fix critical bug when renaming `MaxUpdateLength` to `Obstacle`... which results in much higher logical error rates

## 0.1.5 -> 0.2.0

- changed interface to support floating-point input natively (or rational number when using mwpf_rational).
- integrated visualization tool for Jupyter notebook

## 0.2.0 -> 0.2.1

fixed potential bug of unsafe clear (not clearing `edge.grow_rate` before)

## 0.2.2

exposed `hyperion_exposed` variable in visualization tool so that user can programmatically control the UI

## 0.2.3

- support direct numpy array as input

## 0.2.4

- support hover and select on the index group

## 0.2.5

- support saving html with ctrl-S or cmd-S in GUI
- update readme to include all the parameters
- support sinter decoder inside mwpf

## 0.2.6

- add feature of constantly trying to solve primal problem even though there are still relaxers; this is good for decoding accuracy while still maintain the same level of time
- release GIL during the decoding process

## 0.2.7 - 2025-02-07

- restructure mixed Rust and Python project so that user can edit Python code without recompile
- catch Exception in the decoding loop and record the failing case for debugging purpose
- make the decoder and several related things pickleable, for people to exactly reproduce certain failing cases
- TODO: detect KeyboardInterrupt such that user can interrupt the decoding process in the middle
