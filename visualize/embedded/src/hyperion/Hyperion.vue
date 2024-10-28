<script setup lang="ts">
import { onMounted, ref, computed, provide, watchEffect, onBeforeUnmount, useTemplateRef } from 'vue'
import { Renderer, OrthographicCamera, Scene, AmbientLight, Raycaster } from 'troisjs'
import { type VisualizerData, RuntimeData, Config, ConfigProps } from './hyperion'
import Vertices from './Vertices.vue'
import { WebGLRenderer, OrthographicCamera as ThreeOrthographicCamera } from 'three'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import { assert } from '@/util'
// @ts-ignore
import Stats from 'troisjs/src/components/misc/Stats'

interface Props {
    visualizer: VisualizerData
    config: ConfigProps
}

const props = withDefaults(defineProps<Props>(), {
    config: () => new ConfigProps()
})

const config = ref(new Config(new RuntimeData(props.visualizer), props.config))
provide('config', config) // prop drilling to all children components

const container = useTemplateRef('container_ref')
const container_pane = useTemplateRef('container_pane_ref')
const show_config = ref(props.config.show_config)
const renderer = useTemplateRef('renderer_ref')
const width = ref(400)
const height = computed(() => width.value / config.value.basic.aspect_ratio)
const orthographic_camera = useTemplateRef('orthographic_camera_ref')

onMounted(() => {
    // pass camera object
    const three_camera: ThreeOrthographicCamera = (orthographic_camera.value as any).camera
    config.value.camera.orthographic_camera = three_camera

    // initialize controller pane
    config.value.create_pane(container_pane.value)

    // make the renderer selected in HTML: https://stackoverflow.com/a/12887221, to react to key events
    const canvas: HTMLElement = (renderer.value as any).canvas
    canvas.setAttribute('tabindex', '1')
    canvas.style.setProperty('outline-style', 'none') // remove select border

    // listen to orbit control events
    const orbit_controls: OrbitControls = (renderer.value as any).three.cameraCtrl
    orbit_controls.addEventListener('change', () => {
        canvas.focus()
    })

    // update renderer if width or height changes
    watchEffect(() => {
        const webgl_renderer: WebGLRenderer = (renderer.value as any).renderer
        webgl_renderer.setSize(width.value, height.value)
    })

    // observe container size change and update the width and height values
    const container_resize_observer = new ResizeObserver(entries => {
        for (const entry of entries) {
            const container_width = entry.contentRect.width
            width.value = container_width
            if (props.config.full_screen) {
                config.value.aspect_ratio =
                    (document.documentElement.clientWidth / document.documentElement.clientHeight) * 1.02
            }
        }
    })
    container_resize_observer.observe(container.value as any)
})

onBeforeUnmount(() => {
    config.value.pane?.dispose()
})

function onKeyDown(event: KeyboardEvent) {
    if (!event.metaKey) {
        if (event.key == 't' || event.key == 'T') {
            config.value.camera.set_position('Top')
        } else if (event.key == 'l' || event.key == 'L') {
            config.value.camera.set_position('Left')
        } else if (event.key == 'f' || event.key == 'F') {
            config.value.camera.set_position('Front')
        } else if (event.key == 'c' || event.key == 'C') {
            show_config.value = !show_config.value
        } else if (event.key == 's' || event.key == 'S') {
            config.value.basic.show_stats = !config.value.basic.show_stats
        } else if (event.key == 'ArrowRight') {
            if (config.value.snapshot_index < config.value.snapshot_num - 1) {
                config.value.snapshot_index += 1
            }
        } else if (event.key == 'ArrowLeft') {
            if (config.value.snapshot_index > 0) {
                config.value.snapshot_index -= 1
            }
        }
    }
}
</script>

<template>
    <div ref="container_ref" class="hyperion-container" @keydown="onKeyDown">
        <!-- placeholder for controller pane container -->
        <div v-show="show_config" ref="container_pane_ref" class="pane-container"></div>

        <Renderer
            ref="renderer_ref"
            :width="width + 'px'"
            :height="height + 'px'"
            :orbit-ctrl="true"
            :antialias="true"
            :alpha="true"
            :params="{ powerPreference: 'high-performance' }"
        >
            <OrthographicCamera
                :left="-config.basic.aspect_ratio"
                :right="config.basic.aspect_ratio"
                :zoom="config.camera.zoom"
                :position="config.camera.position"
                ref="orthographic_camera_ref"
            >
            </OrthographicCamera>
            <Stats v-if="config.basic.show_stats"></Stats>
            <Raycaster @pointer-enter="config.data.onPointerEnter" @pointer-leave="config.data.onPointerLeave">
            </Raycaster>
            <Scene :background="config.basic.background">
                <AmbientLight></AmbientLight>
                <Vertices></Vertices>
            </Scene>
        </Renderer>
    </div>
</template>

<style scoped>
.hyperion-container {
    margin: 10px;
    position: relative;
}

.pane-container {
    position: absolute;
    top: 0;
    right: 0;
    width: 300px;
    padding: 0;
    margin: 0;
}
</style>
