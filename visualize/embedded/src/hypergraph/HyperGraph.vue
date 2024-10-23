<script setup lang="ts">
import { onMounted, ref, watchEffect } from 'vue'
import { Renderer, OrthographicCamera, Scene, PointLight, Box, LambertMaterial } from 'troisjs'
import { assert } from '@/util';
import { WebGLRenderer } from 'three'

const container_ref = ref(null)
const renderer_ref = ref(null)
const width = ref(400)
const height = ref(400)


onMounted(() => {
    console.log(renderer_ref?.value)
    console.log(container_ref?.value)


    watchEffect(() => {
        assert(renderer_ref.value != null)
        const renderer: any = renderer_ref.value
        const webgl_renderer: WebGLRenderer = renderer.renderer
        webgl_renderer.setSize(width.value, height.value)
    })


    const container_resize_observer = new ResizeObserver((entries) => {
        for (const entry of entries) {
            const container_width = entry.contentRect.width
            width.value = container_width
            height.value = container_width
        }
    })
    assert(container_ref.value != null)
    container_resize_observer.observe(container_ref.value)

})

</script>

<template>
    <div ref="container_ref" class="hypergraph-container">
        <Renderer ref="renderer_ref" :width="width + 'px'" :height="height + 'px'" :orbit-ctrl="{}" :antialias="true"
            :alpha="true">
            <OrthographicCamera :position="{ z: 10 }" />
            <Scene>
                <PointLight :position="{ y: 50, z: 50 }" />
                <Box ref="box" :rotation="{ y: Math.PI / 4, z: Math.PI / 4 }">
                    <LambertMaterial />
                </Box>
            </Scene>
        </Renderer>
    </div>
</template>

<style scoped>
.hypergraph-container {
    margin: 10px;
}
</style>
