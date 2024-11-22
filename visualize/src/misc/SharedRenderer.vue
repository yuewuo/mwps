<script lang="ts">
import { Camera, Scene, WebGLRenderer, type WebGLRendererParameters } from 'three'
import { type ComponentPublicInstance, defineComponent, type PropType, watch } from 'vue'
import { RendererInjectionKey } from 'troisjs'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'

export interface EventInterface {
    type: 'init' | 'mounted'
    renderer: RendererInterface
}

export interface RenderEventInterface {
    type: 'beforerender' | 'afterrender'
    renderer: RendererInterface
    time: number
}

type CallbackType<T> = (event: T) => void
type InitCallbackType = CallbackType<EventInterface>
type MountedCallbackType = CallbackType<EventInterface>
type RenderCallbackType = CallbackType<RenderEventInterface>

interface EventCallbackMap {
    init: InitCallbackType
    mounted: MountedCallbackType
    beforerender: RenderCallbackType
    afterrender: RenderCallbackType
}

interface RenderFunctionEventInterface {
    renderer: RendererInterface
    time: number
}

interface RendererSetupInterface {
    canvas: HTMLCanvasElement
    three: ThreeRenderer
    renderer: WebGLRenderer
    renderFn(e: RenderFunctionEventInterface): void
    raf: boolean

    initCallbacks: InitCallbackType[]
    mountedCallbacks: MountedCallbackType[]
    beforeRenderCallbacks: RenderCallbackType[]
    afterRenderCallbacks: RenderCallbackType[]
}

export interface RendererInterface extends RendererSetupInterface {
    scene?: Scene
    camera?: Camera

    onInit(cb: InitCallbackType): void
    onMounted(cb: MountedCallbackType): void

    onBeforeRender(cb: RenderCallbackType): void
    offBeforeRender(cb: RenderCallbackType): void
    onAfterRender(cb: RenderCallbackType): void
    offAfterRender(cb: RenderCallbackType): void

    addListener<T extends keyof EventCallbackMap>(t: T, cb: EventCallbackMap[T]): void
    removeListener<T extends keyof EventCallbackMap>(t: T, cb: EventCallbackMap[T]): void
    isVisibleInViewport(): boolean
}

export interface RendererPublicInterface extends ComponentPublicInstance, RendererInterface {}

class GlobalThreeRenderer {
    renderer: WebGLRenderer
    width: number = 0
    height: number = 0
    local_renderers: ThreeRenderer[] = []

    constructor(params: WebGLRendererParameters) {
        // create a renderer with drawing buffer so that the rendered result can be captured
        this.renderer = new WebGLRenderer({ ...params, preserveDrawingBuffer: true })
        this.renderer.setPixelRatio(window.devicePixelRatio)
        requestAnimationFrame(this.renderLoop.bind(this))
        setInterval(() => {
            for (const renderer of this.local_renderers) {
                try {
                    renderer.visible = renderer.parent?.isVisibleInViewport() || false
                } catch (e) {
                    console.error(e)
                }
            }
        }, 100)
    }

    renderLoop(time: number) {
        requestAnimationFrame(this.renderLoop.bind(this))
        this.render(time)
    }

    render(time: number) {
        for (const renderer of this.local_renderers) {
            try {
                renderer.render(time)
            } catch (e) {
                renderer.visible = false
                console.error(e)
            }
        }
    }
}

// maintain a single global renderer
export let global_renderer: GlobalThreeRenderer | undefined = undefined

class ThreeRenderer {
    renderer: WebGLRenderer
    global_renderer: GlobalThreeRenderer
    camera?: Camera // set by the children node
    scene?: Scene // set by the children node
    parent?: RendererInterface
    cameraCtrl?: OrbitControls
    visible: boolean = true // will be updated every 100ms to reduce cost of `getBoundingClientRect`

    constructor(global_renderer: GlobalThreeRenderer) {
        this.renderer = global_renderer.renderer
        this.global_renderer = global_renderer
        global_renderer.local_renderers.push(this)
        console.log(`adding one renderer, in total: ${global_renderer.local_renderers.length}`)
    }
    dispose() {
        this.global_renderer.local_renderers = this.global_renderer.local_renderers.filter(renderer => renderer != this)
        console.log(`removing one renderer, remaining: ${this.global_renderer.local_renderers.length}`)
    }
    init() {
        if (!this.scene) {
            console.error('Missing Scene')
            return
        }

        if (!this.camera) {
            console.error('Missing Camera')
            return false
        }

        if (this.parent) {
            this.cameraCtrl = new OrbitControls(this.camera, this.parent.canvas)
            this.parent.onBeforeRender(() => {
                this.cameraCtrl?.update()
            })

            this.parent.renderFn = () => {
                const canvas: HTMLCanvasElement = this.parent?.canvas as any
                if (canvas.clientWidth == 0 || canvas.clientHeight == 0) return
                canvas.width = canvas.clientWidth
                canvas.height = canvas.clientHeight
                this.renderer.setSize(canvas.clientWidth, canvas.clientHeight)
                this.renderer.setPixelRatio(window.devicePixelRatio)
                this.renderer.render(this.scene as Scene, this.camera as Camera)
                const context = canvas.getContext('2d')
                context?.drawImage(this.renderer.domElement, 0, 0, canvas.width, canvas.height)
            }
        }
    }
    render(time: number) {
        // check if the renderer is in the view
        if (!this.visible) {
            return
        }
        if (this.parent && this.parent.raf) {
            const parent: RendererInterface = this.parent
            parent.beforeRenderCallbacks.forEach(e => e({ type: 'beforerender', renderer: parent, time }))
            parent.renderFn({ renderer: parent, time })
            parent.afterRenderCallbacks.forEach(e => e({ type: 'afterrender', renderer: parent, time }))
        }
    }

