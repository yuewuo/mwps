<script setup lang="ts">
/**
 * The hyperedge is displayed based on its degree.
 * 1. degree-1 edges are displayed as a 2D ring with a single vertex at the center.
 * 2. degree-(2+) edges are displayed as tubes from all its incident vertices to the center.
 *      The tube is colored based on the Cover of the dual variables, starting from vertices $\delta(S) \cap e$
 *      and gradually growing towards the center and further to other vertices $e \setminus \delta(S)$.
 *
 * InstancedMesh cannot be used across objects with different opacities, and thus we need to create different objects
 * for different opacities. For simplicity, we only have 3 opacities: 0.1 (empty), 0.2 (partial grown), and 1 (tight).
 */

import { type Ref, computed, watchEffect, useTemplateRef, onMounted } from 'vue'
import {
    compute_vector3,
    unit_up_vector,
    EdgeRingState,
    EdgeTubeState,
    EdgeStates,
    ring_resolution,
    compute_edge_to_dual_indices,
    type EdgeBranchSegments,
    calculate_edge_branch_segmented,
} from './hyperion'
import { type Config } from './config_pane'
import { PhysicalMaterial, CylinderGeometry, RingGeometry } from 'troisjs'
import MyInstancedMesh from '@/misc/MyInstancedMesh.vue'
import { Object3D, BackSide, DoubleSide, Color, Vector3, Quaternion } from 'three'
import { assert_inject } from '@/misc/util'
import { useEdgesStore } from './store'

const counter = useEdgesStore() // TODO
counter.increment()
console.log(counter.count)

const config: Ref<Config> = assert_inject('config')
const ring_index_of_ratio = (ratio: number) => Math.max(0, Math.min(ring_resolution, Math.round(ratio * ring_resolution)))

