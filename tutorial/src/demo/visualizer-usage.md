# Visualizer Usage

There are generally two ways to use the visualization tool, each with pros and cons
- web page: good for full-screen access, but not working well on remote machines (you have to SSH forwarding the HTTP server)
- Jupyter notebook: good for use in Jupyter Notebook with all your code and visuals in the same place, but with smaller figure

We would generally recommend using Jupyter notebook if you're using it, because you can always export a web page in the notebook
but not the other way around.

## Web Page

TODO: to open an visualizer in the browser, follow the example below.

TODO: to save the visualizer in a file so that you can view later, see below.

TODO: alternatively, if you have a lot of files to be saved, you can choose to output the visualizer HTML and the data separately
to save space. See the example below.

## Jupyter Notebook

TODO: to add a visualizer block, follow the example below.

## Persistent (save to file)

Both the web page and the Jupyter notebook plugins allows persistent.

For Jupyter notebook, it is simple: all the output will be automatically saved as part of the notebook.
Note that we have optimized for the space, so that although the visualization library has ~500KB overhead, the overhead does not increase with the number of plots.

The web page also allows easy persistent.
When you open the visualizer configuration panel (press 'C' when your mouse hover over the window), you'll see an 'Import/Export'
folder.
You can choose to export a standalone HTML file, which can also include the existing settings like camera view, customized colors and sizes, etc.
We find this feature useful when people try to tune the visual for their papers, and want to save it for future accesses.
