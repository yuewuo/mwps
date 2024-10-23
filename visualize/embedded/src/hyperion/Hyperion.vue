<script setup lang="ts">
import { onMounted, type Ref, type ComputedRef, ref, computed, provide, watchEffect, onBeforeUnmount } from 'vue'
import { Renderer, OrthographicCamera, Scene, AmbientLight, Raycaster } from 'troisjs'
import { type VisualizerData, RuntimeData, Config } from './hyperion'
import { WebGLRenderer, OrthographicCamera as ThreeOrthographicCamera } from 'three'
import { assert } from '@/util'
// @ts-ignore
import Stats from 'troisjs/src/components/misc/Stats'


interface Props {
    visualizer: VisualizerData
    hide_config?: boolean
    full_screen?: boolean
}

const props = withDefaults(defineProps<Props>(), {
    hide_config: false,
    full_screen: false
})

const data: Ref<RuntimeData> = computed(() => new RuntimeData(props.visualizer))
provide('data', data)  // prop drilling to all children components
const config = ref(new Config(data.value))
provide('config', config)

const container_ref: Ref<HTMLElement | undefined> = ref(undefined)
const container_pane: Ref<HTMLElement | undefined> = ref(undefined)
const renderer_ref: Ref<HTMLElement | undefined> = ref(undefined)
const width: Ref<number> = ref(400)
const height: ComputedRef<number> = computed(() => width.value / config.value.basic.aspect_ratio)
const orthographic_camera: Ref<HTMLElement | undefined> = ref(undefined)


onMounted(() => {
    // pass camera object
    const camera: any = orthographic_camera.value
    const three_camera: ThreeOrthographicCamera = camera.camera
    config.value.camera.orthographic_camera = three_camera

    // initialize controller pane
    if (!props.hide_config) {
        config.value.create_pane(container_pane.value)
    }

    // update renderer if width or height changes
    watchEffect(() => {
        const renderer: any = renderer_ref.value
        assert(renderer != undefined)
        const webgl_renderer: WebGLRenderer = renderer.renderer
        webgl_renderer.setSize(width.value, height.value)
    })

    // observe container size change and update the width and height values
    const container_resize_observer = new ResizeObserver((entries) => {
        for (const entry of entries) {
            const container_width = entry.contentRect.width
            width.value = container_width
            if (props.full_screen) {
                config.value.basic.aspect_ratio = document.documentElement.clientWidth / document.documentElement.clientHeight * 1.02
            }
        }
    })
    assert(container_ref.value != undefined)
    container_resize_observer.observe(container_ref.value)
})

onBeforeUnmount(() => {
    config.value.pane?.dispose()
})

</script>

<template>
    <div ref="container_ref" class="hyperion-container">
        <!-- placeholder for controller pane container -->
        <div ref="container_pane" class="pane-container"></div>

        <Renderer ref="renderer_ref" :width="width + 'px'" :height="height + 'px'" :orbit-ctrl="true" :antialias="true"
            :alpha="true" :params="{ powerPreference: 'high-performance' }">
            <OrthographicCamera :left="-config.basic.aspect_ratio" :right="config.basic.aspect_ratio"
                :zoom="config.camera.zoom" :position="config.camera.position" ref="orthographic_camera">
            </OrthographicCamera>
            <Stats v-if="config.basic.show_stats"></Stats>
            <Raycaster @pointer-enter="data.onPointerEnter" @pointer-leave="data.onPointerLeave"></Raycaster>
            <Scene :background="config.basic.background">
                <AmbientLight></AmbientLight>
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