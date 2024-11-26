import mwpf, pytest


def test_echelon_matrix_simple():
    # pytest -s tests/python/test_matrix.py::test_echelon_matrix_simple
    matrix = mwpf.EchelonMatrix()
    matrix.add_constraint(vertex_index=0, incident_edges=[1, 4, 6], parity=True)
    matrix.add_constraint(1, [4, 9], parity=False)
    matrix.add_constraint(2, [1, 9], parity=True)
    assert matrix.edge_to_var_index(4) == 1
    for edge_index in [1, 4, 6, 9]:
        matrix.update_edge_tightness(edge_index, True)
    print()
    print(matrix)
    assert (
        str(matrix)
        == """\
┌──┬─┬─┬─┬─┬───┬─┐
┊ E┊1┊4┊6┊9┊ = ┊▼┊
╞══╪═╪═╪═╪═╪═══╪═╡
┊ 0┊1┊ ┊ ┊1┊ 1 ┊1┊
├──┼─┼─┼─┼─┼───┼─┤
┊ 1┊ ┊1┊ ┊1┊   ┊4┊
├──┼─┼─┼─┼─┼───┼─┤
┊ 2┊ ┊ ┊1┊ ┊   ┊6┊
├──┼─┼─┼─┼─┼───┼─┤
┊ ▶┊0┊1┊2┊*┊◀  ┊▲┊
└──┴─┴─┴─┴─┴───┴─┘
"""
    )
    # set edges 1 and 6 as the tail
    matrix.set_tail_edges({6, 1})
    assert matrix.get_tail_edges() == {1, 6}
    print(matrix)
    assert (
        str(matrix)
        == """\
┌──┬─┬─┬─┬─┬───┬─┐
┊ E┊4┊9┊1┊6┊ = ┊▼┊
╞══╪═╪═╪═╪═╪═══╪═╡
┊ 0┊1┊ ┊1┊ ┊ 1 ┊4┊
├──┼─┼─┼─┼─┼───┼─┤
┊ 1┊ ┊1┊1┊ ┊ 1 ┊9┊
├──┼─┼─┼─┼─┼───┼─┤
┊ 2┊ ┊ ┊ ┊1┊   ┊6┊
├──┼─┼─┼─┼─┼───┼─┤
┊ ▶┊0┊1┊*┊2┊◀  ┊▲┊
└──┴─┴─┴─┴─┴───┴─┘
"""
    )
    # set edge 4 as the tail
    matrix.set_tail_edges({4})
    print(matrix)
    assert (
        str(matrix)
        == """\
┌──┬─┬─┬─┬─┬───┬─┐
┊ E┊1┊6┊9┊4┊ = ┊▼┊
╞══╪═╪═╪═╪═╪═══╪═╡
┊ 0┊1┊ ┊ ┊1┊ 1 ┊1┊
├──┼─┼─┼─┼─┼───┼─┤
┊ 1┊ ┊1┊ ┊ ┊   ┊6┊
├──┼─┼─┼─┼─┼───┼─┤
┊ 2┊ ┊ ┊1┊1┊   ┊9┊
├──┼─┼─┼─┼─┼───┼─┤
┊ ▶┊0┊1┊2┊*┊◀  ┊▲┊
└──┴─┴─┴─┴─┴───┴─┘
"""
    )
    # set one edge to loose
    matrix.update_edge_tightness(edge_index=6, is_tight=False)
    print(matrix)
    assert (
        str(matrix)
        == """\
┌──┬─┬─┬─┬───┬─┐
┊ E┊1┊9┊4┊ = ┊▼┊
╞══╪═╪═╪═╪═══╪═╡
┊ 0┊1┊ ┊1┊ 1 ┊1┊
├──┼─┼─┼─┼───┼─┤
┊ 1┊ ┊1┊1┊   ┊9┊
├──┼─┼─┼─┼───┼─┤
┊ ▶┊0┊1┊*┊◀  ┊▲┊
└──┴─┴─┴─┴───┴─┘
"""
    )
    matrix.update_edge_tightness(1, False)
    matrix.update_edge_tightness(9, False)
    print(matrix)
    assert (
        str(matrix)
        == """\
┌──┬─┬───┬─┐
┊ X┊4┊ = ┊▼┊
╞══╪═╪═══╪═╡
┊ 0┊1┊ 1 ┊4┊
├──┼─┼───┼─┤
┊ 1┊ ┊ 1 ┊*┊
├──┼─┼───┼─┤
┊ ▶┊0┊◀  ┊▲┊
└──┴─┴───┴─┘
"""
    )


