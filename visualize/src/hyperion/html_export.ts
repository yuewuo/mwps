import { assert, bigInt, compress_content } from '@/util'
import HTMLString from '../../index.html?raw'
import type { ConfigProps } from './hyperion'

function begin (name: string): string {
    return `/* ${name}_BEGIN */`
}
function end (name: string): string {
    return `/* ${name}_END */`
}

function slice_content (content: string, name: string): [string, string, string] {
    const begin_flag = begin(name)
    const end_flag = end(name)
    const start_index = content.indexOf(begin_flag)
    assert(start_index != -1, 'start flag not found in content')
    const end_index = content.indexOf(end_flag)
    assert(end_index != -1, 'end flag not found in content')
    assert(start_index + begin_flag.length < end_index, 'start and end flag misplaced in index.html')
    return [content.slice(0, start_index).trim(), content.slice(start_index + begin_flag.length, end_index).trim(), content.slice(end_index + end_flag.length).trim()]
}

const caller_dom = document.getElementById('hyperion_visual_compressed_js_caller')
export const available = caller_dom != undefined

// when `caller_dom` is available, we're in the release mode where the compressed js library is available
// the only difference between index.html and dist/standalone.html (our) is that the library is different
const [script_head, , script_tail] = slice_content(HTMLString, 'HYPERION_VISUAL_MODULE_LOADER')

// then, inside script body, we would want to customize the visualizer data,
const data_flag = 'HYPERION_VISUAL_DATA'
const [vis_data_head, , vis_data_tail] = slice_content(script_head, data_flag)
const override_config_flag = 'HYPERION_VISUAL_OVERRIDE_CONFIG'
const [override_head, , override_tail] = slice_content(vis_data_tail, override_config_flag)

export async function generate_html (visualizer_data: object, compress_data: boolean, override_config: ConfigProps): Promise<string> {
    assert(available, 'no compressed js library available, please run this in release mode')
    const override = { ...override_config, full_screen: true }
    const override_str = bigInt.JavascriptStringify(override, { maxLength: 160, indent: 4 })
    const new_vis_data_tail = override_head + '\n' + begin(override_config_flag) + '\n' + override_str + '\n' + end(override_config_flag) + '\n' + override_tail
    let javascript_data = ''
    if (compress_data) {
        // use gzip to compress the visualizer data, which is supposed to have a high redundancy
        const json = bigInt.JSONStringify(visualizer_data)
        const compressed = await compress_content(new TextEncoder().encode(json))
        javascript_data = `"${compressed}"`
    } else {
        // use uncompressed javascript to store the data, for better readability and ease manual modification
        javascript_data = bigInt.JavascriptStringify(visualizer_data, { maxLength: 160, indent: 4 })
    }
    const new_script_head = vis_data_head + '\n' + begin(data_flag) + '\n' + javascript_data + '\n' + end(data_flag) + '\n' + new_vis_data_tail
    const new_html = new_script_head + caller_dom?.innerText + script_tail

    return new_html
}
