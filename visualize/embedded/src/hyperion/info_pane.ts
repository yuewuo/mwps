import { Pane, FolderApi } from 'tweakpane'
import * as EssentialsPlugin from '@tweakpane/plugin-essentials'
import { Config } from './config_pane'
import { type Ref } from 'vue'
import { assert } from '@/util'

/* info class of the current snapshot */
export class Info {
    config: Ref<Config>
    pane?: Pane

    constructor (config: Ref<Config>) {
        this.config = config
    }

    create_pane (container: HTMLElement) {
        assert(this.pane == null, 'cannot create pane twice')
        this.pane = new Pane({
            title: 'Snapshot Info',
            container: container,
            expanded: false
        })
        this.pane.registerPlugin(EssentialsPlugin)
        const pane: FolderApi = this.pane
    }
}
