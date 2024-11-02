<script setup lang="ts">
import { type Ref, computed, watchEffect, useTemplateRef, onMounted } from 'vue'
import { type Position, load_position } from './hyperion'
import { type Config } from './config_pane'
import { PhysicalMaterial, SphereGeometry } from 'troisjs'
import MyInstancedMesh from '@/misc/MyInstancedMesh.vue'
import { Object3D, FrontSide, BackSide, Color } from 'three'
import { assert_inject } from '@/misc/util'

const config: Ref<Config> = assert_inject('config')

class VertexState {
    type: string = 'vertex'
    vi: number
    position: Position

    constructor(vi: number, position: Position) {
        this.vi = vi
        this.position = position
    }
}

class VertexStates {
    normal_vertices: Array<VertexState> = []
    defect_vertices: Array<VertexState> = []
    all_vertices: Array<VertexState> = []
}

const vertex_states = computed(() => {
    const vertex_states = new VertexStates()
    const snapshot = config.value.snapshot
    for (const [i, vertex] of snapshot.vertices.entries()) {
        if (vertex == null) {
            continue
        }
        const state = new VertexState(i, config.value.data.visualizer.positions[i])
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
    updateVerticesMatrix(vertex_states, mesh)
    updateVerticesColor(vertex_states, mesh)
    mesh.userData.vecData = vertex_states
}

function updateVerticesMatrix(vertex_states: Array<VertexState>, mesh: any) {
    for (let i = 0; i < vertex_states.length; i++) {
        const state = vertex_states[i]
        const dummy = new Object3D()
        load_position(dummy.position, state.position)
        dummy.updateMatrix()
        mesh.setMatrixAt(i, dummy.matrix)
    }
    mesh.instanceMatrix.needsUpdate = true
}

function updateVerticesColor(vertex_states: Array<VertexState>, mesh: any) {
    for (let i = 0; i < vertex_states.length; i++) {
        const state = vertex_states[i]
        if (state.vi == selected_vertex?.vi) {
            mesh.setColorAt(i, new Color(config.value.basic.selected_color))
        } else if (state.vi == hovered_vertex?.vi) {
            mesh.setColorAt(i, new Color(config.value.basic.hovered_color))
        } else if (mesh.userData.myData) {
            mesh.setColorAt(i, new Color(mesh.userData.myData.color))
        }
    }
    if (mesh.instanceColor) {
        mesh.instanceColor.needsUpdate = true
    }
}

let hovered_vertex: VertexState | undefined = undefined
let selected_vertex: VertexState | undefined = undefined
function intersecting_vertex(intersect: any): VertexState | undefined {
    if (intersect?.instanceId == undefined) {
        return undefined
    }
    const vertex_state: VertexState | undefined = intersect?.object?.userData?.vecData?.[intersect.instanceId]
    if (vertex_state == undefined || vertex_state.type != 'vertex') {
        return undefined
    }
    return vertex_state
}

const normal_vertices_ref = useTemplateRef('normal_vertices')
const defect_vertices_ref = useTemplateRef('defect_vertices')
const vertices_outlines_ref = useTemplateRef('vertices_outlines')

const update_normal_vertices = () => applyMeshVertices(vertex_states.value.normal_vertices, (normal_vertices_ref.value as any).mesh)
const update_defect_vertices = () => applyMeshVertices(vertex_states.value.defect_vertices, (defect_vertices_ref.value as any).mesh)
const update_vertices_outlines = () => applyMeshVertices(vertex_states.value.all_vertices, (vertices_outlines_ref.value as any).mesh)
function update() {
    update_normal_vertices()
    update_defect_vertices()
    update_vertices_outlines()
}
function update_intersect_color() {
    updateVerticesColor(vertex_states.value.normal_vertices, (normal_vertices_ref.value as any).mesh)
    updateVerticesColor(vertex_states.value.defect_vertices, (defect_vertices_ref.value as any).mesh)
    updateVerticesColor(vertex_states.value.all_vertices, (vertices_outlines_ref.value as any).mesh)
}

onMounted(() => {
    // when anything changes, update the mesh
    watchEffect(() => update())
    // update color
    watchEffect(() => {
        // update the color when the hovered or selected state changes
        hovered_vertex = intersecting_vertex(config.value.data.hovered)
        selected_vertex = intersecting_vertex(config.value.data.selected)
        update_intersect_color()
    })
})
</script>

<template>
    <!-- note: color must be set individually, because otherwise ThreeJS will multiply the color values and create strange visual effects -->

    <!-- normal vertices -->
    <MyInstancedMesh ref="normal_vertices" :count="vertex_states.normal_vertices.length" @reinstantiated="update_normal_vertices" :myData="{ color: config.vertex.normal_color }">
        <SphereGeometry :radius="config.vertex.radius" :height-segments="config.basic.segments" :width-segments="config.basic.segments"> </SphereGeometry>
        <PhysicalMaterial :props="{ transparent: false, side: FrontSide }"></PhysicalMaterial>
    </MyInstancedMesh>

    <!-- defect vertices -->
    <MyInstancedMesh ref="defect_vertices" :count="vertex_states.defect_vertices.length" @reinstantiated="update_defect_vertices" :myData="{ color: config.vertex.defect_color }">
        <SphereGeometry :radius="config.vertex.radius" :height-segments="config.basic.segments" :width-segments="config.basic.segments"> </SphereGeometry>
        <PhysicalMaterial :props="{ transparent: false, side: FrontSide }"></PhysicalMaterial>
    </MyInstancedMesh>

    <!-- all vertices outlines -->
    <MyInstancedMesh ref="vertices_outlines" :count="vertex_states.all_vertices.length" @reinstantiated="update_vertices_outlines">
        <SphereGeometry :radius="config.vertex.outline_radius" :height-segments="config.basic.segments" :width-segments="config.basic.segments"> </SphereGeometry>
        <PhysicalMaterial :props="{ transparent: false, side: BackSide, color: '#000000' }"></PhysicalMaterial>
    </MyInstancedMesh>
</template>
