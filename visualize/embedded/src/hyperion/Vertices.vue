<script setup lang="ts">
import { type Ref } from 'vue'
import { type Config } from './hyperion'
import { InstancedMesh, StandardMaterial, SphereGeometry } from 'troisjs'
import { assert_inject } from '@/misc/util';


const config: Ref<Config> = assert_inject('config')

console.log(config.value.snapshot)

</script>

<template>
    <InstancedMesh ref="vertex" :count="100">
        <SphereGeometry :radius="1" :height-segments="config.basic.segments" :width-segments="config.basic.segments">
        </SphereGeometry>
        <StandardMaterial :props="{ transparent: true, opacity: 0.5 }"></StandardMaterial>
    </InstancedMesh>
</template>