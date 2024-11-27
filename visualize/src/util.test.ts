import { parse_rust_bigint, display_nominator, tweakpane_find_value } from './util'
import { describe, expect, test } from 'vitest'

describe('testing parsing bigint from Rust output', () => {
    // npx vitest --testNamePattern 'normal integer'
    test('normal integer', () => {
        expect(parse_rust_bigint(123)).toBe(123)
    })

    // npx vitest --testNamePattern 'normal f64'
    test('normal f64', () => {
        expect(parse_rust_bigint(12.3)).toBe(12.3)
    })

    // npx vitest --testNamePattern 'big integer'
    test('big integer', () => {
        expect(parse_rust_bigint(234n)).toBe(234n)
    })

    // npx vitest --testNamePattern 'string'
    test('string', () => {
        expect(parse_rust_bigint('345')).toBe(345n)
    })

    // Rust program uses serde_json to serialize BigInt
    // it uses a custom serializer to convert BigInt to an array of [sign, [BigDigit, BigDigit, ...]]
    //    where BigDigit is u32, and sign is 1 for positive and -1 for negative
    // see https://docs.rs/num-bigint/0.4.5/src/num_bigint/bigint/serde.rs.html#42-51
    // note that while this is ugly, it is the most efficient way to serialize BigInt in Rust

    // npx vitest --testNamePattern 'Rust positive'
    test('Rust positive', () => {
        expect(parse_rust_bigint([1, [2789360843, 2091295457, 1046895595, 1558]])).toBe(123456789012345678901234567890123n)
    })

    // npx vitest --testNamePattern 'Rust negative'
    test('Rust negative', () => {
        expect(parse_rust_bigint([-1, [2789360843, 2091295457, 1046895595, 1558]])).toBe(-123456789012345678901234567890123n)
    })

    // npx vitest --testNamePattern 'Rust zero'
    test('Rust zero', () => {
        expect(parse_rust_bigint([0, []])).toBe(0n)
    })
})

describe('testing display_nominator for both bigint and number', () => {
    test('display bigint', () => {
        expect(display_nominator(123456789012345678901234567890123n)).toBe('123456789012345678901234567890123')
    })

    test('display very small floating-point', () => {
        expect(display_nominator(1e-50)).toBe('0.' + '0'.repeat(49) + '1')
    })

    test('display another small floating-point', () => {
        expect(display_nominator(1.23456789e-100)).toBe('0.' + '0'.repeat(99) + '123456789')
    })

    test('display negative small floating-point', () => {
        expect(display_nominator(-1.23456789e-100)).toBe('-0.' + '0'.repeat(99) + '123456789')
    })

    test('display negative small floating-point', () => {
        expect(display_nominator(-1.23456789e-10)).toBe('-0.' + '0'.repeat(9) + '123456789')
    })

    test('display NaN', () => {
        expect(display_nominator(NaN)).toBe('NaN')
    })

    test('display +inf', () => {
        // eslint-disable-next-line
        expect(display_nominator(1e10000)).toBe('Infinity')
    })

    test('display -inf', () => {
        // eslint-disable-next-line
        expect(display_nominator(-1e10000)).toBe('-Infinity')
    })
})

describe('tweakpane find value', () => {
    // npx vitest --testNamePattern 'tweakpane find value'
    test('tweakpane find value', () => {
        const data = {
            disabled: false,
            hidden: false,
            children: [
                {
                    disabled: false,
                    hidden: false,
                    children: [
                        {
                            disabled: false,
                            hidden: false,
                        },
                        {
                            disabled: false,
                            hidden: false,
                            label: 'zoom',
                            max: 1000,
                            min: 0.001,
                            binding: {
                                key: 'zoom',
                                value: 0.1998,
                            },
                        },
                        {
                            disabled: false,
                            hidden: false,
                            label: 'position',
                            binding: {
                                key: 'position',
                                value: {
                                    x: 0,
                                    y: 1000,
                                    z: 0,
                                },
                            },
                        },
                    ],
                    expanded: false,
                    title: 'Camera',
                },
                {
                    disabled: false,
                    hidden: false,
                    children: [
                        {
                            disabled: false,
                            hidden: false,
                            label: 'index',
                            max: 2,
                            min: 0,
                            binding: {
                                key: 'index',
                                value: 2,
                            },
                        },
                        {
                            disabled: false,
                            hidden: false,
                            label: 'name',
                            options: [
                                {
                                    text: '[0] syndrome',
                                    value: 0,
                                },
                                {
                                    text: '[1] grow 1',
                                    value: 1,
                                },
                                {
                                    text: '[2] shrink 1 + grow 1',
                                    value: 2,
                                },
                            ],
                            binding: {
                                key: 'name',
                                value: 2,
                            },
                        },
                    ],
                    expanded: true,
                    title: 'Snapshot',
                },
                {
                    disabled: false,
                    hidden: false,
                    children: [
                        {
                            disabled: false,
                            hidden: false,
                            label: null,
                            binding: {
                                key: 'user_note',
                                value: '1234',
                            },
                        },
                    ],
                    expanded: true,
                    title: 'Note',
                },
            ],
            expanded: true,
            title: 'MWPF Visualizer (3/3)',
        }
        expect(tweakpane_find_value(data, 'user_note')).toBe('1234')
    })
})
