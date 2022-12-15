import * as gui3d from './gui3d.js'
import * as patches from './patches.js'

if (typeof window === 'undefined' || typeof document === 'undefined') {
    await import('./mocker.js')
}

window.gui3d = gui3d

const is_mock = typeof mockgl !== 'undefined'
if (is_mock) {
    global.gui3d = gui3d
    global.mocker = await import('./mocker.js')
}

// to work both in browser and nodejs
if (typeof Vue === 'undefined') {
    global.Vue = await import('vue')
}
const { ref, reactive, watch, computed } = Vue

// fetch visualization data
const urlParams = new URLSearchParams(window.location.search)
const filename = urlParams.get('filename') || "visualizer.json"

export var mwps_data
var patch_done = ref(false)

// alert(navigator.userAgent)
const is_chrome = navigator.userAgent.toLowerCase().indexOf('chrome/') > -1
const is_firefox = navigator.userAgent.toLowerCase().indexOf('firefox/') > -1
const is_browser_supported = ref(is_chrome || is_firefox)

export const snapshot_select = ref(0)

// create vue3 app
const App = {
    setup() {
        return {
            error_message: ref(null),
            warning_message: ref(null),
            snapshot_num: ref(1),
            snapshot_select: snapshot_select,
            snapshot_select_label: ref(1),
            snapshot_labels: ref([]),
            mwps_data_ready: ref(false),
            use_perspective_camera: gui3d.use_perspective_camera,
            sizes: gui3d.sizes,
            export_scale_selected: ref(1),
            export_resolution_options: ref([]),
            // GUI related states
            show_stats: gui3d.show_stats,
            show_config: gui3d.show_config,
            show_hover_effect: gui3d.show_hover_effect,
            lock_view: ref(false),
            is_browser_supported: is_browser_supported,
            // select
            current_selected: gui3d.current_selected,
            selected_vertex_neighbor_edges: ref([]),
            selected_vertex_attributes: ref(""),
            selected_edge: ref(null),
            selected_edge_attributes: ref(""),
            dual_module_info: ref(null),
        }
    },
    async mounted() {
        gui3d.root.style.setProperty('--control-visibility', 'visible')
        let response = null
        try {
            response = await fetch('./data/' + filename, { cache: 'no-cache', })
        } catch (e) {
            this.error_message = "fetch file error"
            throw e
        }
        if (response.ok || is_mock) {
            mwps_data = await response.json()
            // console.log(mwps_data)
            if (mwps_data.format != "mwps") {
                this.error_message = `visualization file format error, get "${mwps_data.format}" expected "mwps"`
                throw this.error_message
            }
        } else {
            this.error_message = `fetch file error ${response.status}: ${response.statusText}`
            throw this.error_message
        }
        // load snapshot
        this.show_snapshot(0)  // load the first snapshot
        this.snapshot_num = mwps_data.snapshots.length
        for (let [idx, [name, _]] of mwps_data.snapshots.entries()) {
            this.snapshot_labels.push(`[${idx}] ${name}`)
        }
        this.snapshot_select_label = this.snapshot_labels[0]
        this.mwps_data_ready = true
        // only if data loads successfully will the animation starts
        if (!is_mock) {  // if mock, no need to refresh all the time
            gui3d.animate()
        }
        // add keyboard shortcuts
        document.onkeydown = (event) => {
            if (!event.metaKey) {
                if (event.key == "t" || event.key == "T") {
                    this.reset_camera("top")
                } else if (event.key == "l" || event.key == "L") {
                    this.reset_camera("left")
                } else if (event.key == "f" || event.key == "F") {
                    this.reset_camera("front")
                } else if (event.key == "c" || event.key == "C") {
                    this.show_config = !this.show_config
                } else if (event.key == "s" || event.key == "S") {
                    this.show_stats = !this.show_stats
                } else if (event.key == "v" || event.key == "V") {
                    this.lock_view = !this.lock_view
                } else if (event.key == "o" || event.key == "O") {
                    this.use_perspective_camera = false
                } else if (event.key == "p" || event.key == "P") {
                    this.use_perspective_camera = true
                } else if (event.key == "e" || event.key == "E") {
                    this.toggle_error_correction()
                } else if (event.key == "ArrowRight") {
                    if (this.snapshot_select < this.snapshot_num - 1) {
                        this.snapshot_select += 1
                    }
                } else if (event.key == "ArrowLeft") {
                    if (this.snapshot_select > 0) {
                        this.snapshot_select -= 1
                    }
                } else {
                    return  // unrecognized, propagate to other listeners
                }
                event.preventDefault()
                event.stopPropagation()
            }
        }
        // get command from url parameters
        await Vue.nextTick()
        let snapshot_idx = urlParams.get('si') || urlParams.get('snapshot_idx')
        if (snapshot_idx != null) {
            snapshot_idx = parseInt(snapshot_idx)
            if (snapshot_idx < 0) {  // iterate from the end, like python list[-1]
                snapshot_idx = this.snapshot_num + snapshot_idx
                if (snapshot_idx < 0) {  // too small
                    snapshot_idx = 0
                }
            }
            if (snapshot_idx >= this.snapshot_num) {
                snapshot_idx = this.snapshot_num - 1
            }
            this.snapshot_select = snapshot_idx
        }
        this.update_dual_module_info()
        // update resolution options when sizes changed
        watch(gui3d.sizes, this.update_export_resolutions, { immediate: true })
        // execute patch scripts
        setTimeout(async () => {
            const patch_name = urlParams.get('patch')
            if (patch_name != null) {
                console.log(`running patch ${patch_name}`)
                const patch_function = patches[patch_name]
                await patch_function.bind(this)()
            }
            const patch_url = urlParams.get('patch_url')
            if (patch_url != null) {
                this.warning_message = `patching from external file: ${patch_url}`
                let patch_module = await import(patch_url)
                if (patch_module.patch == null) {
                    this.error_message = "invalid patch file: `patch` function not found"
                    throw "patch file error"
                }
                await patch_module.patch.bind(this)()
                this.warning_message = null
            }
            patch_done.value = true
        }, 100);
    },
    methods: {
        show_snapshot(snapshot_idx) {
            try {
                window.mwps_data = mwps_data
                window.snapshot_idx = snapshot_idx
                gui3d.show_snapshot(snapshot_idx, mwps_data)
            } catch (e) {
                this.error_message = "load data error"
                throw e
            }
        },
        reset_camera(direction) {
            gui3d.reset_camera_position(direction)
        },
        async update_selected_display() {
            if (this.current_selected == null) return
            if (this.current_selected.type == "vertex") {
                let vertex_index = this.current_selected.vertex_index
                let vertex = this.snapshot.vertices[vertex_index]
                this.selected_vertex_attributes = ""
                if (vertex.s == 1) {
                    this.selected_vertex_attributes += "(defect) "
                }
                // fetch edge list
                let neighbor_edges = []
                for (let [edge_index, edge] of this.snapshot.edges.entries()) {
                    if (edge == null) {
                        continue
                    }
                    for (let v_idx of edge.v) {
                        if (v_idx == vertex_index) {
                            neighbor_edges.push({
                                edge_index: edge_index,
                                grown: edge.g,
                                unexplored: edge.w - edge.g,
                                weight: edge.w,
                                vertices: [...edge.v],
                            })
                            break
                        }
                    }
                }
                this.selected_vertex_neighbor_edges = neighbor_edges
            }
            if (this.current_selected.type == "edge") {
                const edge_index = this.current_selected.edge_index
                const edge = this.snapshot.edges[edge_index]
                this.selected_edge = {
                    edge_index: edge_index,
                    grown: edge.g,
                    unexplored: edge.w - edge.g,
                    weight: edge.w,
                    vertices: [...edge.v],
                }
                this.selected_edge_attributes = ""
            }
        },
        jump_to(type, data, is_click=true) {
            let current_ref = is_click ? gui3d.current_selected : gui3d.current_hover
            if (type == "edge") {
                current_ref.value = {
                    type, edge_index: data
                }
            }
            if (type == "vertex") {
                current_ref.value = {
                    type, vertex_index: data
                }
            }
            if (type == "vertices") {
                current_ref.value = {
                    type, vertices: data
                }
            }
            if (type == "edges") {
                current_ref.value = {
                    type, edges: data
                }
            }
        },
        mouseenter(type, data) {
            this.jump_to(type, data, false)
        },
        mouseleave() {
            gui3d.current_hover.value = null
        },
        update_export_resolutions() {
            this.export_resolution_options.splice(0, this.export_resolution_options.length)
            let exists_in_new_resolution = false
            for (let i=-100; i<100; ++i) {
                let scale = 1 * Math.pow(10, i/10)
                let width = Math.round(this.sizes.canvas_width * scale)
                let height = Math.round(this.sizes.canvas_height * scale)
                if (width > 5000 || height > 5000) {  // to large, likely exceeds WebGL maximum buffer size
                    break
                }
                if (width >= 300 || height >= 300) {
                    let label = `${width} x ${height}`
                    this.export_resolution_options.push({
                        label: label,
                        value: scale,
                    })
                    if (scale == this.export_scale_selected) {
                        exists_in_new_resolution = true
                    }
                }
            }
            if (!exists_in_new_resolution) {
                this.export_scale_selected = null
            }
        },
        preview_image() {
            const data = gui3d.render_png(this.export_scale_selected)
            gui3d.open_png(data)
        },
        download_image() {
            const data = gui3d.render_png(this.export_scale_selected)
            gui3d.download_png(data)
        },
        get_idx_from_label(label) {
            return parseInt(label.split(']')[0].split('[')[1])
        },
        update_dual_module_info() {
            let snapshot = mwps_data.snapshots[this.snapshot_select][1]
            let dual_nodes = snapshot.dual_nodes
            this.dual_module_info = []
            for (let node of dual_nodes) {
                this.dual_module_info.push({
                    ...node
                })
                console.log(node)
            }
        },
    },
    watch: {
        async snapshot_select() {
            // console.log(this.snapshot_select)
            this.show_snapshot(this.snapshot_select)
            this.snapshot_select_label = this.snapshot_labels[this.snapshot_select]
            for (const _ of Array(4).keys()) await Vue.nextTick()
            await this.update_selected_display()
            this.update_dual_module_info()
        },
        snapshot_select_label() {
            this.snapshot_select = this.get_idx_from_label(this.snapshot_select_label)
        },
        async current_selected() {
            await this.update_selected_display()
        },
        lock_view() {
            gui3d.enable_control.value = !this.lock_view
        },
    },
    computed: {
        scale() {
            return this.sizes.scale
        },
        vertical_thumb_style() {
            return {
                right: `4px`,
                borderRadius: `5px`,
                backgroundColor: '#027be3',
                width: `5px`,
                opacity: 0.75
            }
        },
        horizontal_thumb_style() {
            return {
                bottom: `4px`,
                borderRadius: `5px`,
                backgroundColor: '#027be3',
                height: `5px`,
                opacity: 0.75
            }
        },
        vertical_bar_style() {
            return {
                right: `2px`,
                borderRadius: `9px`,
                backgroundColor: '#027be3',
                width: `9px`,
                opacity: 0.2
            }
        },
        horizontal_bar_style() {
            return {
                bottom: `2px`,
                borderRadius: `9px`,
                backgroundColor: '#027be3',
                height: `9px`,
                opacity: 0.2
            }
        },
        snapshot() {
            return mwps_data.snapshots[this.snapshot_select][1]
        },
    },
}

if (!is_mock) {
    const app = Vue.createApp(App)
    app.use(Quasar)
    window.app = app.mount("#app")
} else {
    global.Element = window.Element
    global.SVGElement = window.SVGElement  // https://github.com/jsdom/jsdom/issues/2734
    App.template = "<div></div>"
    const app = Vue.createApp(App)
    window.app = app.mount("#app")
    while (!patch_done.value) {
        await sleep(50)
    }
    for (let i=0; i<10; ++i) {
        await sleep(10)
        await Vue.nextTick()
    }
    console.log("[rendering]")
    const pixels = await gui3d.nodejs_render_png()
    console.log("[saving]")
    mocker.save_pixels(pixels)
}
