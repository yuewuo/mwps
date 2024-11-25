<script setup lang="ts">
interface Props {
    title?: string
    indices: Array<number>
    titleWidth?: number
    width?: number
}
const props = withDefaults(defineProps<Props>(), {
    titleWidth: 30,
    width: 300,
})
</script>

<template>
    <div class="div" :style="{ width: props.width + 'px' }">
        <div class="flex-div">
            <span class="title" v-html="props.title" :style="{ width: props.titleWidth + 'px' }"></span>
            <div class="indices-div" :style="{ width: props.width - props.titleWidth + 'px' }">
                <div class="flex-div">
                    <span class="bracket">{</span>
                    <button v-for="(idx, i) of props.indices" :key="i" class="idx-button">{{ idx }}</button>
                    <span class="bracket">}</span>
                </div>
            </div>
        </div>
    </div>
</template>

<style scoped>
.div {
    display: inline-block;
}
.title {
    text-align: center;
    display: inline-block;
    user-select: none;
}
.title:hover {
    color: #6fdfdf;
}
.title:active {
    color: #4b7be5;
}
.indices-div {
    display: inline-block;
    background-color: #adafb7;
    overflow-x: scroll;
    scrollbar-width: none;
    color: #29292e;
    border-radius: 5px;
    padding: 1px 0 1px 0;
    height: 14px;
}
.bracket {
    line-height: 14px;
    position: relative;
    top: -1px;
}
.flex-div {
    display: flex;
    flex-direction: row;
    align-items: center;
}
.idx-button {
    border: solid black 1px;
    border-radius: 3px;
    height: 14px;
    display: inline;
    font-size: 10px;
    line-height: 10px;
    padding: 0 1px 1px 2px;
    margin: 0 1px 0 1px;
}
.idx-button:hover {
    color: #6fdfdf;
}
.idx-button:active {
    color: #4b7be5;
}
</style>
