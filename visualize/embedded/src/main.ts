import { parity_matrix } from './parity_matrix/parity_matrix'
import { hypergraph } from './hypergraph/hypergraph'
import { type VisualizerData, ConfigProps } from '@/hyperion/hyperion'
import Hyperion from '@/hyperion/Hyperion.vue'
import { createApp } from 'vue'

function bind_to_div (div_selector: string, visualizer: VisualizerData, config?: ConfigProps) {
    const app = createApp(Hyperion, { visualizer, config })
    app.mount(div_selector)
}

function default_config (): ConfigProps {
    return new ConfigProps()
}

export const hyperion_visual = {
    parity_matrix,
    hypergraph,

    bind_to_div,
    default_config
}
