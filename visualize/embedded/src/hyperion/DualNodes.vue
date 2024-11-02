<script setup lang="ts">
import { onMounted, onUnmounted, watchEffect } from 'vue'
import { type Config } from './config_pane'

interface Props {
    config: Config
}

const props = defineProps<Props>()
const config = props.config

onMounted(() => {
    console.log('mounted')
})
onUnmounted(() => {
    console.log('unmounted')
})

watchEffect(() => {
    console.log(config.snapshot)
})
</script>

<template>
    <span class="dual-node-s">hello world</span>
    <button>haha</button>
</template>

<style lang="css" scoped>
.dual-node-s {
    color: red;
}
</style>
