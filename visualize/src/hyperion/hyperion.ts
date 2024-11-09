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
    g: number
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
                dual_node.dn = parse_rust_bigint(dual_node.dn)
                dual_node.dd = parse_rust_bigint(dual_node.dd)
                dual_node.rn = parse_rust_bigint(dual_node.rn)
                dual_node.rd = parse_rust_bigint(dual_node.rd)
            }
        }
        if (snapshot.edges != undefined) {
            for (const edge of snapshot.edges) {
                edge.gn = parse_rust_bigint(edge.gn)
                edge.gd = parse_rust_bigint(edge.gd)
                edge.un = parse_rust_bigint(edge.un)
                edge.ud = parse_rust_bigint(edge.ud)
            }
        }
        if (snapshot.interface != undefined) {
            snapshot.interface.sdn = parse_rust_bigint(snapshot.interface.sdn)
            snapshot.interface.sdd = parse_rust_bigint(snapshot.interface.sdd)
        }
        if (snapshot.weight_range != undefined) {
            snapshot.weight_range.ld = parse_rust_bigint(snapshot.weight_range.ld)
            snapshot.weight_range.ln = parse_rust_bigint(snapshot.weight_range.ln)
            snapshot.weight_range.ud = parse_rust_bigint(snapshot.weight_range.ud)
            snapshot.weight_range.un = parse_rust_bigint(snapshot.weight_range.un)
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
        console.log(visualizer)
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
