import { Vector3, type Intersection } from 'three'

export const zero_vector = new Vector3(0, 0, 0)
export const unit_up_vector = new Vector3(0, 1, 0)
export const renderer_params = {
    antialias: true,
    alpha: true,
    powerPreference: 'high-performance',
    precision: 'highp',
    stencil: true
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
}
