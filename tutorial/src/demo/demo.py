import mwpf

code = mwpf.CodeCapacityPlanarCode(d=3, p=0.01, max_half_weight=500)

## Construct Syndrome

syndrome = mwpf.SyndromePattern(defect_vertices=[1, 5])

## Visualize Result

positions = code.get_positions()
visualizer = mwpf.Visualizer(positions=positions)

initializer = code.get_initializer()
solver = mwpf.SolverSerial(initializer)

solver.solve(syndrome)

subgraph = solver.subgraph(visualizer)
print(f"MWPF: {subgraph}")  # Vec<EdgeIndex>

visualizer
