import { FolderApi, Pane } from 'tweakpane'
import * as EssentialsPlugin from '@tweakpane/plugin-essentials'
import { Config } from './config_pane'
import { createApp, watchEffect } from 'vue'
import { assert } from '@/util'
import { HyperionPluginBundle } from './tp_plugins'
import DualNodes from './tp_plugins/DualNodes.vue'
import CurrentSelect from './tp_plugins/CurrentSelect.vue'

/* info class of the current snapshot */
export class Info {
    config: Config
    // @ts-expect-error we will not use pane before it's initialized, ignore for simplicity
    pane: Pane

    constructor (config: Config) {
        this.config = config
    }

    create_pane (container: HTMLElement) {
        assert(this.pane == null, 'cannot create pane twice')
        this.pane = new Pane({
            title: 'Snapshot Info',
            container: container,
            expanded: false,
        })
        this.pane.registerPlugin(EssentialsPlugin)
        this.pane.registerPlugin(HyperionPluginBundle)
        this.add_dual_pane()
        this.add_current_selection()
    }

    display_zero_dual_variables: boolean = false
    // @ts-expect-error it will be set
    dual_folder: FolderApi
    add_dual_pane () {
        const folder = this.pane.addFolder({ title: 'Dual Variables', expanded: false })
        folder.addBinding(this, 'display_zero_dual_variables')
        folder.addBlade({ view: 'vue', app: createApp(DualNodes, { info: this }) })
        watchEffect(() => {
            folder.title = `Dual Variables (𝚺ys = ${this.config.snapshot.interface?.sum_dual})`
        })
        this.dual_folder = folder
    }

    // @ts-expect-error it will be set
    selection_folder: FolderApi
    add_current_selection () {
        const folder = this.pane.addFolder({ title: 'Current Selection', expanded: true })
        folder.addBlade({ view: 'vue', app: createApp(CurrentSelect, { info: this }) })
        watchEffect(() => {
            if (this.config.data.selected != undefined) {
                this.pane.expanded = true
                folder.expanded = true
            }
        })
        this.selection_folder = folder
    }
}
