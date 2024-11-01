
# Problem Definition

We'll formally define the problem we're solving in this chapter.

## Decoding HyperGraph

The decoding hypergraph \\( G = (V, E) \\) is naturally defined by the QEC code and the noise model.
Every vertex \\( v \in V \\) is a detector measurement, an XOR of multiple stabilizer measurement results.
Every hyperedge \\( e \in E \\) corresponds to independent physical error(s) that can cause defect measurements on vertices \\( e \subseteq V \\).
Each edge has a non-negative weight \\( w_e \\), calculated by \\( \ln \frac{1-p_e}{p_e} \\) for the aggregated physical error rate \\( p_e \le \frac{1}{2} \\).

For simplicity, here we're assuming a single round of measurement.
A normal stabilizer measurement means trivial measurement result (+1) and a defect stabilizer measurement means non-trivial measurement result (-1).
This can be easily extended to multiple rounds of measurement, where a normal stabilizer measurement means the same result as the previous round, and a defect stabilizer stabilizer measurement means different result from the previous round.

- white sphere: real vertex \\( v \in V, v \notin D \\), a normal stabilizer measurement
- red sphere: defect vertex \\( v \in V, v \in D \\), a defect stabilizer measurement
- circle around a sphere: degree-1 hyperedge that only incident to one vertex that it surrounds
- lines connecting multiple vertices and joint center: hyperedge with 2+ degree

<div style="display: flex; justify-content: center;">
    <div style="width: 49%; text-align: center;">
        <img style="width: 90%;" src="img/decoding-hypergraph.png"/>
        <p>Decoding Hypergraph</p>
    </div>
    <div style="width: 49%; text-align: center;">
        <img style="width: 90%;" src="img/example-hyperedges.png"/>
        <p>Different Types of Edges</p>
    </div>
</div>

# TODO: finish the problem definition and use the demo
