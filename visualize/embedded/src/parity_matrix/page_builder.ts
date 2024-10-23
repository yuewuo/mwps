/*
 * Build an html that can be saved
 */

import { assert } from "@/util"

const caller_dom = document.getElementById("hyperion_visual_compressed_js_caller")
const library_dom = document.getElementById("hyperion_visual_compressed_js_library")

export enum Mode {
    Debug,  // without any compressed source
    Inline,  // with inline source
    Linked,  // linking to a compressed source
}

let mode = Mode.Debug
if (caller_dom != null) {
    if (library_dom != null) {
        mode = Mode.Linked
    } else {
        mode = Mode.Inline
    }
}

export const is_compressed_js_available = mode != Mode.Debug

export function generate_inline_html(matrix_data: Object): string {
    assert(is_compressed_js_available, "no compressed js library available")
    // regardless of whether it's linked or inline
    return prefix + JSON.stringify(matrix_data) + after_data
        + caller_script_head + caller_dom?.innerText + "</script>"
        + suffix
}

const prefix = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Parity Matrix</title>
</head>
<body>
  <div id="app"></div>
  <script>
    let display_matrix = `

const after_data = `
    function main() {
      globalThis.hyperion_visual.parity_matrix.bind_to_div("#app", display_matrix)
    }
    function sleep(ms) {
      return new Promise(resolve => setTimeout(resolve, ms));
    }
    async function wait_library() {
      for (let i = 0; i < 10; ++i) {
        if (globalThis.hyperion_visual != null) {
          break
        }
        if (i != 0) console.log(\`window.hyperion_visual not ready, tried \${i} times\`)
        await sleep(100)
      }
      if (globalThis.hyperion_visual != null) {
        main()
      } else {
        throw new Error("window.hyperion_visual, failed after 1s from window.onload")
      }
    }
    window.onload = wait_library
    </script>
`

const caller_script_head = `<script type="module" id='hyperion_visual_compressed_js_caller'>`

const suffix = `</body></html>`
