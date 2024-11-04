<script setup lang="ts">
import { computed } from 'vue'
import { Info } from '../info_pane'
import type { EdgeRingState, EdgeTubeState, VertexState } from '../hyperion'

interface Props {
    info: Info
}

const props = defineProps<Props>()
const config = props.info.config

const vi = computed(() => {
    const instanceId = config.data.selected?.instanceId
    if (instanceId == undefined) return
    const vertex_state = config.data.selected?.object?.userData?.vecData?.[instanceId]
    if (vertex_state?.type != 'vertex') return
    return (vertex_state as VertexState).vi
})

const ei = computed(() => {
    const instanceId = config.data.selected?.instanceId
    if (instanceId == undefined) return
    const edge_state = config.data.selected?.object?.userData?.vecData?.[instanceId]
    if (edge_state?.type != 'edge') return
    return (edge_state as EdgeTubeState | EdgeRingState).ei
})
</script>

<template>
    <div class="div">
        <div v-if="vi != undefined">
            <div class="title">{{ config.snapshot.vertices[vi]?.s ? 'Defect' : 'Normal' }} Vertex {{ vi }}</div>
        </div>
        <div v-if="ei != undefined">
            <div class="title">{{ config.snapshot.edges[ei].g >= config.snapshot.edges[ei].w ? 'Tight' : 'Loose' }} Edge {{ ei }}</div>
        </div>
    </div>
</template>

<style scoped>
.div {
    color: #bbbcc3;
}
.title {
    font-size: 120%;
    text-align: center;
}
</style>
