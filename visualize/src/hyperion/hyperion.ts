import { Vector3, type Intersection, Quaternion } from 'three'
import { assert, parse_rust_bigint } from '@/util'

export const zero_vector = new Vector3(0, 0, 0)
export const unit_up_vector = new Vector3(0, 1, 0)
export const renderer_params = {
    antialias: true,
    alpha: true,
    powerPreference: 'high-performance',
    precision: 'highp',
    stencil: true,
}

export interface Position {
    t: number
    i: number
    j: number
}

export function load_position (mesh_position: Vector3, data_position: Position) {
    mesh_position.z = data_position.i
    mesh_position.x = data_position.j
    mesh_position.y = data_position.t
}

export function compute_vector3 (data_position: Position): Vector3 {
    const vector = new Vector3(0, 0, 0)
    load_position(vector, data_position)
    return vector
}

export interface DualNode {
    // V_S: vertex list
    v: number[]
    // E_S: edge list
    e: number[]
    // delta(S): hair edge list
    h: number[]
    // dual variable (d = dn/dd)
    d: number
    dn: bigint | number
    dd: bigint | number
    // grow rate (r = rn/rd)
    r: number
    rn: bigint | number
    rd: bigint | number
}

export interface Edge {
    // weight
    w: number
    // vertices
    v: number[]
    // grown (g = gn/gd)
    g?: number
    gn: bigint | number
    gd: bigint | number
    // un-grown (u = un/ud = w_e - g)
    u: number
    un: bigint | number
    ud: bigint | number
}

export interface Interface {
    // sum of dual variables (sum_dual = sdn/sdd)
    sum_dual: number
    sdn: bigint | number
    sdd: bigint | number
}

export interface Vertex {
    // is defect
    s: boolean
}

export type Subgraph = number[]

export interface WeightRange {
    // lower (l = ln/ld)
    lower: number
    ld: bigint | number
    ln: bigint | number
    // upper (u = un/ud)
    upper: number
    ud: bigint | number
    un: bigint | number
}

export interface Snapshot {
    dual_nodes?: DualNode[]
    edges: Edge[]
    interface: Interface
    vertices: Vertex[]
    subgraph?: Subgraph
    weight_range: WeightRange
}

export type SnapshotTuple = (string | Snapshot)[]

export interface VisualizerData {
    format: string
    version: string
    positions: Position[]
    // [name, snapshot]
    snapshots: SnapshotTuple[]
}

export function fix_visualizer_data (visualizer: VisualizerData) {
    for (const entry of visualizer.snapshots) {
        assert(entry.length == 2)
        const snapshot = entry[1] as Snapshot
        if (snapshot.dual_nodes != undefined) {
            for (const dual_node of snapshot.dual_nodes) {
                if (dual_node.dn != undefined) dual_node.dn = parse_rust_bigint(dual_node.dn)
                if (dual_node.dd != undefined) dual_node.dd = parse_rust_bigint(dual_node.dd)
                if (dual_node.rn != undefined) dual_node.rn = parse_rust_bigint(dual_node.rn)
                if (dual_node.rd != undefined) dual_node.rd = parse_rust_bigint(dual_node.rd)
            }
        }
        if (snapshot.edges != undefined) {
            for (const edge of snapshot.edges) {
                if (edge.gn != undefined) edge.gn = parse_rust_bigint(edge.gn)
                if (edge.gd != undefined) edge.gd = parse_rust_bigint(edge.gd)
                if (edge.un != undefined) edge.un = parse_rust_bigint(edge.un)
                if (edge.ud != undefined) edge.ud = parse_rust_bigint(edge.ud)
            }
        }
        if (snapshot.interface != undefined) {
            if (snapshot.interface.sdn != undefined) snapshot.interface.sdn = parse_rust_bigint(snapshot.interface.sdn)
            if (snapshot.interface.sdd != undefined) snapshot.interface.sdd = parse_rust_bigint(snapshot.interface.sdd)
        }
        if (snapshot.weight_range != undefined) {
            if (snapshot.weight_range.ld != undefined) snapshot.weight_range.ld = parse_rust_bigint(snapshot.weight_range.ld)
            if (snapshot.weight_range.ln != undefined) snapshot.weight_range.ln = parse_rust_bigint(snapshot.weight_range.ln)
            if (snapshot.weight_range.ud != undefined) snapshot.weight_range.ud = parse_rust_bigint(snapshot.weight_range.ud)
            if (snapshot.weight_range.un != undefined) snapshot.weight_range.un = parse_rust_bigint(snapshot.weight_range.un)
        }
    }
}