const edge_states = computed(() => {
    const edge_states = new EdgeStates()
    const snapshot = config.value.snapshot
    for (const [i, edge] of snapshot.edges.entries()) {
        if (edge == null) {
            continue
        }
        // calculate the center point of all vertices
        let sum_position = new Vector3(0, 0, 0)
        for (let j = 0; j < edge.v.length; ++j) {
            const vertex_index = edge.v[j]
            const vertex_position = config.value.data.visualizer.positions[vertex_index]
            sum_position = sum_position.add(compute_vector3(vertex_position))
        }
        const center_position = sum_position.multiplyScalar(1 / edge.v.length)
        const is_tight = edge.u == 0
        for (let j = 0; j < edge.v.length; ++j) {
            const vertex_index = edge.v[j]
            const vertex_position = config.value.data.visualizer.positions[vertex_index]
            const relative = center_position.clone().add(compute_vector3(vertex_position).multiplyScalar(-1))
            const direction = relative.clone().normalize()
            const quaternion = new Quaternion()
            quaternion.setFromUnitVectors(unit_up_vector, direction)
            let start = edge_offset.value
            const distance = relative.length()
            let edge_length = distance - edge_offset.value
            if (edge_length < 0) {
                // edge length should be non-negative
                start = distance
                edge_length = 0
            }
            const end = start + edge_length
            let start_position = compute_vector3(vertex_position).add(relative.clone().multiplyScalar(start / distance))
            let end_position = compute_vector3(vertex_position).add(relative.clone().multiplyScalar(end / distance))
            if (edge.v.length == 1) {
                start_position = compute_vector3(vertex_position)
                end_position = compute_vector3(vertex_position)
            }
            const segment_position_of = (ratio: number) => {
                // 0: start, 1: end
                return start_position
                    .clone()
                    .multiplyScalar(1 - ratio)
                    .add(end_position.clone().multiplyScalar(ratio))
            }
            const edge_branch_segmented_data = edge_branch_segments.value[i]
            const grown_end = edge_branch_segmented_data.grown_end[j]
            const grown_center = edge_branch_segmented_data.grown_center[j]
            const segments_center = edge_branch_segmented_data.contributor_center[j]
            const segments_end = edge_branch_segmented_data.contributor_end[j]
            // calculate the segments of this edge branch
            let accumulated_ratio = 0
            const branch_weight = Math.abs(edge.w) / edge.v.length
            interface Segment {
                ni: number | null
                accumulated_ratio: number
                segment_ratio: number
            }
            const segments: Array<Segment> = []
            //     growing from end vertices
            for (const [ni, length] of segments_end) {
                const segment_ratio = length / branch_weight
                segments.push({ ni, accumulated_ratio, segment_ratio })
                accumulated_ratio += segment_ratio
            }
            //     the middle empty segment
            if (grown_end + grown_center < branch_weight) {
                const segment_ratio = (branch_weight - grown_end - grown_center) / branch_weight
                segments.push({ ni: null, accumulated_ratio, segment_ratio })
                accumulated_ratio += segment_ratio
            }
            //     growing from center vertices
            for (let index = segments_center.length - 1; index >= 0; index--) {
                const [ni, length] = segments_center[index]
                const segment_ratio = length / branch_weight
                segments.push({ ni, accumulated_ratio, segment_ratio })
                accumulated_ratio += segment_ratio
            }
            // create the segments
            for (const { ni, accumulated_ratio, segment_ratio } of segments) {
                if (edge.v.length != 1) {
                    const state = new EdgeTubeState(i, segment_position_of(accumulated_ratio), edge_length * segment_ratio, edge.v.length, quaternion)
                    if (subgraph_set.value[i]) {
                        // display the solid blue edge as subgraph
                        state.color = config.value.edge.color_palette.subgraph
                        edge_states.tight_edge_tubes.push(state)
                    } else if (snapshot.subgraph != null) {
                        // do not display anything if the intention of this snapshot is to display the subgraph
                        state.color = config.value.edge.color_palette.ungrown
                        edge_states.ungrown_edge_tubes.push(state)
                    } else if (ni != null) {
                        state.color = config.value.edge.color_palette.get(ni)
                        if (is_tight) {
                            edge_states.tight_edge_tubes.push(state)
                        } else {
                            edge_states.grown_edge_tubes.push(state)
                        }
                    } else {
                        state.color = config.value.edge.color_palette.ungrown
                        edge_states.ungrown_edge_tubes.push(state)
                    }
                } else {
                    const outline_ratio = config.value.vertex.outline_ratio
                    const func = (ratio: number) => outline_ratio + (config.value.edge.deg_1_ratio - 1) * outline_ratio * ratio
                    const state = new EdgeRingState(i, segment_position_of(accumulated_ratio))
                    state.inner = func(accumulated_ratio)
                    state.outer = func(segment_ratio + accumulated_ratio)
                    const ring_index = ring_index_of_ratio(state.inner / state.outer)
                    if (subgraph_set.value[i]) {
                        // display the solid blue edge as subgraph
                        state.color = config.value.edge.color_palette.subgraph
                        edge_states.tight_edge_rings[ring_index].push(state)
                    } else if (snapshot.subgraph != null) {
                        // do not display anything if the intention of this snapshot is to display the subgraph
                        state.color = config.value.edge.color_palette.ungrown
                        edge_states.ungrown_edge_rings[ring_index].push(state)
                    } else if (ni != null) {
                        state.color = config.value.edge.color_palette.get(ni)
                        if (is_tight) {
                            edge_states.tight_edge_rings[ring_index].push(state)
                        } else {
                            edge_states.grown_edge_rings[ring_index].push(state)
                        }
                    } else {
                        state.color = config.value.edge.color_palette.ungrown
                        edge_states.ungrown_edge_rings[ring_index].push(state)
                    }
                }
            }
        }
    }
    return edge_states
})

function applyMeshEdgeRings(singular_edges: Array<Array<EdgeRingState>>, refs: any) {
    updateEdgeRingsMatrix(singular_edges, refs)
    updateEdgeRingsColor(singular_edges, refs)
    for (let i = 0; i < refs.length; i++) {
        const mesh = refs[i].mesh
        mesh.userData.vecData = singular_edges[i]
    }
}

