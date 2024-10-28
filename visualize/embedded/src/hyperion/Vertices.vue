<script setup lang="ts">
import { type Ref, computed, watchEffect, useTemplateRef, onMounted } from 'vue'
import { type Config, type Position } from './hyperion'
import { StandardMaterial, SphereGeometry } from 'troisjs'
import MyInstancedMesh from '@/misc/MyInstancedMesh.vue'
import { Object3D, Color } from 'three'
import { assert_inject } from '@/misc/util'

const config: Ref<Config> = assert_inject('config')

const vertices_ref = useTemplateRef('vertices')

class VertexState {
    userData: any = null
    position: Position

    constructor() {
        this.position = { t: 0, i: 0, j: 0 }
    }
}

// the vertices information computed from the snapshot, regardless of whether they are displayed
const vertex_states = computed(() => {
    const vertex_states: Array<VertexState> = []
    const snapshot = config.value.snapshot
    for (const [i, vertex] of snapshot.vertices.entries()) {
        if (vertex == null) {
            continue
        }
        const state = new VertexState()
        state.userData = {
            vi: i
        }
        let position = config.value.data.visualizer.positions[i]
        console.log(vertex)
        vertex_states.push(state)
    }
    return vertex_states
})

function updateMesh() {
    const imesh = (vertices_ref.value as any).mesh
    const dummy = new Object3D()
    for (let i = 0; i < vertex_states.value.length; i++) {
        const state = vertex_states.value[i]
        dummy.position.set(0, i, 0)
        const scale = 1
        dummy.scale.set(scale, scale, scale)
        dummy.updateMatrix()
        imesh.setMatrixAt(i, dummy.matrix)
        imesh.setColorAt(i, new Color('red'))
    }
    imesh.instanceMatrix.needsUpdate = true
    imesh.instanceColor.needsUpdate = true
}

onMounted(() => {
    // when anything changes, update the mesh
    watchEffect(() => updateMesh())
})
</script>

<template>
    <MyInstancedMesh ref="vertices" :maxcount="100" :count="vertex_states.length" @reinstantiated="updateMesh">
        <SphereGeometry :radius="1" :height-segments="config.basic.segments" :width-segments="config.basic.segments">
        </SphereGeometry>
        <StandardMaterial :props="{ transparent: true, opacity: 0.5 }"></StandardMaterial>
    </MyInstancedMesh>
</template>
