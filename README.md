# MWPF
Hypergraph <span style="color: red;">M</span>inimum-<span style="color: red; font-size: 120%;">W</span>eight <span style="color: red; font-size: 120%;">P</span>arity <span style="color: red; font-size: 120%;">F</span>actor Decoder for QEC

*Preview version claim: We publish the binary Python package but do not guarantee any correctness or speed. The source code and the full version will be made publicly available when our paper comes out.*

Note: hypergraph MWPF is proven to be NP-hard. Our design is taking advantage of clustering technique to lower
the **average** time complexity and reach almost-linear **average** time complexity at small physical error rate.
Please wait for our paper for more discussion of the speed v.s. accuracy.

## Installation

```sh
pip install mwpf
```

## Background

Solving MWPF on hypergraph is essential for QEC decoding because it can implement exact Most Likely Error (MLE) decoder 
on topological codes assuming independent physical qubit errors. Existing work like MWPM decoder can only model independent 
errors that generate 1 or 2 defect vertices. We model such a decoding problem as solving MWPF on the decoding graph in 
[this tutorial](https://tutorial.fusionblossom.com/problem-definition.html). Extending the MWPF algorithm to hypergraph, 
however, requires substantial modification over [the existing MWPF algorithm on normal graph](https://github.com/yuewuo/fusion-blossom). 
Hypergraph MWPF algorithm can model any independent error that generates arbitrary number of defect vertices, 
enabling applications in not only decoding depolarizing noise channel but also other decoding codes like color 
code and tailored surface code.

Here is an example to use the library. Consider the simplest case 

```python
"""
o: virtual vertex (can be matched arbitrary times)
*: real vertex (must be matched specific times according to the syndrome)

   0     1     2     3     4      edge (weights=100)
o --- * --- * --- * --- * --- o
0     1     2     3     4     5   vertex

      |     |     |
      -------------  hyperedge 5 (weight=60) (only considered by MWPF but not MWPM)
"""
```

When using traditional MWPM decoder, e.g. [fusion blossom](https://github.com/yuewuo/fusion-blossom), we would construct a solver like this

```python
import fusion_blossom as fb

def prepare_fusion_solver() -> fb.SolverSerial:
    vertex_num = 6
    weighted_edges = [(0, 1, 100), (1, 2, 100), (2, 3, 100), (3, 4, 100), (4, 5, 100)]
    virtual_vertices = [0, 5]
    initializer = fb.SolverInitializer(vertex_num, weighted_edges, virtual_vertices)
    solver = fb.SolverSerial(initializer)
    return solver
```

Note that we omit hyperedge 5 because MWPM decoder is not capable of handling hyperedges.
For a syndrome of vertices `[1, 2, 4]`, a minimum weight perfect matching would be edges `[1, 4]` with weight 200.

```python
syndrome = [1, 2, 4]
fusion = prepare_fusion_solver()
fusion.solve(fb.SyndromePattern(syndrome))
fusion_subgraph = fusion.subgraph()
print(fusion_subgraph)  # out: [1, 4]
```

Now, when we use a MWPF decoder (our implementation is called "Hyperion"), it's capable of considering all hyperedges.
Note that in the MWPF decoder there is no need to define virtual vertices.
A virtual vertex can be modeled by adding a zero-weighted hyperedge of `Hyperedge([v], 0)` to the vertex `v`.

```python
import mwpf

def prepare_hyperion_solver() -> mwpf.SolverSerialJointSingleHair:
    vertex_num = 6
    weighted_edges = [
        mwpf.HyperEdge([0, 1], 100),
        mwpf.HyperEdge([1, 2], 100),
        mwpf.HyperEdge([2, 3], 100),
        mwpf.HyperEdge([3, 4], 100),
        mwpf.HyperEdge([4, 5], 100),
        mwpf.HyperEdge([1, 2, 3], 60),  # hyperedge
        mwpf.HyperEdge([0], 0),  # virtual vertex
        mwpf.HyperEdge([5], 0),  # virtual vertex
    ]
    initializer = mwpf.SolverInitializer(vertex_num, weighted_edges)
    solver = mwpf.SolverSerialJointSingleHair(initializer)
    return solver
```

When solving the same syndrome, it's capable of using the lower-weighted hyperedge to find a more-likely error pattern.
And most interestingly, although we do not guarantee most-likely error in all cases, we do have the bound for the result.
When the lower bound is equal to the upper bound, we know the result is optimal, i.e. most-likely error pattern.
When they're not equal, we know the worst-case proximity of the result which is useful.


```python
syndrome = [1, 2, 4]
hyperion = prepare_hyperion_solver()
hyperion.solve(mwpf.SyndromePattern(syndrome))
hyperion_subgraph = hyperion.subgraph()
print(hyperion_subgraph)  # out: [3, 5]
_, bound = hyperion.subgraph_range()
print((bound.lower, bound.upper))  # out: (Fraction(160, 1), Fraction(160, 1))
```

