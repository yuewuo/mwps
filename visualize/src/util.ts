import { JSONParse, JSONStringify } from 'json-with-bigint'
import stringify from 'json-stringify-pretty-compact'

export function assert (condition: boolean, msg?: string): asserts condition {
    if (!condition) {
        throw new Error(msg)
    }
}

export function is_string (object: unknown): boolean {
    return typeof object === 'string' || object instanceof String
}

export function uint8_to_array_buffer (array: Uint8Array): ArrayBuffer {
    return array.buffer.slice(array.byteOffset, array.byteLength + array.byteOffset)
}

export function array_buffer_to_base64 (buffer: ArrayBuffer): string {
    return btoa(new Uint8Array(buffer).reduce((data, byte) => data + String.fromCharCode(byte), ''))
}

export function base64_to_array_buffer (base64_str: string): ArrayBuffer {
    return uint8_to_array_buffer(Uint8Array.from(atob(base64_str), c => c.charCodeAt(0)))
}

export function assert_buffer_equal (buf1: ArrayBuffer, buf2: ArrayBuffer) {
    const error = new Error('decompressed buffer not equal to the original buffer')
    if (buf1.byteLength != buf2.byteLength) {
        throw error
    }
    const dv1 = new Int8Array(buf1)
    const dv2 = new Int8Array(buf2)
    for (let i = 0; i != buf1.byteLength; i++) {
        if (dv1[i] != dv2[i]) {
            throw error
        }
    }
}

export function sleep (ms: number) {
    return new Promise(resolve => setTimeout(resolve, ms))
}

export interface BigIntStringifyOptions {
    indent?: number | string
    maxLength?: number
    replacer?: ((this: any, key: string, value: any) => any) | (number | string)[]
}

export const bigInt = {
    JSONParse,
    JSONStringify,
    // modified from https://github.com/Ivan-Korolenko/json-with-bigint/blob/main/json-with-bigint.js
    // by using json-stringify-pretty-compact to generate a pretty JSON
    PrettyJSONStringify: (data: any, options?: BigIntStringifyOptions): string => {
        const bigInts = /([[:])?"(-?\d+)n"([,}\]])/g
        const preliminaryJSON = JSON.stringify(data, (_, value) => (typeof value === 'bigint' ? value.toString() + 'n' : value))
        const prettyJSON = stringify(JSON.parse(preliminaryJSON), options)
        return prettyJSON.replace(bigInts, '$1$2$3')
    },
    JavascriptStringify: (data: any, options?: BigIntStringifyOptions): string => {
        const bigInts = /([[:])?"(-?\d+)n"([,}\]])/g
        const preliminaryJSON = JSON.stringify(data, (_, value) => (typeof value === 'bigint' ? value.toString() + 'n' : value))
        const prettyJSON = stringify(JSON.parse(preliminaryJSON), options)
        return prettyJSON.replace(bigInts, '$1$2n$3')
    },
}

export async function compress_content (buffer: ArrayBuffer): Promise<string> {
    const blob = new Blob([buffer])
    const stream = blob.stream().pipeThrough(new CompressionStream('gzip'))
    const compressed = await new Response(stream).arrayBuffer()
    const base64_string = array_buffer_to_base64(compressed)
    return base64_string
}

export async function decompress_content (base64_str: string): Promise<ArrayBuffer> {
    const base64_binary = base64_to_array_buffer(base64_str)
    const blob = new Blob([base64_binary])
    const decompressed_stream = blob.stream().pipeThrough(new DecompressionStream('gzip'))
    const decompressed = await new Response(decompressed_stream).arrayBuffer()
    return decompressed
}

export function parse_rust_bigint (data: any): bigint | number {
    if (typeof data === 'number' || typeof data === 'bigint') {
        return data
    } else if (typeof data === 'string') {
        return BigInt(data)
    } else if (typeof data === 'object') {
        assert(data.length === 2)
        const [sign, digits] = data
        assert(typeof sign === 'number')
        assert(sign == 1 || sign == -1 || sign == 0)
        assert(typeof digits === 'object')
        let value = 0n
        for (let i = digits.length - 1; i >= 0; i--) {
            value = (value << 32n) + BigInt(digits[i])
        }
        return BigInt(sign) * value
    } else {
        throw new Error(`invalid data type: ${typeof data}`)
    }
}

export function display_nominator (dn: bigint | number): string {
    if (typeof dn == 'bigint') {
        return dn.toString()
    } else {
        if (dn == 0 || isNaN(dn) || !isFinite(dn)) {
            return `${dn}`
        }
        const exponent = Math.floor(Math.log10(Math.abs(dn)))
        let value = dn.toFixed(0)
        if (exponent < 15) {
            // add more decimal digits
            if (exponent < -85) {
                // need special care because toFixed only supports up to 100
                const val = (dn * Math.pow(10, -exponent - 85)).toFixed(100)
                if (dn > 0) {
                    value = '0.' + '0'.repeat(-exponent - 85) + val.slice(2)
                } else {
                    value = '-0.' + '0'.repeat(-exponent - 85) + val.slice(3)
                }
            } else {
                value = dn.toFixed(-exponent + 15)
            }
        }
        return value.replace(/(\.0*|0+)$/, '')
    }
}

export function tweakpane_find_value (obj: any, name: string): any {
    for (const key in obj) {
        if (obj['key'] === name) {
            return obj['value']
        } else if (typeof obj[key] === 'object') {
            const result = tweakpane_find_value(obj[key], name)
            if (result !== undefined) {
                return result
            }
        }
    }
    return
}