function updateEdgeRingsMatrix(singular_edges: Array<Array<EdgeRingState>>, refs: any) {
    for (let i = 0; i < refs.length; i++) {
        const mesh = refs[i].mesh
        const edges = singular_edges[i]
        for (let j = 0; j < edges.length; j++) {
            const state = edges[j]
            const dummy = new Object3D()
            dummy.rotateX(Math.PI / 2)
            const radius_ratio = config.value.vertex.radius * state.outer
            dummy.scale.set(radius_ratio, radius_ratio, 1)
            dummy.position.copy(state.position.clone())
            dummy.updateMatrix()
            mesh.setMatrixAt(j, dummy.matrix)
        }
        mesh.instanceMatrix.needsUpdate = true
    }
}

function updateEdgeRingsColor(singular_edges: Array<Array<EdgeRingState>>, refs: any) {
    for (let i = 0; i < refs.length; i++) {
        const mesh = refs[i].mesh
        const edges = singular_edges[i]
        for (let j = 0; j < edges.length; j++) {
            const state = edges[j]
            if (state.ei == selected_edge?.ei) {
                mesh.setColorAt(j, new Color(config.value.basic.selected_color))
            } else if (state.ei == hovered_edge?.ei) {
                mesh.setColorAt(j, new Color(config.value.basic.hovered_color))
            } else {
                mesh.setColorAt(j, new Color(state.color))
            }
        }
        if (mesh.instanceColor) {
            mesh.instanceColor.needsUpdate = true
        }
    }
}

function applyMeshEdgeTubes(edges: Array<EdgeTubeState>, mesh: any) {
    updateEdgeTubesMatrix(edges, mesh)
    updateEdgeTubesColor(edges, mesh)
    mesh.userData.vecData = edges
}

function updateEdgeTubesMatrix(edges: Array<EdgeTubeState>, mesh: any) {
    for (let i = 0; i < edges.length; i++) {
        const state = edges[i]
        const dummy = new Object3D()
        const radius_ratio = config.value.edge.ratio_of_deg(state.degree)
        dummy.scale.set(radius_ratio, state.length, radius_ratio)
        dummy.setRotationFromQuaternion(state.quaternion)
        dummy.position.copy(state.position.clone())
        dummy.translateY(state.length / 2)
        dummy.updateMatrix()
        mesh.setMatrixAt(i, dummy.matrix)
    }
    mesh.instanceMatrix.needsUpdate = true
}

function updateEdgeTubesColor(edges: Array<EdgeTubeState>, mesh: any) {
    for (let i = 0; i < edges.length; i++) {
        const state = edges[i]
        if (state.ei == selected_edge?.ei) {
            mesh.setColorAt(i, new Color(config.value.basic.selected_color))
        } else if (state.ei == hovered_edge?.ei) {
            mesh.setColorAt(i, new Color(config.value.basic.hovered_color))
        } else {
            mesh.setColorAt(i, new Color(state.color))
        }
    }
    if (mesh.instanceColor) {
        mesh.instanceColor.needsUpdate = true
    }
}

const ungrown_edge_rings_ref = useTemplateRef('ungrown_edge_rings')
const grown_edge_rings_ref = useTemplateRef('grown_edge_rings')
const tight_edge_rings_ref = useTemplateRef('tight_edge_rings')
const ungrown_edge_tubes_ref = useTemplateRef('ungrown_edge_tubes')
const grown_edge_tubes_ref = useTemplateRef('grown_edge_tubes')
const tight_edge_tubes_ref = useTemplateRef('tight_edge_tubes')

