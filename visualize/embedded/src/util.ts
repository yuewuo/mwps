
export function assert(condition: any, msg?: string): asserts condition {
    if (!condition) {
        throw new Error(msg);
    }
}

export function is_string(object: unknown): boolean {
    return typeof object === 'string' || object instanceof String
}

export function uint8_to_array_buffer(array: Uint8Array): ArrayBuffer {
    return array.buffer.slice(array.byteOffset, array.byteLength + array.byteOffset)
}

export function array_buffer_to_base64(buffer: ArrayBuffer): string {
    return btoa(
        new Uint8Array(buffer)
            .reduce((data, byte) => data + String.fromCharCode(byte), '')
    )
}

export function base64_to_array_buffer(base64_str: string): ArrayBuffer {
    return uint8_to_array_buffer(Uint8Array.from(atob(base64_str), c => c.charCodeAt(0)))
}

export function assert_buffer_equal(buf1: ArrayBuffer, buf2: ArrayBuffer) {
    const error = new Error("decompressed buffer not equal to the original buffer")
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
