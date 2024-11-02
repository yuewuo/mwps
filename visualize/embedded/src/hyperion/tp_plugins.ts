import {
    createPlugin,
    type TpPluginBundle,
    type View,
    ViewProps,
    type BladePlugin,
    type BaseBladeParams,
    BladeController,
    type Blade,
    parseRecord,
    type MicroParser,
    BladeApi
} from '@tweakpane/core'
import { type DualNode } from './hyperion'
import styleString from './tp_plugins.css?raw'
import DualNodeVue from './DualNode.vue'
import { createApp, type App } from 'vue'

export interface ControllerArguments<P extends BaseBladeParams> {
    blade: Blade
    document: Document
    viewProps: ViewProps
    params: P
}

export interface DualNodesConfig extends BaseBladeParams {
    value: Array<DualNode>
    label?: string
    view: 'dual_nodes'
}

export const DualNodesPlugin: BladePlugin<DualNodesConfig> = createPlugin({
    id: 'dual_nodes',
    type: 'blade',
    accept (params: Record<string, unknown>) {
        const result = parseRecord<DualNodesConfig>(params, p => ({
            view: p.required.constant('dual_nodes'),
            label: p.optional.string,
            value: p.optional.raw as MicroParser<DualNode[]>
        }))
        return result ? { params: result } : null
    },
    controller (args: ControllerArguments<DualNodesConfig>) {
        return new DualNodesBladeController(args)
    },
    api (args) {
        if (!(args.controller instanceof DualNodesBladeController)) {
            return null
        }
        return new BladeApi<DualNodesBladeController>(args.controller)
    }
})

export class DualNodesBladeController extends BladeController<View> {
    private instances: Array<App> = []

    constructor (args: ControllerArguments<DualNodesConfig>) {
        // create view
        const element = args.document.createElement('div')
        // do the standard initialization
        const viewProps = ViewProps.create()
        viewProps.handleDispose(() => {
            for (const instance of this.instances) {
                instance.unmount()
            }
        })
        super({ blade: args.blade, view: { element }, viewProps })
        // create elements
        for (const dual_node of args.params.value) {
            const nodeElement = args.document.createElement('div')
            element.appendChild(nodeElement)
            nodeElement.classList.add('hy-dual-div')
            const previewElem = args.document.createElement('div')
            previewElem.textContent = JSON.stringify(dual_node)
            nodeElement.appendChild(previewElem)
            const buttonElem = args.document.createElement('button')
            buttonElem.textContent = '+'
            buttonElem.addEventListener('mouseenter', () => {
                console.log('mouseenter')
            })
            buttonElem.addEventListener('mouseleave', () => {
                console.log('mouseleave')
            })
            buttonElem.addEventListener('click', () => {
                console.log('click')
            })
            nodeElement.appendChild(buttonElem)
            // add vue component
            const div1 = args.document.createElement('div')
            nodeElement.appendChild(div1)
            const instance1 = createApp(DualNodeVue)
            instance1.mount(div1)
            this.instances.push(instance1)
            const div2 = args.document.createElement('div')
            nodeElement.appendChild(div2)
            const instance2 = createApp(DualNodeVue)
            instance2.mount(div2)
            this.instances.push(instance2)
        }
    }
}

export const HyperionPluginBundle: TpPluginBundle = {
    // Identifier of the plugin bundle
    id: 'hyperion',
    // Plugins that should be registered
    plugins: [DualNodesPlugin],
    // Additional CSS for this bundle
    css: styleString
}
