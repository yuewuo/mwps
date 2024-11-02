<script setup lang="ts">
import { onMounted } from 'vue'
import { onUnmounted } from 'vue'

onMounted(() => {
    console.log('mounted')
})
onUnmounted(() => {
    console.log('unmounted')
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
