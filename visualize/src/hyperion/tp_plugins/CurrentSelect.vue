<script setup lang="ts">
import { computed } from 'vue'
import { Info } from '../info_pane'
import { type EdgeRingState, type EdgeTubeState, type VertexState, compute_edge_to_dual_indices } from '../hyperion'
import Indices from './Indices.vue'
import EdgeDualSum from '../equations/EdgeDualSum.vue'
import { display_nominator } from '@/util'

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

const edge = computed(() => {
    if (ei.value == undefined) return
    return config.snapshot.edges[ei.value]
})

const edge_to_dual_indices = computed(() => {
    const snapshot = config.snapshot
    return compute_edge_to_dual_indices(snapshot)
})

const edge_contributing_nodes = computed(() => {
    if (ei.value == undefined) return
    let dual_indices = []
    if (config.snapshot.dual_nodes != null) {
        // check the non-zero contributing dual variables
        for (let node_index of edge_to_dual_indices.value[ei.value]) {
            if (config.snapshot.dual_nodes[node_index].d != 0) {
                dual_indices.push(node_index)
            }
        }
    }
    return dual_indices
})
</script>

<template>
    <div class="div">
        <div v-if="vi != undefined">
            <div class="title">{{ config.snapshot.vertices[vi]?.s ? 'Defect' : 'Normal' }} Vertex {{ vi }}</div>
        </div>
        <div v-if="ei != undefined">
            <div class="title">{{ config.snapshot.edges[ei].g >= config.snapshot.edges[ei].w ? 'Tight' : 'Loose' }} Edge {{ ei }}</div>
            <div style="margin-top: 10px">
                <math display="inline" style="font-size: 150%; position: relative; top: 3px">
                    <mi>V</mi>
                    <mn>(</mn>
                    <mi>e</mi>
                    <mn>)</mn>
                    <mo>=</mo>
                </math>
                <Indices :titleWidth="0" :width="335" :indices="edge!.v"></Indices>
            </div>
            <div style="margin-top: 10px">
                <math display="inline-block" style="font-size: 150%; math-style: normal">
                    <msub>
                        <mi>w</mi>
                        <mi>e</mi>
                    </msub>
                    <mo>=</mo>
                    <mn>{{ edge!.w }}</mn>
                </math>
            </div>
            <div style="margin-top: 0px">
                <math display="inline-block" style="font-size: 120%; math-style: normal; position: relative; top: 3px">
                    <mrow>
                        <mo>{</mo>
                        <mi>S</mi>
                        <mo>|</mo>
                        <mi>S</mi>
                        <mn>∈</mn>
                        <mi>O</mi>
                        <mn>,</mn>
                        <mi>e</mi>
                        <mn>∈</mn>
                        <mi>δ</mi>
                        <mn>(</mn>
                        <mi>S</mi>
                        <mn>)</mn>
                        <mo>}</mo>
                    </mrow>
                    <mo>=</mo>
                </math>
                <Indices :titleWidth="0" :width="267" :indices="edge_contributing_nodes!" style="margin-top: 10px"></Indices>
            </div>
            <div style="margin-top: 10px">
                <math display="inline-block" style="font-size: 150%; math-style: compact; position: relative; top: -12px">
                    <EdgeDualSum />
                    <mo>=</mo>
                </math>
                <div class="rational">
                    <div class="nominator rational-number">{{ display_nominator(edge!.gn) }}</div>
                    <div class="rational-divider"></div>
                    <div class="denominator rational-number">{{ edge!.gd }}</div>
                </div>
                <math display="inline-block" style="font-size: 150%; math-style: compact; position: relative; top: -12px">
                    <mo>=</mo>
                    <mn>{{ edge!.g }}</mn>
                </math>
            </div>
            <div style="margin-top: 0px">
                <math display="inline-block" style="font-size: 150%; math-style: compact; position: relative; top: -12px">
                    <msub>
                        <mi>w</mi>
                        <mi>e</mi>
                    </msub>
                    <mo>-</mo>
                    <EdgeDualSum />
                    <mo>=</mo>
                </math>
                <div class="rational">
                    <div class="nominator rational-number">{{ display_nominator(edge!.un) }}</div>
                    <div class="rational-divider"></div>
                    <div class="denominator rational-number">{{ edge!.ud }}</div>
                </div>
                <math display="inline-block" style="font-size: 150%; math-style: compact; position: relative; top: -12px">
                    <mo>=</mo>
                    <mn>{{ edge!.u }}</mn>
                </math>
            </div>
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
.rational {
    display: inline-block;
    height: 34px;
    width: 80px;
}
.rational-divider {
    height: 0.5px;
    background-color: white;
}
.nominator {
    height: 13px;
    padding-top: 3px;
}
.denominator {
    height: 14px;
    padding-top: 2px;
}
.rational-number {
    text-align: center;
    overflow-y: scroll;
    scrollbar-width: none;
    scroll-padding: 0;
}
</style>
