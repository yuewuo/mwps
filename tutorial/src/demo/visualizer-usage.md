# Visualizer Usage

There are generally two ways to use the visualization tool, each with pros and cons
- web page: good for full-screen access, but not working well on remote machines (you have to SSH forwarding the HTTP server)
- Jupyter notebook: good for use in Jupyter Notebook with all your code and visuals in the same place, but with smaller figure

We would generally recommend using Jupyter notebook if you're using it, because you can always export a web page in the notebook
but not the other way around.

## Web Page

To simplify the build process and reduce the possibility that the building toolchain is broken some time in the future, w
commit the compiled library into the git repo. For people who needs to rebuild the visualization library, please see 
[the installation section](../installation.md#install-frontend-tools-optional). The compiled binary is placed at
`visualizer/data/mwpf-vis.js` in the repo.

TODO: to open an visualizer in the browser, follow the example below.

TODO: to save the visualizer in a file so that you can view later, see below.

TODO: alternatively, if you have a lot of files to be saved, you can choose to output the visualizer HTML and the data separately
to save space. See the example below.

## Jupyter Notebook

TODO: to add a visualizer block, follow the example below.

## Persistent (save to file)

Both the web page and the Jupyter notebook plugins allows persistent.

For Jupyter notebook, it is simple: all the output will be automatically saved as part of the notebook.
Note that we have optimized for the space.
Although the visualization library takes ~1MB overhead to your file, the overhead does not increase with the number of plots.

The web page also allows easy persistent.
When you open the visualizer configuration panel (press 'C' when your mouse hover over the window), you'll see an 'Import/Export'
folder.
You can choose to export a standalone HTML file, or you can choose to export separate HTML library with JSON data file.
You can even choose to include the current user settings into your exported HTML file (only supported for standalone HTML).
This will persist your camera view, customized colors and sizes, etc.
We find this feature useful when people try to tune the visual for their papers, and want to save it for future accesses.
