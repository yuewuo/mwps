import fusion_blossom as fb
import mwps


def prepare_hyperion_solver() -> mwps.SolverSerialJointSingleHair:
    vertex_num = 6
    weighted_edges = [
        mwps.HyperEdge([0, 1], 100),
        mwps.HyperEdge([1, 2], 100),
        mwps.HyperEdge([2, 3], 100),
        mwps.HyperEdge([3, 4], 100),
        mwps.HyperEdge([4, 5], 100),
        mwps.HyperEdge([1, 2, 3], 60),  # hyper edge
        mwps.HyperEdge([0], 0),  # virtual vertex
        mwps.HyperEdge([5], 0),  # virtual vertex
    ]
    initializer = mwps.SolverInitializer(vertex_num, weighted_edges)
    solver = mwps.SolverSerialJointSingleHair(initializer)
    return solver


def prepare_fusion_solver() -> fb.SolverSerial:
    vertex_num = 6
    weighted_edges = [(0, 1, 100), (1, 2, 100), (2, 3, 100),
                      (3, 4, 100), (4, 5, 100)]
    virtual_vertices = [0, 5]
    initializer = fb.SolverInitializer(
        vertex_num, weighted_edges, virtual_vertices)
    solver = fb.SolverSerial(initializer)
    return solver


def test_compare_hyperion_fusion() -> None:
    syndrome = [1, 2, 4]
    # hyperion
    hyperion = prepare_hyperion_solver()
    hyperion.solve(mwps.SyndromePattern(syndrome))
    hyperion_subgraph = hyperion.subgraph()
    _, bound = hyperion.subgraph_range()
    print(hyperion_subgraph)
    print((bound.lower, bound.upper))
    # fusion blossom
    fusion = prepare_fusion_solver()
    fusion.solve(fb.SyndromePattern(syndrome))
    fusion_subgraph = fusion.subgraph()
    print(fusion_subgraph)
