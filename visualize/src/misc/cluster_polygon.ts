/*
 * Finding a polygon of an MWPF cluster
 *
 * The polygon must satisfy the following conditions:
 * 1. The vertices are the end points of the grown portions of the edge segments, except for the points whose incident
 *          edges form the complete 2D plane.
 *        The above definition may not be clear so here in an example:
 *            if a vertex has the following shape:  -- *  then it can be a vertex of the polygon
 *                                                     |
 *                                                                         /
 *            on the other hand, if a vertex has the following shape: -- * -- then it cannot be a vertex of the polygon
 * 2. The polygon should fully cover all grown portions of the edge segments
 * 3. The polygon should be as small as possible
 *
 * Note that the existence of such a polygon satisfying both 1 and 2 is guaranteed by how 1 is selected.
 * We can then use a simple greedy algorithm to minimize the polygon, but not necessarily the smallest.
 */

import hull from 'hull'
import { polygonArea, pointInPolygon, lineIntersectsLine } from 'geometric'
import { assert } from '@/util'
import { type VisualizerData } from '@/hyperion/hyperion'

export type Vector2 = [number, number]
export type Line2 = [Vector2, Vector2]

export class ClusterPolygon {
    vertices: Array<Vector2>
    edges: Array<Line2>

    all_vertices: Array<Vector2>
    bounding_box: [Vector2, Vector2]
    max_edge_length: number

    constructor (vertices: Array<Vector2>, edges: Array<Line2>) {
        assert(vertices.length > 0)
        this.vertices = vertices
        this.edges = edges
        this.all_vertices = vertices.concat(edges.map(edge => edge[0]).concat(edges.map(edge => edge[1])))
        const min_x = this.all_vertices.map(v => v[0]).reduce((a, b) => Math.min(a, b), -Infinity)
        const max_x = this.all_vertices.map(v => v[0]).reduce((a, b) => Math.max(a, b), +Infinity)
        const min_y = this.all_vertices.map(v => v[1]).reduce((a, b) => Math.min(a, b), -Infinity)
        const max_y = this.all_vertices.map(v => v[1]).reduce((a, b) => Math.max(a, b), +Infinity)
        this.bounding_box = [
            [min_x, min_y],
            [max_x, max_y],
        ]
        this.max_edge_length = Math.sqrt(Math.pow(max_x - min_x, 2) + Math.pow(max_y - min_y, 2))
    }

    async polygon (): Promise<Array<Vector2>> {
        const yielder = () => new Promise(resolve => setTimeout(() => resolve(1), 0))
        // first get the convex hull
        let convex: Array<Vector2> = hull(this.all_vertices, this.max_edge_length * 2) as any
        let inner_points = this.vertices.filter(point => convex.indexOf(point) < 0)
        // then recursively try to reduce the area of the convex hull without introducing
        assert(convex.length > 1)
        assert(convex[0] == convex[convex.length - 1], 'should be a cycle')
        let found = true
        while (found) {
            found = false
            // iterate over the polygon path
            for (let i = 0; i < convex.length - 1; ++i) {
                await yielder() // do not block the control because this computation is usually pretty long
                // then try to see if injecting a vertex will reduce the area without violating the rules
                for (const [point_index, point] of inner_points.entries()) {
                    let violation = false
                    // check (roughly) if all the edges are within the polygon
                    let new_convex = convex
                        .slice(0, i + 1)
                        .concat([point])
                        .concat(convex.slice(i + 1))
                    for (const edge of this.edges) {
                        const samples = 5
                        for (let si = 0; si < samples + 1; ++si) {
                            const t = (si / samples) * 0.95 + 0.05 // allow some numerical error
                            const x = edge[0][0] * t + edge[1][0] * (1 - t)
                            const y = edge[0][1] * t + edge[1][1] * (1 - t)
                            if (!pointInPolygon([x, y], new_convex)) {
                                violation = true
                                break
                            }
                        }
                        if (violation) {
                            break
                        }
                    }
                    if (!violation) {
                        // found a new convex
                        convex = new_convex
                        inner_points.splice(point_index, 1) // delete this point
                        found = true
                        break
                    }
                }
                if (found) {
                    break
                }
            }
            break
        }
        return convex
    }

    static cluster_plane_available (visualizer_data: VisualizerData): boolean {
        // cluster planes are available when all the vertices are on the same plane
        if (visualizer_data.positions.length == 0) {
            return false
        }
        const t = visualizer_data.positions[0].t
        for (const position of visualizer_data.positions) {
            if (position.t != t) {
                return false
            }
        }
        return true
    }
}
