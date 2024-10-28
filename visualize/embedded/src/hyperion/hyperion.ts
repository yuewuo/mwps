import { type Ref, ref, computed } from 'vue'
import { Pane, FolderApi } from 'tweakpane'
import * as EssentialsPlugin from '@tweakpane/plugin-essentials'
import { assert } from '@/util'
import { Vector3, type OrthographicCamera, type Intersection } from 'three'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'

export const zero_vector = new Vector3(0, 0, 0)
export const unit_up_vector = new Vector3(0, 1, 0)

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
    let vector = new Vector3(0, 0, 0)
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
    dual_nodes: DualNode[]
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
    hovered: Ref<any> = ref(undefined)
    selected: Ref<any> = ref(undefined)

    constructor (visualizer: VisualizerData) {
        this.visualizer = visualizer
    }

    getPointerObject (event: PointerIntersectEventInterface) {
        if (event.intersect == undefined) {
            return undefined
        }
        const instanceId = event.intersect.instanceId
        const component = event.intersect.object.userData.component
        return { instanceId, ...component.userData }
    }

    onPointerEnter (event: PointerIntersectEventInterface) {
        this.hovered.value = this.getPointerObject(event)
    }

    onPointerLeave (event: PointerIntersectEventInterface) {
        this.hovered.value = undefined
    }
}

export class ConfigProps {
    show_config: boolean = true
    full_screen: boolean = true
    segments: number = 32
}

/* configuration helper class given the runtime data */
export class Config {
    data: RuntimeData
    config_prop: ConfigProps
    basic: BasicConfig
    snapshot_config: SnapshotConfig = new SnapshotConfig()
    camera: CameraConfig = new CameraConfig()
    vertex: VertexConfig = new VertexConfig()
    edge: EdgeConfig = new EdgeConfig()
    pane?: Pane

    constructor (data: RuntimeData, config_prop: ConfigProps) {
        this.data = data
        this.config_prop = config_prop
        this.basic = new BasicConfig(config_prop)
    }

    create_pane (container: any) {
        assert(this.pane == null, 'cannot create pane twice')
        this.pane = new Pane({
            title: this.title,
            container: container,
            expanded: false
        })
        this.pane.registerPlugin(EssentialsPlugin)
        const pane: FolderApi = this.pane
        const snapshot_names = []
        for (const [name, _] of this.data.visualizer.snapshots) {
            snapshot_names.push(name as string)
        }
        this.snapshot_config.add_to(pane.addFolder({ title: 'Snapshot', expanded: true }), snapshot_names)
        this.camera.add_to(pane.addFolder({ title: 'Camera', expanded: true }))
        this.basic.add_to(pane.addFolder({ title: 'Basic', expanded: true }))
        this.vertex.add_to(pane.addFolder({ title: 'Vertex', expanded: true }))
        this.edge.add_to(pane.addFolder({ title: 'Edge', expanded: true }))
    }

    refresh_pane () {
        const pane: FolderApi = this.pane
        pane.refresh()
    }

    public get title (): string {
        return `MWPF (${this.snapshot_index + 1}/${this.snapshot_num})`
    }

    public set aspect_ratio (aspect_ratio: number) {
        this.basic.aspect_ratio = aspect_ratio
        this.refresh_pane()
    }

    public set snapshot_index (index: number) {
        this.snapshot_config.index = index
        this.snapshot_config.name = index
        const pane: FolderApi = this.pane
        pane.title = this.title
        this.refresh_pane()
    }

    public get snapshot_index (): number {
        return this.snapshot_config.index
    }

    public get snapshot_count (): number {
        return this.data.visualizer.snapshots.length
    }

    public get_snapshot (snapshot_index: number): Snapshot {
        return this.data.visualizer.snapshots[snapshot_index][1] as Snapshot
    }

    public get snapshot (): Snapshot {
        const that = this
        return computed<Snapshot>(() => {
            return that.get_snapshot(that.snapshot_index)
        }) as any
    }

    public get snapshot_num (): number {
        return this.data.visualizer.snapshots.length
    }
}

/* controls basic elements like background and aspect ratio */
export class BasicConfig {
    aspect_ratio: number = 1
    background: string = '#ffffff'
    light_intensity: number = 10
    segments: number
    show_stats: boolean = true
    config_props: ConfigProps

    constructor (config_props: ConfigProps) {
        this.config_props = config_props
        this.segments = config_props.segments
    }

    add_to (pane: FolderApi): void {
        if (!this.config_props.full_screen) {
            // in full screen mode, user cannot adjust aspect ratio manually
            pane.addBinding(this, 'aspect_ratio', { min: 0.1, max: 3 })
        }
        pane.addBinding(this, 'background')
        pane.addBinding(this, 'light_intensity', { min: 0.1, max: 10 })
        pane.addBinding(this, 'show_stats')
        pane.addBinding(this, 'segments', { step: 1, min: 3, max: 128 })
    }
}

export class SnapshotConfig {
    index: number = 0
    name: number = 0

    add_to (pane: FolderApi, snapshot_names: string[]): void {
        pane.addBinding(this, 'index', { step: 1, min: 0, max: snapshot_names.length - 1 }).on('change', () => {
            this.name = this.index
            pane.refresh()
        })
        const options: { [Name: string]: number } = {}
        for (const [index, name] of snapshot_names.entries()) {
            options[name] = index
        }
        pane.addBinding(this, 'name', { options }).on('change', () => {
            this.index = this.name
            pane.refresh()
        })
    }
}

const names = ['Top', 'Left', 'Front']
const positions = [new Vector3(0, 1000, 0), new Vector3(-1000, 0, 0), new Vector3(0, 0, 1000)]
export class CameraConfig {
    zoom: number = 0.2
    position: Vector3 = positions[0].clone()
    orthographic_camera?: OrthographicCamera
    orbit_control?: OrbitControls

    add_to (pane: FolderApi): void {
        pane.addBlade({
            view: 'buttongrid',
            size: [3, 1],
            cells: (x: number) => ({
                title: names[x]
            }),
            label: 'reset view'
        }).on('click', (event: any) => {
            const i: number = event.index[0]
            this.set_position(names[i])
        })
        this.zoom = this.zoom * 0.999 // trigger camera zoom
        pane.addBinding(this, 'zoom', { min: 0.001, max: 1000 })
        if (this.orthographic_camera != null) {
            pane.addBinding(this, 'position')
        }
    }

    set_position (name: string) {
        const index = names.indexOf(name)
        if (index == -1) {
            console.error(`position name "${name}" is not recognized`)
            return
        }
        this.position = positions[index].clone()
        if (this.orbit_control != undefined) {
            this.orbit_control.target = new Vector3()
        }
    }
}

export class VertexConfig {
    radius: number = 0.15
    outline_ratio: number = 1.2
    normal_color: string = '#FFFFFF'
    defect_color: string = '#FF0000'

    add_to (pane: FolderApi): void {
        pane.addBinding(this, 'radius', { min: 0, max: 10, step: 0.001 })
        pane.addBinding(this, 'outline_ratio', { min: 0, max: 10, step: 0.001 })
        pane.addBinding(this, 'normal_color')
        pane.addBinding(this, 'defect_color')
    }
}

export class EdgeConfig {
    radius: number = 0.03

    add_to (pane: FolderApi): void {
        pane.addBinding(this, 'radius', { min: 0, max: 1, step: 0.001 })
    }
}

export interface PointerIntersectEventInterface {
    type: 'pointerenter' | 'pointerover' | 'pointermove' | 'pointerleave' | 'click'
    component: any
    over?: boolean
    intersect?: Intersection
}
