<script setup lang="ts">
import { computed } from 'vue'
import { NButton, NIcon, NTooltip } from 'naive-ui'
import { ParityMatrixData } from './parser'
import { Icon } from '@vicons/utils'
import {
    CodeWorking as CodeIcon,
    SaveSharp as SaveIcon,
    Open as OpenIcon
} from '@vicons/ionicons5'
import { is_compressed_js_available, generate_inline_html } from './page_builder'

const props = defineProps({
    'object': {
        type: Object,
        required: true,
    },
})

const data = computed(() => {
    return new ParityMatrixData(props.object)
})

function is_data_block(ri: number, ci: number): boolean {
    const [rows, columns] = data.value.table.dimension
    if (!data.value.is_echelon_form) {
        return ri > 0 && ri < rows && ci > 0 && ci < columns
    } else {
        return ri > 0 && ri < rows - 1 && ci > 0 && ci < columns - 1
    }
}

function is_rhs(ri: number, ci: number): boolean {
    return is_data_block(ri, ci) && !is_data_block(ri, ci + 1)
}

function download_html() {
    if (!is_compressed_js_available) return
    const html = generate_inline_html(props.object)
    const a = document.createElement("a")
    a.href = window.URL.createObjectURL(new Blob([html], { type: "text/html" }))
    a.download = "parity_matrix.html"
    a.click()
}

function open_in_new_tab() {
    if (!is_compressed_js_available) return
    const html = generate_inline_html(props.object)
    const new_tab = window.open("", '_blank')
    new_tab?.document.write(html)
    new_tab?.document.close()
}

</script>

<template>
    <div class="box">
        <div class="toolbox">
            <n-tooltip placement="bottom" trigger="hover">
                <template #trigger>
                    <n-button strong secondary circle @click="open_in_new_tab" class="button">
                        <template #icon>
                            <n-icon :color="is_compressed_js_available ? 'blue' : 'lightgrey'"><open-icon /></n-icon>
                        </template>
                    </n-button>
                </template>
                <span v-if="!is_compressed_js_available">Open in new Tab (unavailable in debug mode)</span>
                <span v-if="is_compressed_js_available">Open in new Tab</span>
            </n-tooltip>
            <n-tooltip placement="bottom" trigger="hover">
                <template #trigger>
                    <n-button strong secondary circle @click="download_html" class="button">
                        <template #icon>
                            <n-icon :color="is_compressed_js_available ? 'orange' : 'lightgrey'"><save-icon /></n-icon>
                        </template>
                    </n-button>
                </template>
                <span v-if="!is_compressed_js_available">Download (unavailable in debug mode)</span>
                <span v-if="is_compressed_js_available">Download</span>
            </n-tooltip>
            <n-tooltip placement="bottom" trigger="hover">
                <template #trigger>
                    <n-button strong secondary circle class="button">
                        <template #icon>
                            <Icon color="green"><code-icon /></Icon>
                        </template>
                    </n-button>
                </template>
                <span>View Object</span>
            </n-tooltip>
        </div>
        <table>
            <tr v-for="(  row, ri  ) in   data.table.rows  " :key="ri">
                <th v-for="(  element, ci  ) of   row.elements  " :key="ci" :class="{
                    'title': ri == 0, 'square': is_data_block(ri, ci),
                    'line-head': ri != 0 && ci == 0, 'rhs': is_rhs(ri, ci),
                }
                    ">{{ element }}</th>
            </tr>
        </table>
    </div>
</template>

<style scoped>
.box {
    display: inline-block;
    margin: 10px;
    border: 0;
    min-width: 50px;
    min-height: 50px;
}

th {
    border: 1px solid grey;
}

table,
th,
tr {
    text-align: center;
    border-collapse: collapse;
    font-size: 14px;
    padding: 0;
    white-space: pre;
    line-height: 14px;
    color: black;
    background-color: white;
}

.square {
    font-size: 18px;
    font-family: monospace;
    width: 22px;
    height: 22px;
}

.title {
    /* color: red; */
    background-color: lightblue;
}

.line-head {
    background-color: lightpink;
}

.rhs {
    background-color: lightsalmon;
}

.toolbox {
    display: flex;
    justify-content: right;
    align-items: right;
    margin-bottom: 3px;
}

.button {
    margin: 3px;
}
</style>
