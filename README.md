# mwps
Hypergraph Minimum-Weight Parity Subgraph (MWPS) Algorithm for Quantum LDPC Codes

**This is a placeholder for the project. We plan to release the code in summer 2023.**

## Background

Solving MWPS on hypergraph is essential for QEC decoding because it can implement exact Most Likely Error (MLE) decoder on topological codes assuming independent physical qubit errors. Existing work like MWPM decoder can only model independent errors that generate 1 or 2 defect vertices. We model such a decoding problem as solving MWPS on the decoding graph in [this tutorial](https://tutorial.fusionblossom.com/problem-definition.html). Extending the MWPS algorithm to hypergraph, however, requires substantial modification over [the existing MWPS algorithm on normal graph](https://github.com/yuewuo/fusion-blossom). Hypergraph MWPS algorithm can model any independent error that generates arbitrary number of defect vertices, enabling applications in not only decoding depolarizing noise channel but also other decoding codes like color code and tailored surface code.