/* runtime data */
export class RuntimeData {
    visualizer: VisualizerData
    hovered: Intersection | undefined = undefined
    selected: Intersection | undefined = undefined

    constructor (visualizer: VisualizerData) {
        // first fix the visualizer data (primarily the BigInts)
        fix_visualizer_data(visualizer)
        this.visualizer = visualizer
    }
}

export class ConfigProps {
    show_config: boolean = true
    show_info: boolean = true
    full_screen: boolean = false
    segments: number = 32
    visualizer_config: any = undefined
    initial_aspect_ratio?: number = undefined
    snapshot_index?: number = undefined
}

/*
The following are visualization specific states
*/

export class VertexState {
    type: string = 'vertex'
    vi: number
    color?: string = undefined
    position: Position

    constructor (vi: number, position: Position) {
        this.vi = vi
        this.position = position
    }
}

export class VertexStates {
    normal_vertices: Array<VertexState> = []
    defect_vertices: Array<VertexState> = []
    all_outlines: Array<VertexState> = []
}

export class EdgeRingState {
    type: string = 'edge'
    ei: number
    position: Vector3
    color: string = 'black'
    inner: number = 0
    outer: number = 1

    constructor (ei: number, position: Vector3) {
        this.ei = ei
        this.position = position
    }
}

export class EdgeTubeState {
    type: string = 'edge'
    ei: number
    position: Vector3
    color: string = 'black'
    length: number
    degree: number
    quaternion: Quaternion

    constructor (ei: number, position: Vector3, length: number, degree: number, quaternion: Quaternion) {
        this.ei = ei
        this.position = position
        this.length = length
        this.degree = degree
        this.quaternion = quaternion
    }
}

export const ring_resolution = 100
export class EdgeStates {
    // rings (degree-1 edges)
    ungrown_edge_rings: Array<Array<EdgeRingState>> = Array.from({ length: ring_resolution + 1 }, () => [])
    grown_edge_rings: Array<Array<EdgeRingState>> = Array.from({ length: ring_resolution + 1 }, () => [])
    tight_edge_rings: Array<Array<EdgeRingState>> = Array.from({ length: ring_resolution + 1 }, () => [])
    // tubes (higher degree edges)
    ungrown_edge_tubes: Array<EdgeTubeState> = []
    grown_edge_tubes: Array<EdgeTubeState> = []
    tight_edge_tubes: Array<EdgeTubeState> = []
}

export function compute_edge_to_dual_indices (snapshot: Snapshot): Array<Array<number>> {
    const dual_indices: Array<Array<number>> = Array.from({ length: snapshot.edges.length }, () => [])
    if (snapshot.dual_nodes != null) {
        for (const [node_index, node] of snapshot.dual_nodes.entries()) {
            for (const edge_index of node.h) {
                dual_indices[edge_index].push(node_index)
            }
        }
    }
    return dual_indices
}

export function compute_vertex_to_dual_indices (snapshot: Snapshot): Array<Array<number>> {
    const dual_indices: Array<Array<number>> = Array.from({ length: snapshot.vertices.length }, () => [])
    if (snapshot.dual_nodes != null) {
        for (const [node_index, node] of snapshot.dual_nodes.entries()) {
            for (const vertex_index of node.v) {
                dual_indices[vertex_index].push(node_index)
            }
        }
    }
    return dual_indices
}

export function compute_vertex_incident_edges (snapshot: Snapshot): Array<Array<number>> {
    const incident_edges: Array<Array<number>> = Array.from({ length: snapshot.vertices.length }, () => [])
    for (const [edge_index, edge] of snapshot.edges.entries()) {
        for (const vertex_index of edge.v) {
            incident_edges[vertex_index].push(edge_index)
        }
    }
    return incident_edges
}

/**
 * Idea from Katie 2024.10.2: draw each edge branch differently to show which vertices the dual variables contribute
 *
 * The previous visualization is to display all each hypergraph in deg_v branches, and each branch is identical.
 *     Then for each branch, we print the contribution of all the dual variables. This method is very simple, and essentially
 *     convert the hyperedge printing problem to deg_v number of simple edge printing. However, this method will not
 *     convey the information of which vertices are the dual variables "flooding" from. For example, a single defect vertex
 *     grows over its adjacent hyperedges, however this method does not allow readers to get the information of which vertex
 *     is growing by looking at the edge along.
 *
 * We then found a new method to display it better: Since we know the subset of vertices that a dual variable contributes,
 *     namely $e \cap V_S$, we can grow from these vertices and then show the direction of the dual variable. This method
 *     is stable, in a sense that a small change of dual variables corresponds to a small change of the visualization effect,
 *     given that the dual variables have a consistent order (by their indices).
 *
 * This function outputs an object describing the segments on each edge branch.
 */

