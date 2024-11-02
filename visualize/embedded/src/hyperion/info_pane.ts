import { Pane, FolderApi, BladeApi } from 'tweakpane'
import * as EssentialsPlugin from '@tweakpane/plugin-essentials'
import { Config } from './config_pane'
import { createApp } from 'vue'
import { assert } from '@/util'
import { HyperionPluginBundle } from './tp_plugins'
import DualNodes from './DualNodes.vue'

/* info class of the current snapshot */
export class Info {
    config: Config
    // @ts-expect-error we will not use pane before it's initialized, ignore for simplicity
    pane: Pane

    dual_folder?: FolderApi
    dual_blades?: BladeApi

    constructor (config: Config) {
        this.config = config
    }

    create_pane (container: HTMLElement) {
        assert(this.pane == null, 'cannot create pane twice')
        this.pane = new Pane({
            title: 'Snapshot Info',
            container: container,
            expanded: true
        })
        this.pane.registerPlugin(EssentialsPlugin)
        this.pane.registerPlugin(HyperionPluginBundle)
        // create dual nodes
        this.dual_folder = this.pane.addFolder({ title: 'Dual Variables', expanded: true })
        this.dual_blades = this.dual_folder.addBlade({
            view: 'vue',
            app: createApp(DualNodes, { config: this.config })
        })
    }

    update_pane () {
        this.pane.refresh()
    }
}
