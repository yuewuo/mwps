import { Vector3, type Intersection, Quaternion } from 'three'

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
    dn: number
    dd: number
    // grow rate (r = rn/rd)
    r: number
    rn: number
    rd: number
}

export interface Edge {
    // weight
    w: number
    // vertices
    v: number[]
    // grown (g = gn/gd)
    g: number
    gn: number
    gd: number
    // un-grown (u = un/ud = w_e - g)
    u: number
    un: number
    ud: number
}

export interface Interface {
    // sum of dual variables (sum_dual = sdn/sdd)
    sum_dual: number
    sdn: number
    sdd: number
}

export interface Vertex {
    // is defect
    s: boolean
}

export type Subgraph = number[]

export interface WeightRange {
    // lower (l = ln/ld)
    lower: number
    ld: number
    ln: number
    // upper (u = un/ud)
    upper: number
    ud: number
    un: number
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

/* runtime data */
export class RuntimeData {
    visualizer: VisualizerData
    hovered: Intersection | undefined = undefined
    selected: Intersection | undefined = undefined

    constructor (visualizer: VisualizerData) {
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
