from common import *
import pickle


def test_basic_panic():
    vertex_num = 2
    weighted_edges = [
        mwpf.HyperEdge([0, 1], 100),
    ]
    initializer = mwpf.SolverInitializer(vertex_num, weighted_edges)
    solver = mwpf.SolverSerialJointSingleHair(initializer)

    visualizer = mwpf.Visualizer(
        positions=[mwpf.VisualizePosition(0, 0, 0), mwpf.VisualizePosition(1, 0, 0)]
    )
    solver.solve(mwpf.SyndromePattern([0]), visualizer)  # unsolvable, and should panic

    visualizer.save_html(
        os.path.join(os.path.dirname(__file__), f"test_basic_panic.html")
    )
    try:
        solver.subgraph()
    except BaseException as panic:
        print(panic)
    else:
        assert False, "panic expected"
