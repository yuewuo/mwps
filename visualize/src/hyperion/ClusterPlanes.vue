<script setup lang="ts">
import { type Ref, computed, watchEffect, useTemplateRef, onMounted } from 'vue'
import { computedAsync } from '@vueuse/core'
import { VertexState, VertexStates, load_position } from './hyperion'
import { type Config } from './config_pane'
import { PhysicalMaterial, SphereGeometry } from 'troisjs'
import MyInstancedMesh from '@/misc/MyInstancedMesh.vue'
import { Object3D, FrontSide, BackSide, Color } from 'three'
import { assert_inject } from '@/misc/util'
import { useEdgesStore } from './store'

const counter = useEdgesStore() // TODO
counter.increment()
console.log(counter.count)

const config: Ref<Config> = assert_inject('config')

/*
 * There are two types of clusters:
 *    1. UF/MWPM clusters: in this setting, a "blossom" is a cluster. One can imagine that UF is equivalent to a modified
 *       blossom algorithm (MWPM) where an alternating tree is always shrunk into a blossom regardless. In this case, we
 *       would like to visualize the clusters based on the connected vertices and edges that belong to the same node.
 *    2. MWPF clusters: in the general MWPF algorithm, a cluster is any connected region connected by any node.
 *
 * For simplicity, we will use the MWPF cluster definition because it's more general. Although it won't be able to
 * distinguish between different "blossoms", in practice we do not really care much about it.
 */

const cluster_states = computedAsync(async () => {
    const snapshot = config.value.snapshot
    // first find the clusters given the edge constructions
    console.log(snapshot)

    return 10
}, null)
</script>

<template>
    <div v-for="i in cluster_states">
        <MyInstancedMesh ref="planes" :count="1">
            <SphereGeometry :radius="config.vertex.radius" :height-segments="config.basic.segments" :width-segments="config.basic.segments"> </SphereGeometry>
            <PhysicalMaterial :props="{ transparent: false, side: FrontSide }"></PhysicalMaterial>
        </MyInstancedMesh>
    </div>
</template>
