import { parity_matrix } from './parity_matrix/parity_matrix'
import { hypergraph } from './hypergraph/hypergraph'
import { type VisualizerData } from '@/hyperion/hyperion'
import Hyperion from '@/hyperion/Hyperion.vue'
import { createApp } from 'vue'

function bind_to_div(div_selector: string, visualizer: VisualizerData, hide_config: boolean, full_screen: boolean) {
    const app = createApp(Hyperion, {visualizer, hide_config, full_screen})
    app.mount(div_selector)
}

export const hyperion_visual = {
    parity_matrix,
    hypergraph,
    bind_to_div,
}
