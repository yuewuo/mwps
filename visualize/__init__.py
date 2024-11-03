import os
import json
import string
import random
from typing import Optional
from IPython.display import display, Javascript, HTML

print(__file__)
FOLDER = os.path.dirname(os.path.abspath(__file__))
JS_PATH = os.path.join(FOLDER, "dist", "hyperion-visual.compressed.js")
assert (os.path.exists(JS_PATH), "please run `node run build` first")
with open(JS_PATH, "r", encoding="utf-8") as file:
    library_js = file.read()

library_loaded = False


def require_library() -> None:
    global library_loaded
    if library_loaded:
        return
    display(HTML("""<script type="module" id='hyperion_visual_compressed_js_caller'>
                           /* HYPERION_VISUAL_MODULE_CODE_BEGIN */
                           """ + library_js + """
                       /* HYPERION_VISUAL_MODULE_CODE_END */
                         </script>
                       """
                 ))
    library_loaded = True


def add_random_div(width: Optional[str] = None) -> str:
    id = ''.join(random.choices(string.ascii_lowercase, k=16))
    style = ""
    if width is not None:
        style = f'style="width: {width};"'
    display(HTML(f'<div {style} id="{id}"></div>'))
    return id


default_matrix = {
    "edges": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14],
    "hs": 0,
    "table": [
        ["", "0", "1", "2", "3", "4", "5", "6", "7", "8",
         "9", "1\n0", "1\n1", "1\n2", "1\n3", "1\n4", "="],
        ["0.", "1", "", "", "1", "", "", "", "",
         "1", "", "", "", "1", "", "", "1"],
        ["1.", "", "", "", "", "", "", "1", "1",
         "", "", "", "", "", "", "", "1"],
        ["2.", "", "1", "1", "", "", "", "",
         "", "", "", "", "", "", "", "", ""],
        ["3.", "", "", "1", "1", "1", "", "",
         "", "", "", "", "", "", "", "", ""],
        ["4.", "", "", "", "", "1", "1", "1",
         "", "", "", "", "", "", "", "", ""],
        ["5.", "1", "1", "", "", "", "", "", "",
         "", "", "", "", "", "", "1", ""],
        ["6.", "", "", "", "", "", "1", "", "",
         "1", "1", "", "", "", "", "", ""],
        ["7.", "", "", "", "", "", "", "", "1",
         "", "1", "", "", "", "", "", ""],
        ["8.", "", "", "", "", "", "", "", "",
         "", "", "", "", "", "1", "1", ""],
        ["9.", "", "", "", "", "", "", "", "",
         "", "", "", "1", "1", "1", "", ""],
        ["10.", "", "", "", "", "", "", "", "",
                "", "", "1", "1", "", "", "", ""]
    ],
    "version": "0.0.1",
    "is_echelon_form": False
}

js_call_main = '''
function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}
async function wait_library() {
  for (let i = 0; i < 10; ++i) {
    if (globalThis.hyperion_visual != null) {
      break
    }
    if (i != 0) console.log(`window.hyperion_visual not ready, tried ${i} times`)
    await sleep(100)
  }
  if (globalThis.hyperion_visual != null) {
    main()
  } else {
    throw new Error("window.hyperion_visual, failed after 1s from window.onload")
  }
}
wait_library()
'''


def display_test_matrix(matrix=None) -> None:
    require_library()
    id = add_random_div()
    parity_matrix_data = default_matrix
    if matrix is not None:
        parity_matrix_data = matrix.to_visualize_json()
    code = "let display_matrix = " + json.dumps(parity_matrix_data) + '''
function main() {
    globalThis.hyperion_visual.parity_matrix.bind_to_div("#''' + id + '''", display_matrix)
}
    ''' + js_call_main
    # print(code)
    display(Javascript(code))


def display_test_hypergraph(width: Optional[str] = "50%") -> None:
    require_library()
    id = add_random_div(width=width)
    code = '''
function main() {
    globalThis.hyperion_visual.hypergraph.bind_to_div("#''' + id + '''")
}
    ''' + js_call_main
    # print(code)
    display(Javascript(code))
