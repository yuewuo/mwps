import mwpf


vertex_num = 6
weighted_edges = [
    mwpf.HyperEdge([0, 1], 100),
    mwpf.HyperEdge([1, 2], 100),
    mwpf.HyperEdge([2, 3], 100),
    mwpf.HyperEdge([3, 4], 100),
    mwpf.HyperEdge([4, 5], 100),
    mwpf.HyperEdge([1, 2, 3], 60),  # hyper edge
    mwpf.HyperEdge([0], 0),  # virtual vertex
    mwpf.HyperEdge([5], 0),  # virtual vertex
]
initializer = mwpf.SolverInitializer(vertex_num, weighted_edges)
solver = mwpf.SolverSerialJointSingleHair(initializer)


syndrome = [1, 2, 4]
solver.solve(mwpf.SyndromePattern(syndrome))
subgraph, bound = solver.subgraph_range()
print(subgraph)
print((bound.lower, bound.upper))
