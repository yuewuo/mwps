import { inject, type Ref, ref } from 'vue'
import { assert } from '@/util'

export function assert_inject<T> (name: string | any): T {
    const injected: T | undefined = inject(name)
    assert(injected != undefined)
    return injected
}