    // be compatible with ThreeInterface in troisjs, but we do not need the pointer implementation
    addIntersectObject() {}
    removeIntersectObject() {}
}

export default defineComponent({
    name: 'Renderer',
    props: {
        params: { type: Object as PropType<WebGLRendererParameters>, default: () => ({}) },
        antialias: Boolean,
        alpha: Boolean,
        orbitCtrl: { type: Boolean as PropType<boolean>, default: false },
        width: String,
        height: String,
        pixelRatio: Number,
        xr: Boolean,
        props: { type: Object, default: () => ({}) },
    },
    inheritAttrs: false,
    setup(props, { attrs }): RendererSetupInterface {
        const initCallbacks: InitCallbackType[] = []
        const mountedCallbacks: MountedCallbackType[] = []
        const beforeRenderCallbacks: RenderCallbackType[] = []
        const afterRenderCallbacks: RenderCallbackType[] = []

        const canvas = document.createElement('canvas')
        Object.entries(attrs).forEach(([key, value]) => {
            const matches = key.match(/^on([A-Z][a-zA-Z]*)$/)
            if (matches) {
                canvas.addEventListener(matches[1].toLowerCase(), value as { (): void })
            } else {
                canvas.setAttribute(key, value as string)
            }
        })

        if (global_renderer == undefined) {
            global_renderer = new GlobalThreeRenderer(props.params)
        }

        const three = new ThreeRenderer(global_renderer)
        watch([props], () => {
            canvas.style.width = props.width || '100%'
            canvas.style.height = props.height || '100%'
        })

        const renderFn: { (): void } = () => {}

        return {
            canvas,
            three,
            renderer: three.renderer,
            renderFn,
            raf: false,
            initCallbacks,
            mountedCallbacks,
            beforeRenderCallbacks,
            afterRenderCallbacks,
        }
    },
    computed: {
        camera: {
            get: function (): Camera | undefined {
                return this.three.camera
            },
            set: function (camera: Camera): void {
                this.three.camera = camera
            },
        },
        scene: {
            get: function (): Scene | undefined {
                return this.three.scene
            },
            set: function (scene: Scene): void {
                this.three.scene = scene
            },
        },
    },
    provide() {
        return {
            [RendererInjectionKey as symbol]: this,
        }
    },
    mounted() {
        // appendChild won't work on reload
        this.$el.parentNode.insertBefore(this.canvas, this.$el)
        this.three.parent = this

        this.three.init()

        this.mountedCallbacks.forEach(e => e({ type: 'mounted', renderer: this }))

        this.raf = true
    },
    beforeUnmount() {
        this.canvas.remove()
        this.beforeRenderCallbacks = []
        this.afterRenderCallbacks = []
        this.raf = false
        this.three.dispose()
    },
    methods: {
        onInit(cb: InitCallbackType) {
            this.addListener('init', cb)
        },
        onMounted(cb: MountedCallbackType) {
            this.addListener('mounted', cb)
        },
        onBeforeRender(cb: RenderCallbackType) {
            this.addListener('beforerender', cb)
        },
        offBeforeRender(cb: RenderCallbackType) {
            this.removeListener('beforerender', cb)
        },
        onAfterRender(cb: RenderCallbackType) {
            this.addListener('afterrender', cb)
        },
        offAfterRender(cb: RenderCallbackType) {
            this.removeListener('afterrender', cb)
        },

        addListener(type: string, cb: { (e?: any): void }) {
            const callbacks = this.getCallbacks(type)
            callbacks.push(cb)
        },

        removeListener(type: string, cb: { (e?: any): void }) {
            const callbacks = this.getCallbacks(type)
            const index = callbacks.indexOf(cb)
            if (index !== -1) callbacks.splice(index, 1)
        },

        getCallbacks(type: string) {
            if (type === 'init') {
                return this.initCallbacks
            } else if (type === 'mounted') {
                return this.mountedCallbacks
            } else if (type === 'beforerender') {
                return this.beforeRenderCallbacks
            } else if (type === 'afterrender') {
                return this.afterRenderCallbacks
            } else {
                return []
            }
        },
        isVisibleInViewport(partiallyVisible = true) {
            const { top, left, bottom, right } = this.canvas.getBoundingClientRect()
            const { innerHeight, innerWidth } = window
            return partiallyVisible
                ? ((top > 0 && top < innerHeight) || (bottom > 0 && bottom < innerHeight)) && ((left > 0 && left < innerWidth) || (right > 0 && right < innerWidth))
                : top >= 0 && left >= 0 && bottom <= innerHeight && right <= innerWidth
        },
    },
    render() {
        return this.$slots.default ? this.$slots.default() : []
    },
    __hmrId: 'Renderer',
})
</script>