const update_ungrown_edge_rings = () => applyMeshEdgeRings(edge_states.value.ungrown_edge_rings, ungrown_edge_rings_ref.value)
const update_grown_edge_rings = () => applyMeshEdgeRings(edge_states.value.grown_edge_rings, grown_edge_rings_ref.value)
const update_tight_edge_rings = () => applyMeshEdgeRings(edge_states.value.tight_edge_rings, tight_edge_rings_ref.value)
const update_ungrown_edge_tubes = () => applyMeshEdgeTubes(edge_states.value.ungrown_edge_tubes, (ungrown_edge_tubes_ref.value as any).mesh)
const update_grown_edge_tubes = () => applyMeshEdgeTubes(edge_states.value.grown_edge_tubes, (grown_edge_tubes_ref.value as any).mesh)
const update_tight_edge_tubes = () => applyMeshEdgeTubes(edge_states.value.tight_edge_tubes, (tight_edge_tubes_ref.value as any).mesh)
function update() {
    update_ungrown_edge_rings()
    update_grown_edge_rings()
    update_tight_edge_rings()
    update_ungrown_edge_tubes()
    update_grown_edge_tubes()
    update_tight_edge_tubes()
}
function update_intersect_color() {
    updateEdgeRingsColor(edge_states.value.ungrown_edge_rings, ungrown_edge_rings_ref.value)
    updateEdgeRingsColor(edge_states.value.grown_edge_rings, grown_edge_rings_ref.value)
    updateEdgeRingsColor(edge_states.value.tight_edge_rings, tight_edge_rings_ref.value)
    updateEdgeTubesColor(edge_states.value.ungrown_edge_tubes, (ungrown_edge_tubes_ref.value as any).mesh)
    updateEdgeTubesColor(edge_states.value.grown_edge_tubes, (grown_edge_tubes_ref.value as any).mesh)
    updateEdgeTubesColor(edge_states.value.tight_edge_tubes, (tight_edge_tubes_ref.value as any).mesh)
}

let hovered_edge: EdgeRingState | EdgeTubeState | undefined = undefined
let selected_edge: EdgeRingState | EdgeTubeState | undefined = undefined
function intersecting_edge(intersect: any): EdgeRingState | EdgeTubeState | undefined {
    if (intersect?.instanceId == undefined) {
        return undefined
    }
    const edge_state: EdgeRingState | EdgeTubeState | undefined = intersect?.object?.userData?.vecData?.[intersect.instanceId]
    if (edge_state == undefined || edge_state.type != 'edge') {
        return undefined
    }
    return edge_state
}

onMounted(() => {
    // when anything changes, update the mesh
    watchEffect(() => update())
    // update color
    watchEffect(() => {
        // update the color when the hovered or selected state changes
        hovered_edge = intersecting_edge(config.value.data.hovered)
        selected_edge = intersecting_edge(config.value.data.selected)
        update_intersect_color()
    })
})

const subgraph_set = computed(() => {
    const snapshot = config.value.snapshot
    let subgraph_set: { [edge_index: number]: boolean } = {}
    if (snapshot.subgraph != null) {
        for (let edge_index of snapshot.subgraph) {
            subgraph_set[edge_index] = true
        }
    }
    return subgraph_set
})

// calculate the edge offset because of the vertex radius
const edge_offset = computed(() => {
    if (config.value.edge.radius < config.value.vertex.outline_radius) {
        return Math.sqrt(Math.pow(config.value.vertex.outline_radius, 2) - Math.pow(config.value.edge.radius, 2))
    }
    return 0
})

// calculate the dual variable indices for each edge
const edge_to_dual_indices = computed(() => {
    const snapshot = config.value.snapshot
    return compute_edge_to_dual_indices(snapshot)
})

const edge_branch_segments = computed(() => {
    const snapshot = config.value.snapshot
    let edge_branch_segments: Array<EdgeBranchSegments> = []
    for (let edge_index = 0; edge_index < snapshot.edges.length; ++edge_index) {
        edge_branch_segments.push(calculate_edge_branch_segmented(snapshot, edge_to_dual_indices.value, edge_index))
    }
    return edge_branch_segments
})
</script>

