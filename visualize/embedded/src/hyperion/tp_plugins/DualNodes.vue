<script setup lang="ts">
import Indices from './Indices.vue'
import { Info } from '../info_pane'

interface Props {
    info: Info
}

const props = defineProps<Props>()
const config = props.info.config
</script>

<template>
    <div v-for="(dual_node, ni) of config.snapshot.dual_nodes" :key="ni">
        <div class="entry" v-if="props.info.display_zero_dual_variables || dual_node.d != 0">
            <div class="left">
                <div class="color-indicator" :style="{ 'background-color': config.edge.color_palette.get(ni) }"></div>
                <i style="font-size: 20px; position: relative; top: -13px">y</i>
                <i style="font-size: 13px; position: relative; top: -10px">S</i>
                <span style="display: inline-block; font-size: 10px; position: relative; top: -5px; width: 16px">{{ ni }}</span>
                <i style="font-size: 16px; position: relative; top: -11px">&nbsp;=&nbsp;</i>
                <div class="rational">
                    <!-- TODO: optimize display of floating point number: always show full precision and forbid scientific representation (which puts the important number at the very end...) -->
                    <div class="nominator rational-number">{{ dual_node.dn }}</div>
                    <div class="rational-divider"></div>
                    <div class="denominator rational-number">{{ dual_node.dd }}</div>
                </div>
            </div>
            <div class="right">
                <Indices title="<i>V<i/>&nbsp;=" :width="148" :indices="dual_node.v"></Indices>
                <div style="width: 4px; display: inline-block"></div>
                <Indices title="<i>E<i/>&nbsp;=" :width="148" :indices="dual_node.e"></Indices>
                <div style="height: 2px"></div>
                <Indices title="𝛅(<i>S</i>)&nbsp;=" :title-width="40" :indices="dual_node.h"></Indices>
            </div>
        </div>
    </div>
</template>

<style scoped>
.entry {
    font-family: 'Times New Roman', Times, serif;
    color: #bbbcc3;
    display: flex;
    flex-direction: row;
    justify-content: space-between;
    align-items: center;
    margin-right: 3px;
    margin-bottom: 3px;
}
.left {
    position: relative;
    padding-left: 5px;
    width: 100px;
    /* background-color: red; */
}
.right {
    width: 300px;
    /* background-color: blue; */
}
.rational {
    display: inline-block;
    height: 34px;
    width: 36px;
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
.color-indicator {
    position: absolute;
    width: 20px;
    height: 8px;
    border-radius: 5px;
}
</style>
