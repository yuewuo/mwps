import { describe, test } from 'vitest'
import { ClusterPolygon, type Vector2, type Line2 } from './cluster_polygon'
import hull from 'hull'
import * as assert from 'assert'

describe('testing cluster polygon', () => {
    // npx vitest --testNamePattern 'hull example'
    test('hull example', () => {
        const points = [
            [141, 408],
            [160, 400],
            [177, 430],
            [151, 442],
            [155, 425],
            [134, 430],
            [126, 447],
            [139, 466],
            [160, 471],
            [167, 447],
            [182, 466],
            [192, 442],
            [187, 413],
            [173, 403],
            [165, 430],
            [171, 430],
            [177, 437],
            [175, 443],
            [172, 444],
            [163, 448],
            [156, 447],
            [153, 438],
            [154, 431],
            [160, 428],
        ]
        const expected = [
            [192, 442],
            [182, 466],
            [160, 471],
            [139, 466],
            [126, 447],
            [141, 408],
            [160, 400],
            [173, 403],
            [187, 413],
            [192, 442],
        ]
        assert.deepEqual(hull(points, 50), expected)
    })

    // npx vitest --testNamePattern 'cluster polygon example 1'
    test('cluster polygon example 1', async () => {
        const vertices: Array<Vector2> = [
            [0, 0],
            [1, 0],
            [0, 1],
            [1, 1],
            [0.5, 0.5],
        ]
        const edges: Array<Line2> = [
            [
                [0.2, 0.2],
                [0.2, 0.8],
            ],
            [
                [0.2, 0.2],
                [0.8, 0.2],
            ],
            [
                [0.2, 0.8],
                [0.8, 0.8],
            ],
        ]
        const clusterPolygon = new ClusterPolygon(vertices, edges)
        const expected = [
            [1, 1],
            [0, 1],
            [0, 0],
            [1, 0],
            [0.5, 0.5],
            [1, 1],
        ]
        const polygon = await clusterPolygon.polygon()
        assert.deepEqual(polygon, expected)
    })
})
