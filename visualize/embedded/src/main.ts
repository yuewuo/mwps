import { parity_matrix } from './parity_matrix/parity_matrix'
import { hypergraph } from './hypergraph/hypergraph'
import { type VisualizerData, ConfigProps } from '@/hyperion/hyperion'
import Hyperion from '@/hyperion/Hyperion.vue'
import { createApp } from 'vue'
import { bigInt, decompress_content } from '@/util'

async function bind_to_div (div_selector: string, visualizer: VisualizerData | string, config?: ConfigProps) {
    if (typeof visualizer === 'string') {
        const decompressed = await decompress_content(visualizer)
        const text_decoder = new TextDecoder('utf-8')
        const json_str = text_decoder.decode(decompressed)
        visualizer = bigInt.JSONParse(json_str) as VisualizerData
    }
    const app = createApp(Hyperion, { visualizer, config })
    app.mount(div_selector)
}

function default_config (): ConfigProps {
    return new ConfigProps()
}

export const hyperion_visual = {
    parity_matrix,
    hypergraph,

    bigInt,
    bind_to_div,
    default_config,
}