export interface EdgeBranchSegments {
    grown_end: Array<number>
    grown_center: Array<number>
    contributor_end: Array<Array<[number, number]>>
    contributor_center: Array<Array<[number, number]>>
}

export function calculate_edge_branch_segmented (snapshot: Snapshot, edge_to_dual_indices: Array<Array<number>>, edge_index: number): EdgeBranchSegments {
    // calculate the list of contributing dual variables
    let dual_indices = []
    let edge = snapshot.edges[edge_index]
    if (snapshot.dual_nodes != null) {
        // check the non-zero contributing dual variables
        for (let node_index of edge_to_dual_indices[edge_index]) {
            if (snapshot.dual_nodes[node_index].d != 0) {
                dual_indices.push(node_index)
            }
        }
    }
    // the grown value for each edge branch
    let grown_end = Array(edge.v.length).fill(0)
    let grown_center = Array(edge.v.length).fill(0)
    // the contributing dual variables from the end vertex and the center vertex, respectively
    let contributor_end: Array<Array<[number, number]>> = Array.from({ length: edge.v.length }, () => [])
    let contributor_center: Array<Array<[number, number]>> = Array.from({ length: edge.v.length }, () => [])
    if (snapshot.dual_nodes == null || dual_indices.length == 0) {
        return { grown_end, grown_center, contributor_end, contributor_center }
    }
    // iterate over all dual variables and put them on the edge branches
    let branch_weight = Math.abs(edge.w) / edge.v.length
    for (let ni of dual_indices) {
        const node = snapshot.dual_nodes[ni]
        // calculate the contributing vertices of this dual variable: $e \cap V_S$
        let vertices = []
        let v_s = new Set(snapshot.dual_nodes[ni].v)
        for (let [v_eid, v] of edge.v.entries()) {
            if (v_s.has(v)) {
                vertices.push(v_eid)
            }
        }
        if (vertices.length == 0) {
            // this doesn't make sense, but we should not crash the program
            for (let [v_eid, _v] of edge.v.entries()) {
                vertices.push(v_eid)
            }
        }
        let center_grow = 0 // the amount of growth that must happen at the center because some edge branch is already tight
        let branch_growth = node.d / vertices.length
        // first, grow from end vertices, each with `branch_growth`
        for (let v_eid of vertices) {
            let remain = branch_weight - grown_end[v_eid] - grown_center[v_eid]
            if (branch_growth <= remain) {
                grown_end[v_eid] += branch_growth
                contributor_end[v_eid].push([ni, branch_growth])
            } else {
                grown_end[v_eid] += remain
                contributor_end[v_eid].push([ni, remain])
                center_grow += branch_growth - remain
            }
        }
        // then, grow from center vertices
        while (center_grow > 0) {
            let available_vertices = []
            let min_positive_remain = null
            for (let [v_eid] of edge.v.entries()) {
                let remain = branch_weight - grown_end[v_eid] - grown_center[v_eid]
                if (remain > 0) {
                    available_vertices.push(v_eid)
                    if (min_positive_remain == null) {
                        min_positive_remain = remain
                    } else if (remain < min_positive_remain) {
                        min_positive_remain = remain
                    }
                }
            }
            if (min_positive_remain == null) {
                if (center_grow > 1e-6) {
                    // tolerance of error
                    console.error(`need to grow from center of ${center_grow} but all branches are fully grown`)
                }
                break
            }
            // in this round, we can only grow `min_positive_remain` on the branches of `available_vertices`
            if (min_positive_remain > center_grow / available_vertices.length) {
                min_positive_remain = center_grow / available_vertices.length
            }
            center_grow -= min_positive_remain * available_vertices.length
            for (let v_eid of available_vertices) {
                grown_center[v_eid] += min_positive_remain
                const center = contributor_center[v_eid]
                // grow from center, potentially merging with existing segments
                if (center.length > 0 && center[center.length - 1][0] == ni) {
                    center[center.length - 1][1] += min_positive_remain
                } else {
                    center.push([ni, min_positive_remain])
                }
            }
        }
    }
    return { grown_end, grown_center, contributor_end, contributor_center }
}
