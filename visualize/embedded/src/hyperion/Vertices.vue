<script setup lang="ts">
import { type Ref, computed, watchEffect, useTemplateRef, onMounted } from 'vue'
import { type Config, type Position, load_position } from './hyperion'
import { PhysicalMaterial, SphereGeometry } from 'troisjs'
import MyInstancedMesh from '@/misc/MyInstancedMesh.vue'
import { Object3D, FrontSide, BackSide } from 'three'
import { assert_inject } from '@/misc/util'

const config: Ref<Config> = assert_inject('config')

class VertexState {
    userData: any = null
    position: Position

    constructor() {
        this.position = { t: 0, i: 0, j: 0 }
    }
}

class VertexStates {
    normal_vertices: Array<VertexState> = []
    defect_vertices: Array<VertexState> = []
    all_vertices: Array<VertexState> = []
}

// the vertices information computed from the snapshot, regardless of whether they are displayed
const vertex_states = computed(() => {
    const vertex_states = new VertexStates()
    const snapshot = config.value.snapshot
    for (const [i, vertex] of snapshot.vertices.entries()) {
        if (vertex == null) {
            continue
        }
        const state = new VertexState()
        state.userData = {
            type: 'vertex',
            vi: i
        }
        state.position = config.value.data.visualizer.positions[i]
        vertex_states.all_vertices.push(state)
        if (vertex.s) {
            vertex_states.normal_vertices.push(state)
        } else {
            vertex_states.defect_vertices.push(state)
        }
    }
    return vertex_states
})

function applyMeshVertices(vertex_states: Array<VertexState>, mesh: any) {
    const dummy = new Object3D()
    for (let i = 0; i < vertex_states.length; i++) {
        const state = vertex_states[i]
        load_position(dummy.position, state.position)
        dummy.updateMatrix()
        mesh.setMatrixAt(i, dummy.matrix)
    }
    mesh.instanceMatrix.needsUpdate = true
}

const normal_vertices_ref = useTemplateRef('normal_vertices')
const defect_vertices_ref = useTemplateRef('defect_vertices')
const vertices_outlines_ref = useTemplateRef('vertices_outlines')

const update_normal_vertices = () =>
    applyMeshVertices(vertex_states.value.normal_vertices, (normal_vertices_ref.value as any).mesh)
const update_defect_vertices = () =>
    applyMeshVertices(vertex_states.value.defect_vertices, (defect_vertices_ref.value as any).mesh)
const update_vertices_outlines = () =>
    applyMeshVertices(vertex_states.value.all_vertices, (vertices_outlines_ref.value as any).mesh)
function update() {
    update_normal_vertices()
    update_defect_vertices()
    update_vertices_outlines()
}

onMounted(() => {
    // when anything changes, update the mesh
    watchEffect(() => update())
})
</script>

<template>
    <!-- normal vertices -->
    <MyInstancedMesh
        ref="normal_vertices"
        :count="vertex_states.normal_vertices.length"
        @reinstantiated="update_normal_vertices"
    >
        <SphereGeometry
            :radius="config.vertex.radius"
            :height-segments="config.basic.segments"
            :width-segments="config.basic.segments"
        >
        </SphereGeometry>
        <PhysicalMaterial
            :props="{
                transparent: false,
                side: FrontSide,
                color: config.vertex.normal_color
            }"
        ></PhysicalMaterial>
    </MyInstancedMesh>

    <!-- defect vertices -->
    <MyInstancedMesh
        ref="defect_vertices"
        :count="vertex_states.defect_vertices.length"
        @reinstantiated="update_defect_vertices"
    >
        <SphereGeometry
            :radius="config.vertex.radius"
            :height-segments="config.basic.segments"
            :width-segments="config.basic.segments"
        >
        </SphereGeometry>
        <PhysicalMaterial
            :props="{
                transparent: false,
                side: FrontSide,
                color: config.vertex.defect_color
            }"
        ></PhysicalMaterial>
    </MyInstancedMesh>

    <!-- all vertices outlines -->
    <MyInstancedMesh
        ref="vertices_outlines"
        :count="vertex_states.all_vertices.length"
        @reinstantiated="update_vertices_outlines"
    >
        <SphereGeometry
            :radius="config.vertex.radius * config.vertex.outline_ratio"
            :height-segments="config.basic.segments"
            :width-segments="config.basic.segments"
        >
        </SphereGeometry>
        <PhysicalMaterial
            :props="{
                transparent: false,
                side: BackSide,
                color: '#000000'
            }"
        ></PhysicalMaterial>
    </MyInstancedMesh>
</template>
