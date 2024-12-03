from common import *


def test_flatten_config():
    code = mwpf.CodeCapacityColorCode(d=5, p=0.01)
    config = {
        "timeout": 6.0,
        "cluster_node_limit": 3,
    }
    mwpf.Solver(code.get_initializer(), config)


def test_legacy_config():
    code = mwpf.CodeCapacityColorCode(d=5, p=0.01)
    config = {
        "primal": {
            "timeout": 6.0,
            "cluster_node_limit": 3,
        }
    }
    mwpf.Solver(code.get_initializer(), config)
