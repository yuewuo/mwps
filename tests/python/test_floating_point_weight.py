from common import *


def test_fp_1():
    vertex_num = 4
    weighted_edges = [
        mwpf.HyperEdge([0], 0.6),
        mwpf.HyperEdge([0, 1], 0.7),
        mwpf.HyperEdge([1, 2], 0.8),
        mwpf.HyperEdge([2, 3], 0.9),
        mwpf.HyperEdge([3], 1.1),
        mwpf.HyperEdge([0, 1, 2], 0.3),  # hyper edge
    ]
    initializer = mwpf.SolverInitializer(vertex_num, weighted_edges)
    solver = mwpf.SolverSerialJointSingleHair(initializer)
    visualizer = mwpf.Visualizer(positions=circle_positions(vertex_num))
    syndrome = mwpf.SyndromePattern([0, 1, 2])
    solver.solve(syndrome, visualizer)
    subgraph, bound = solver.subgraph_range(visualizer)
    print(subgraph, bound)
    assert bound.lower == bound.upper
    assert bound.lower.float() == 0.3
    visualizer.save_html(os.path.join(os.path.dirname(__file__), f"test_fp_1.html"))


def test_fp_2():
    vertex_num = 4
    weighted_edges = [
        mwpf.HyperEdge([0], -0.6),
        mwpf.HyperEdge([0, 1], 0.7),
        mwpf.HyperEdge([1, 2], 0.8),
        mwpf.HyperEdge([2, 3], 0.9),
        mwpf.HyperEdge([3], 1.1),
        mwpf.HyperEdge([0, 1, 2], 0.3),  # hyper edge
    ]
    initializer = mwpf.SolverInitializer(vertex_num, weighted_edges)
    solver = mwpf.SolverSerialJointSingleHair(initializer)
    visualizer = mwpf.Visualizer(positions=circle_positions(vertex_num))
    syndrome = mwpf.SyndromePattern([0, 1, 2])
    solver.solve(syndrome, visualizer)
    subgraph, bound = solver.subgraph_range(visualizer)
    print("subgraph edges:", [edge_index for edge_index in subgraph])
    assert subgraph == [0, 2]
    visualizer.snapshot("original problem", initializer, syndrome, subgraph)
    print(subgraph, bound)
    assert math.isclose(bound.lower.float(), 0.2)
    assert math.isclose(bound.upper.float(), 0.2)
    visualizer.save_html(os.path.join(os.path.dirname(__file__), f"test_fp_2.html"))
