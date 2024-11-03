import { assert } from '@/util'
import HTMLString from '../../index.html?raw'

function find_javascript_from_index_html (name: string): [string, string, string] {
    const start_flag = `/* ${name}_BEGIN */`
    const end_flag = `/* ${name}_END */`
    const start_index = HTMLString.indexOf(start_flag)
    assert(start_index != -1, 'start flag not found in index.html')
    const end_index = HTMLString.indexOf(end_flag)
    assert(end_index != -1, 'end flag not found in index.html')
    assert(start_index + start_flag.length < end_index, 'start and end flag misplaced in index.html')
    return [HTMLString.slice(0, start_index).trim(), HTMLString.slice(start_index + start_flag.length, end_index).trim(), HTMLString.slice(end_index + end_flag.length).trim()]
}

const caller_dom = document.getElementById('hyperion_visual_compressed_js_caller')
export const available = caller_dom != undefined

// when `caller_dom` is available, we're in the release mode where the compressed js library is available
// the only difference between index.html and dist/standalone.html (our) is that the library is different
export const [script_head, script_body, script_tail] = find_javascript_from_index_html('HYPERION_VISUAL_MODULE_LOADER')

// then, inside script body, we would want to customize the visualizer data,

export function generate_inline_html (visualizer_data: object): string {
    assert(available, 'no compressed js library available, please run this in release mode')
    console.log(visualizer_data)
    return script_head + caller_dom?.innerText + script_tail
}