def test_tail_matrix_1():
    # pytest -s tests/python/test_matrix.py::test_tail_matrix_1
    matrix = mwpf.TailMatrix()
    matrix.add_constraint(vertex_index=0, incident_edges=[1, 4, 6], parity=True)
    matrix.add_constraint(1, [4, 9], parity=False)
    matrix.add_constraint(2, [1, 9], parity=True)
    assert matrix.edge_to_var_index(4) == 1
    print()
    print(matrix)
    assert (
        str(matrix)
        == """\
┌─┬───┐
┊ ┊ = ┊
╞═╪═══╡
┊0┊ 1 ┊
├─┼───┤
┊1┊   ┊
├─┼───┤
┊2┊ 1 ┊
└─┴───┘
"""
    )
    for edge_index in [1, 4, 6, 9]:
        matrix.update_edge_tightness(edge_index, True)
    print(matrix)
    assert (
        str(matrix)
        == """\
┌─┬─┬─┬─┬─┬───┐
┊ ┊1┊4┊6┊9┊ = ┊
╞═╪═╪═╪═╪═╪═══╡
┊0┊1┊1┊1┊ ┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊1┊ ┊1┊ ┊1┊   ┊
├─┼─┼─┼─┼─┼───┤
┊2┊1┊ ┊ ┊1┊ 1 ┊
└─┴─┴─┴─┴─┴───┘
"""
    )
    # set edges 1 and 6 as the tail
    matrix.set_tail_edges({6, 1})
    assert matrix.get_tail_edges() == {1, 6}
    print(matrix)
    assert (
        str(matrix)
        == """\
┌─┬─┬─┬─┬─┬───┐
┊ ┊4┊9┊1┊6┊ = ┊
╞═╪═╪═╪═╪═╪═══╡
┊0┊1┊ ┊1┊1┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊1┊1┊1┊ ┊ ┊   ┊
├─┼─┼─┼─┼─┼───┤
┊2┊ ┊1┊1┊ ┊ 1 ┊
└─┴─┴─┴─┴─┴───┘
"""
    )


def test_tight_matrix_1():
    # pytest -s tests/python/test_matrix.py::test_tight_matrix_1
    matrix = mwpf.TightMatrix()
    matrix.add_constraint(vertex_index=0, incident_edges=[1, 4, 6], parity=True)
    matrix.add_constraint(1, [4, 9], parity=False)
    matrix.add_constraint(2, [1, 9], parity=True)
    assert matrix.edge_to_var_index(4) == 1
    print()
    print(matrix)
    assert (
        str(matrix)
        == """\
┌─┬───┐
┊ ┊ = ┊
╞═╪═══╡
┊0┊ 1 ┊
├─┼───┤
┊1┊   ┊
├─┼───┤
┊2┊ 1 ┊
└─┴───┘
"""
    )
    matrix.update_edge_tightness(4, True)
    matrix.update_edge_tightness(9, True)
    print(matrix)
    assert (
        str(matrix)
        == """\
┌─┬─┬─┬───┐
┊ ┊4┊9┊ = ┊
╞═╪═╪═╪═══╡
┊0┊1┊ ┊ 1 ┊
├─┼─┼─┼───┤
┊1┊1┊1┊   ┊
├─┼─┼─┼───┤
┊2┊ ┊1┊ 1 ┊
└─┴─┴─┴───┘
"""
    )
    matrix.update_edge_tightness(9, False)
    print(matrix)
    assert (
        str(matrix)
        == """\
┌─┬─┬───┐
┊ ┊4┊ = ┊
╞═╪═╪═══╡
┊0┊1┊ 1 ┊
├─┼─┼───┤
┊1┊1┊   ┊
├─┼─┼───┤
┊2┊ ┊ 1 ┊
└─┴─┴───┘
"""
    )


def test_basic_matrix_1():
    # pytest -s tests/python/test_matrix.py::test_basic_matrix_1
    matrix = mwpf.BasicMatrix()
    print()
    print(matrix)
    assert (
        str(matrix)
        == """\
┌┬───┐
┊┊ = ┊
╞╪═══╡
└┴───┘
"""
    )
    matrix.add_variable(edge_index=1)
    matrix.add_variable(4)
    matrix.add_variable(12)
    matrix.add_variable(345)
    print(matrix)
    assert (
        str(matrix)
        == """\
┌┬─┬─┬─┬─┬───┐
┊┊1┊4┊1┊3┊ = ┊
┊┊ ┊ ┊2┊4┊   ┊
┊┊ ┊ ┊ ┊5┊   ┊
╞╪═╪═╪═╪═╪═══╡
└┴─┴─┴─┴─┴───┘
"""
    )
    matrix.add_constraint(0, [1, 4, 12], True)
    matrix.add_constraint(1, [4, 345], False)
    matrix.add_constraint(2, [1, 345], True)
    print(matrix)
    assert (
        str(matrix)
        == """\
┌─┬─┬─┬─┬─┬───┐
┊ ┊1┊4┊1┊3┊ = ┊
┊ ┊ ┊ ┊2┊4┊   ┊
┊ ┊ ┊ ┊ ┊5┊   ┊
╞═╪═╪═╪═╪═╪═══╡
┊0┊1┊1┊1┊ ┊ 1 ┊
├─┼─┼─┼─┼─┼───┤
┊1┊ ┊1┊ ┊1┊   ┊
├─┼─┼─┼─┼─┼───┤
┊2┊1┊ ┊ ┊1┊ 1 ┊
└─┴─┴─┴─┴─┴───┘
"""
    )
    assert matrix.get_vertices() == {0, 1, 2}
    assert matrix.get_view_edges() == [1, 4, 12, 345]
