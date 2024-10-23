import { type Ref, ref } from 'vue'
import { Pane, FolderApi } from 'tweakpane'
import { assert } from '@/util'
import { Vector3, type OrthographicCamera, type Intersection } from 'three'

export interface Position {
    t: number
    i: number
    j: number
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

export interface Snapshot {
    dual_nodes: DualNode[]
    edges: Edge[]
    interface: Interface
    vertices: Vertex[]
}

export type SnapshotTuple = string | Snapshot

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

/* configuration helper class given the runtime data */
export class Config {
    basic: BasicConfig = new BasicConfig()
    camera: CameraConfig = new CameraConfig()
    pane?: Pane

    constructor (data?: RuntimeData) {
        if (data != undefined) {
            // load other submodules here
        }
    }

    create_pane (container: HTMLElement | undefined) {
        assert(this.pane == null, 'cannot create pane twice')
        this.pane = new Pane({
            title: 'Visualizer Config',
            container: container,
            expanded: false
        })
        const pane: FolderApi = this.pane
        this.camera.add_to(pane.addFolder({ title: 'Camera', expanded: true }))
        this.basic.add_to(pane.addFolder({ title: 'Basic', expanded: true }))
    }
}

/* controls basic elements like background and aspect ratio */
export class BasicConfig {
    aspect_ratio: number = 1
    background: string = '#ffffff'
    show_stats: boolean = true

    add_to (pane: FolderApi): void {
        pane.addBinding(this, 'aspect_ratio', { min: 0.1, max: 3 })
        pane.addBinding(this, 'background')
        pane.addBinding(this, 'show_stats')
    }
}

const names = ['Top', 'Left', 'Front']
const positions = [new Vector3(0, 1000, 0), new Vector3(-1000, 0, 0), new Vector3(0, 0, 1000)]
export class CameraConfig {
    zoom: number = 0.2
    position: Vector3 = positions[0].clone()
    orthographic_camera?: OrthographicCamera

    add_to (pane: FolderApi): void {
        for (let i = 0; i < 3; ++i) {
            const button = pane.addButton({ title: names[i] })
            button.on('click', () => {
                this.position = positions[i].clone()
                pane.refresh()
            })
        }
        this.zoom = this.zoom * 0.99 // trigger camera zoom
        pane.addBinding(this, 'zoom')
        if (this.orthographic_camera != null) {
            pane.addBinding(this, 'position')
        }
    }
}

export interface PointerIntersectEventInterface {
    type: 'pointerenter' | 'pointerover' | 'pointermove' | 'pointerleave' | 'click'
    component: any
    over?: boolean
    intersect?: Intersection
}
