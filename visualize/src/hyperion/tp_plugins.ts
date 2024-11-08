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
    BladeApi,
} from '@tweakpane/core'
import { type App } from 'vue'

export interface ControllerArguments<P extends BaseBladeParams> {
    blade: Blade
    document: Document
    viewProps: ViewProps
    params: P
}

export interface TpVueConfig extends BaseBladeParams {
    view: 'vue'
    app: App
}

export const TpVuePlugin: BladePlugin<TpVueConfig> = createPlugin({
    id: 'vue',
    type: 'blade',
    accept (params: Record<string, unknown>) {
        const result = parseRecord<TpVueConfig>(params, p => ({
            view: p.required.constant('vue'),
            app: p.required.raw as MicroParser<App>,
        }))
        return result ? { params: result } : null
    },
    controller (args: ControllerArguments<TpVueConfig>) {
        return new TpVueBladeController(args)
    },
    api (args) {
        if (!(args.controller instanceof TpVueBladeController)) {
            return null
        }
        return new BladeApi<TpVueBladeController>(args.controller)
    },
})

export class TpVueBladeController extends BladeController<View> {
    private instance: App

    constructor (args: ControllerArguments<TpVueConfig>) {
        // create view
        const element = args.document.createElement('div')
        args.params.app.mount(element)
        // do the standard initialization
        const viewProps = ViewProps.create()
        viewProps.handleDispose(() => {
            this.instance.unmount()
        })
        super({ blade: args.blade, view: { element }, viewProps })
        this.instance = args.params.app
    }
}

export const HyperionPluginBundle: TpPluginBundle = {
    // Identifier of the plugin bundle
    id: 'hyperion',
    // Plugins that should be registered
    plugins: [TpVuePlugin],
}
