from common import *


def test_corner_1():
    """
    adding isolated vertices will not harm the performance of the algorithm so we can ignore this case
    """
    vertex_num = 4
    weighted_edges = [
        mwpf.HyperEdge([0], 0.6),
        mwpf.HyperEdge([0, 1], 0.7),
        mwpf.HyperEdge([1, 2], 0.8),
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
    visualizer.save_html(os.path.join(os.path.dirname(__file__), f"test_corner_1.html"))


def test_corner_2():
    """
    test empty edge set
    """
    vertex_num = 4
    weighted_edges = []
    initializer = mwpf.SolverInitializer(vertex_num, weighted_edges)
    solver = mwpf.SolverSerialJointSingleHair(initializer)
    visualizer = mwpf.Visualizer(positions=circle_positions(vertex_num))
    syndrome = mwpf.SyndromePattern([])
    solver.solve(syndrome, visualizer)
    subgraph, bound = solver.subgraph_range(visualizer)
    print(subgraph, bound)
    assert bound.lower == bound.upper
    assert bound.lower.float() == 0.0
    visualizer.save_html(os.path.join(os.path.dirname(__file__), f"test_corner_2.html"))


def test_corner_3():
    """
    test empty graph
    """
    vertex_num = 0
    weighted_edges = []
    initializer = mwpf.SolverInitializer(vertex_num, weighted_edges)
    solver = mwpf.SolverSerialJointSingleHair(initializer)
    visualizer = mwpf.Visualizer(positions=circle_positions(vertex_num))
    syndrome = mwpf.SyndromePattern([])
    solver.solve(syndrome, visualizer)
    subgraph, bound = solver.subgraph_range(visualizer)
    print(subgraph, bound)
    assert bound.lower == bound.upper
    assert bound.lower.float() == 0.0
    with open(os.path.join(os.path.dirname(__file__), f"test_corner_2.html"), "w") as f:
        f.write(visualizer.generate_html())


def test_corner_4():
    """
    test duplicate edge set
    """
    vertex_num = 4
    weighted_edges = [
        mwpf.HyperEdge([0], 0.6),
        mwpf.HyperEdge([0, 1], 0.7),
        mwpf.HyperEdge([1, 2], 0.8),
        mwpf.HyperEdge([1, 2], 0.8),
        mwpf.HyperEdge([1, 2], 1.2),
        mwpf.HyperEdge([1, 2], 0.8),
        mwpf.HyperEdge([0, 1, 2], 0.3),  # hyper edge
    ]
    initializer = mwpf.SolverInitializer(vertex_num, weighted_edges)
    solver = mwpf.SolverSerialJointSingleHair(initializer)
    visualizer = mwpf.Visualizer(positions=circle_positions(vertex_num))
    syndrome = mwpf.SyndromePattern([])
    solver.solve(syndrome, visualizer)
    subgraph, bound = solver.subgraph_range(visualizer)
    print(subgraph, bound)
    assert bound.lower == bound.upper
    assert bound.lower.float() == 0.0
    with open(os.path.join(os.path.dirname(__file__), f"test_corner_2.html"), "w") as f:
        f.write(visualizer.generate_html())
