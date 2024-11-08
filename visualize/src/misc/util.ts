import { inject } from 'vue'
import { assert } from '@/util'

export function assert_inject<T> (name: string): T {
    const injected: T | undefined = inject(name)
    assert(injected != undefined)
    return injected
}
