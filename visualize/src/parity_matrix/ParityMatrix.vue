<script setup lang="ts">
import { computed } from 'vue'
// import { NButton, NIcon, NTooltip } from 'naive-ui'
import { ParityMatrixData } from './parser'
// import { Icon } from '@vicons/utils'
// import { CodeWorking as CodeIcon, SaveSharp as SaveIcon, Open as OpenIcon } from '@vicons/ionicons5'

const props = defineProps({
    object: {
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

function is_line_head(ri: number, ci: number): boolean {
    const [rows] = data.value.table.dimension
    return ri != 0 && ci == 0 && !(data.value.is_echelon_form && ri == rows - 1)
}

function is_echelon_info_row(ri: number, _ci: number): boolean {
    const [rows] = data.value.table.dimension
    return data.value.is_echelon_form && ri == rows - 1
}

function is_echelon_info_col(_ri: number, ci: number): boolean {
    const [, columns] = data.value.table.dimension
    return data.value.is_echelon_form && ci == columns - 1
}

function block_content(ri: number, ci: number): string {
    const element = data.value.table.rows[ri].elements[ci]
    if (is_rhs(ri, ci)) {
        // the default output includes extra space to have a width of three, so to distinguish with others
        // we don't really need it here because the HTML can have different colors
        return element.trim()
    }
    return element
}

function is_corner_block(ri: number, ci: number): boolean {
    if (!is_data_block(ri, ci) || is_rhs(ri, ci)) {
        return false
    }
    const tail_start_index = data.value.tail_start_index
    const corner_row_index = data.value.corner_row_index
    if (tail_start_index != null && corner_row_index != null && ri > corner_row_index && ci > tail_start_index) {
        return true
    }
    return false
}

function is_tail_columns(ri: number, ci: number): boolean {
    if (!is_data_block(ri, ci) || is_rhs(ri, ci) || is_corner_block(ri, ci)) {
        return false
    }
    const tail_start_index = data.value.tail_start_index
    if (tail_start_index != null && ci > tail_start_index) {
        return true
    }
    return false
}
</script>

<template>
    <div class="box">
        <div class="toolbox"></div>
        <table>
            <tr v-for="(row, ri) in data.table.rows" :key="ri">
                <th
                    v-for="(element, ci) of row.elements"
                    :key="ci"
                    :class="{
                        title: ri == 0 && !is_echelon_info_col(ri, ci),
                        square: is_data_block(ri, ci),
                        'line-head': is_line_head(ri, ci),
                        rhs: is_rhs(ri, ci),
                        'echelon-info-row': is_echelon_info_row(ri, ci),
                        'echelon-info-col': is_echelon_info_col(ri, ci),
                        'tail-columns': is_tail_columns(ri, ci),
                        'corner-block': is_corner_block(ri, ci),
                    }"
                >
                    {{ block_content(ri, ci) }}
                </th>
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
    min-width: 22px;
    text-align: center !important;
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

.echelon-info-row {
    color: lightgrey;
    font-size: 80%;
}

.echelon-info-col {
    color: lightgrey;
    font-size: 80%;
}

.tail-columns {
    background-color: lightcyan;
}

.corner-block {
    background-color: lightgreen;
}
</style>