<template>
    <!-- ungrown edge rings -->
    <MyInstancedMesh
        v-for="(_, idx) in ring_resolution + 1"
        :key="idx"
        ref="ungrown_edge_rings"
        :count="edge_states.ungrown_edge_rings[idx].length"
        @reinstantiated="update_ungrown_edge_rings"
    >
        <RingGeometry :inner-radius="idx / ring_resolution" :outer-radius="1" :theta-segments="config.basic.segments"> </RingGeometry>
        <PhysicalMaterial :props="{ transparent: true, opacity: config.edge.ungrown_opacity, side: DoubleSide }"></PhysicalMaterial>
    </MyInstancedMesh>

    <!-- partially grown edge rings -->
    <MyInstancedMesh
        v-for="(_, idx) in ring_resolution + 1"
        :key="idx"
        ref="grown_edge_rings"
        :count="edge_states.grown_edge_rings[idx].length"
        @reinstantiated="update_grown_edge_rings"
    >
        <RingGeometry :inner-radius="idx / ring_resolution" :outer-radius="1" :theta-segments="config.basic.segments"> </RingGeometry>
        <PhysicalMaterial :props="{ transparent: true, opacity: config.edge.grown_opacity, side: DoubleSide }"></PhysicalMaterial>
    </MyInstancedMesh>

    <!-- tight edge rings -->
    <MyInstancedMesh
        v-for="(_, idx) in ring_resolution + 1"
        :key="idx"
        ref="tight_edge_rings"
        :count="edge_states.tight_edge_rings[idx].length"
        @reinstantiated="update_tight_edge_rings"
    >
        <RingGeometry :inner-radius="idx / ring_resolution" :outer-radius="1" :theta-segments="config.basic.segments"> </RingGeometry>
        <PhysicalMaterial :props="{ transparent: true, opacity: config.edge.tight_opacity, side: DoubleSide }"></PhysicalMaterial>
    </MyInstancedMesh>

    <!-- ungrown edge tubes -->
    <MyInstancedMesh ref="ungrown_edge_tubes" :count="edge_states.ungrown_edge_tubes.length" @reinstantiated="update_ungrown_edge_tubes">
        <CylinderGeometry
            :radius-top="config.edge.radius"
            :radius-bottom="config.edge.radius"
            :height="1"
            :height-segments="1"
            :radial-segments="config.basic.segments"
            :open-ended="true"
        >
        </CylinderGeometry>
        <PhysicalMaterial :props="{ transparent: true, opacity: config.edge.ungrown_opacity, side: BackSide }"></PhysicalMaterial>
    </MyInstancedMesh>

    <!-- partially grown edge tubes -->
    <MyInstancedMesh ref="grown_edge_tubes" :count="edge_states.grown_edge_tubes.length" @reinstantiated="update_grown_edge_tubes">
        <CylinderGeometry
            :radius-top="config.edge.radius"
            :radius-bottom="config.edge.radius"
            :height="1"
            :height-segments="1"
            :radial-segments="config.basic.segments"
            :open-ended="true"
        >
        </CylinderGeometry>
        <PhysicalMaterial :props="{ transparent: true, opacity: config.edge.grown_opacity, side: BackSide }"></PhysicalMaterial>
    </MyInstancedMesh>

    <!--tight edge tubes -->
    <MyInstancedMesh ref="tight_edge_tubes" :count="edge_states.tight_edge_tubes.length" @reinstantiated="update_tight_edge_tubes">
        <CylinderGeometry
            :radius-top="config.edge.radius"
            :radius-bottom="config.edge.radius"
            :height="1"
            :height-segments="1"
            :radial-segments="config.basic.segments"
            :open-ended="true"
        >
        </CylinderGeometry>
        <PhysicalMaterial :props="{ transparent: true, opacity: config.edge.tight_opacity, side: DoubleSide }"></PhysicalMaterial>
    </MyInstancedMesh>
</template>
