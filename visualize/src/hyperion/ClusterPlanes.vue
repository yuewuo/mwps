<script setup lang="ts">
import { type Ref, computed, watchEffect, useTemplateRef, onMounted } from 'vue'
import { computedAsync } from '@vueuse/core'
import { VertexState, VertexStates, load_position } from './hyperion'
import { type Config } from './config_pane'
import { PhysicalMaterial, SphereGeometry } from 'troisjs'
import MyInstancedMesh from '@/misc/MyInstancedMesh.vue'
import { Object3D, FrontSide, BackSide, Color } from 'three'
import { assert_inject } from '@/misc/util'

const config: Ref<Config> = assert_inject('config')

const cluster_states = computedAsync(
    async () => {
        const snapshot = config.value.snapshot

        console.log('hellow')
        return 10
    },
    null,
    { lazy: true },
)
</script>

<template>
    <div v-for="i in cluster_states">
        <MyInstancedMesh ref="planes" :count="1">
            <SphereGeometry :radius="config.vertex.radius" :height-segments="config.basic.segments" :width-segments="config.basic.segments"> </SphereGeometry>
            <PhysicalMaterial :props="{ transparent: false, side: FrontSide }"></PhysicalMaterial>
        </MyInstancedMesh>
    </div>
</template>
