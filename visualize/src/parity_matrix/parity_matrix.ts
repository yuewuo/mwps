import { createApp } from 'vue'
import ParityMatrix from './ParityMatrix.vue'
import { ParityMatrixData } from './parser'

function bind_to_div (div_selector: string, object: object) {
    const app = createApp(ParityMatrix, { object })
    app.mount(div_selector)
}

export const parity_matrix = {
    bind_to_div,
    ParityMatrixData: ParityMatrixData
}
