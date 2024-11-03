import { createApp } from 'vue'
import HyperGraph from './HyperGraph.vue'

function bind_to_div(div_selector: string) {
    const app = createApp(HyperGraph)
    app.mount(div_selector)
}

export const hypergraph = {
    bind_to_div,
}
