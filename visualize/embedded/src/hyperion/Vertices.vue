<script setup lang="ts">
import { type Ref } from 'vue'
import { type Config } from './hyperion'
import { assert_inject } from '@/misc/util';


const config: Ref<Config> = assert_inject('config')

</script>

<template>
    <InstancedMesh ref="vertex" :count="10"></InstancedMesh>
</template>