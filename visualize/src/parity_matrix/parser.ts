import { assert, is_string } from '@/util'

export class ParityMatrixData {
    public version: string
    public edges: number[]
    public table: PrintTable
    public is_echelon_form: boolean
    public tail_start_index?: number
    public corner_row_index?: number

    constructor (data: object) {
        let table: PrintTable | null = null
        let version: string | null = null
        let edges: number[] | null = null
        let is_echelon_form: boolean = false
        for (const [key, value] of Object.entries(data)) {
            switch (key) {
                case 'table':
                    table = new PrintTable(value)
                    break
                case 'version':
                    version = value as string
                    break
                case 'edges':
                    edges = value as number[]
                    break
                case 'is_echelon_form':
                    is_echelon_form = value as boolean
                    break
                case 'tail_start_index':
                    this.tail_start_index = value as number
                    break
                case 'corner_row_index':
                    this.corner_row_index = value as number
                    break
            }
        }
        assert(table != null, 'table must be provided')
        this.table = table
        assert(version != null)
        this.version = version
        assert(edges != null)
        this.edges = edges
        this.is_echelon_form = is_echelon_form
    }
}

export class PrintTable {
    rows: PrintTableRow[]

    constructor (object: object) {
        this.rows = []
        assert(Array.isArray(object), 'table must be array')
        assert(object.length != 0, 'table should at least come with title')
        assert(Array.isArray(object[0]), 'title line must be array')
        const length = object[0].length
        for (const row_object of object) {
            const row = new PrintTableRow(row_object)
            assert(row.length == length)
            this.rows.push(row)
        }
    }

    get dimension (): [number, number] {
        let column_length = 0
        if (this.rows.length > 0) {
            column_length = this.rows[0].length
        }
        return [this.rows.length, column_length]
    }

    at (i: number, j: number): string {
        assert(i < this.rows.length, 'table index overflow')
        const row = this.rows[i]
        assert(j < row.length, 'table index overflow')
        return row.elements[j]
    }
}

export class PrintTableRow {
    elements: string[]

    get length (): number {
        return this.elements.length
    }

    constructor (object: object) {
        this.elements = []
        assert(Array.isArray(object), 'row must be array')
        for (const value of object) {
            assert(is_string(value), 'table element must be string')
            this.elements.push(value as string)
        }
    }
}
