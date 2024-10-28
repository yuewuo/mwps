// 3d related apis

import * as THREE from 'three'
import { OrbitControls } from './node_modules/three/examples/jsm/controls/OrbitControls.js'
import Stats from './node_modules/three/examples/jsm/libs/stats.module.js'
import GUI from './node_modules/three/examples/jsm/libs/lil-gui.module.min.js'
import * as BufferGeometryUtils from './node_modules/three/examples/jsm/utils/BufferGeometryUtils.js'


function create_singular_edge_geometry(inner_radius, outer_radius) {
    const singular_edge_geometry_bottom = new THREE.RingGeometry(inner_radius, outer_radius, segment)
    singular_edge_geometry_bottom.rotateX(Math.PI / 2)
    // singular_edge_geometry_bottom.translate(0, -outer_radius / 2, 0)
    const singular_edge_geometry_top = new THREE.RingGeometry(inner_radius, outer_radius, segment)
    singular_edge_geometry_top.rotateX(-Math.PI / 2)
    // singular_edge_geometry_top.translate(0, outer_radius / 2, 0)
    return BufferGeometryUtils.mergeBufferGeometries([singular_edge_geometry_bottom, singular_edge_geometry_top])
}
const singular_edge_geometry = create_singular_edge_geometry(0, vertex_radius * 2)
// in order to support segmented singular edge, we need to create a list of geometries according to the ratio
const singular_edge_geometry_segments = []
const singular_edge_resolution = 100
for (let i = 0; i <= singular_edge_resolution; ++i) {
    singular_edge_geometry_segments.push(create_singular_edge_geometry(vertex_radius * 2 * i / singular_edge_resolution, vertex_radius * 2))
}
function get_singular_edge_geometry_segments(ratio) {
    const index = parseInt(ratio * singular_edge_resolution)
    if (index < 0) { index = 0 }
    if (index > singular_edge_resolution) { index = singular_edge_resolution }
    return singular_edge_geometry_segments[index]
}
const normal_edge_geometry = new THREE.CylinderGeometry(edge_radius, edge_radius, 1, segment, 1, true)
normal_edge_geometry.translate(0, 0.5, 0)
const tri_edge_geometry = new THREE.CylinderGeometry(edge_radius * 1.5, edge_radius * 1.5, 1, segment, 1, true)
tri_edge_geometry.translate(0, 0.5, 0)
const quad_edge_geometry = new THREE.CylinderGeometry(edge_radius * 2, edge_radius * 2, 1, segment, 1, true)
quad_edge_geometry.translate(0, 0.5, 0)
const edge_geometries = [
    singular_edge_geometry,
    normal_edge_geometry,
    tri_edge_geometry,
    quad_edge_geometry,
]
function get_edge_geometry(edge_degree) {
    if (edge_degree - 1 < edge_geometries.length) return edge_geometries[edge_degree - 1]
    return edge_geometries[edge_geometries.length - 1]
}

export const edge_materials = []
let empty_edge_color = new THREE.Color(0, 0, 0)
let grown_edge_color = new THREE.Color(1, 0, 0)
let empty_edge_opacity = 0.1
let grown_edge_opacity = 1
let almost_empty_ratio = 0.1
let almost_grown_ratio = 0.3
let edge_side = THREE.BackSide
const color_steps = 20  // there are 20 colors in the middle apart from the empty and full
export function lerpColors(color1, color2, ratio) {
    let c1 = new THREE.Color(color1)
    let c2 = new THREE.Color(color2)
    let c = new THREE.Color().lerpColors(c1, c2, ratio)
    return "#" + c.getHexString()
}
function make_edge_material(ratio) {
    if (ratio < 0) ratio = 0
    if (ratio > 1) ratio = 1
    return new THREE.MeshStandardMaterial({
        color: lerpColors(empty_edge_color, grown_edge_color, ratio),
        opacity: empty_edge_opacity + (grown_edge_opacity - empty_edge_opacity) * ratio,
        transparent: true,
        side: edge_side
    })
}
edge_materials.push(make_edge_material(0))
edge_materials.push(make_edge_material(1))
for (let i = 0; i < color_steps; ++i) {
    const ratio = almost_empty_ratio +
        (almost_grown_ratio - almost_empty_ratio) * i / (color_steps - 1)
    edge_materials.push(make_edge_material(ratio))
}
function update_edge_materials() {
    for (let idx = 0; idx < color_steps + 2; ++idx) {
        let ratio = almost_empty_ratio +
            (almost_grown_ratio - almost_empty_ratio) * (idx - 2) / (color_steps - 1)
        if (idx == 0) ratio = 0
        if (idx == 1) ratio = 1
        let new_material = make_edge_material(ratio)
        edge_materials[idx].color = new_material.color
        edge_materials[idx].side = new_material.side
        edge_materials[idx].opacity = new_material.opacity
        new_material.dispose()
    }
}
export function get_edge_material(grown, weight) {
    if (grown <= 0 && weight != 0) {  // empty grown
        return edge_materials[0]
    } else if (grown >= weight) {  // fully grown
        return edge_materials[1]
    } else {
        let idx = Math.floor(grown / weight * color_steps)
        if (idx < 0) idx = 0
        if (idx >= color_steps) idx = color_steps - 1
        return edge_materials[idx + 2]
    }
}
export let segmented_edge_colors = [
    // "#D52C1C",  // red
    "#44C03F",  // green
    // "#2723F7",  // blue
    "#F6C231",  // yellow
    "#4DCCFB",  // light blue
    "#F17B24",  // orange
    "#7C1DD8",  // purple
    "#8C4515",  // brown
    "#E14CB6",  // pink
]
let segmented_untight_opacity = parseFloat(urlParams.get('segmented_untight_opacity') || 0.2)
let segmented_tight_opacity = 1
export const segmented_edge_materials = []
function update_segmented_edge_materials() {
    for (let [untight, tight] of segmented_edge_materials) {
        untight.dispose()
        tight.dispose()
    }
    segmented_edge_materials.splice(0, segmented_edge_materials.length) // clear
    for (let color of segmented_edge_colors) {
        const tight = new THREE.MeshStandardMaterial({
            color: color,
            opacity: segmented_tight_opacity,
            transparent: true,
            side: edge_side
        })
        const untight = new THREE.MeshStandardMaterial({
            color: color,
            opacity: segmented_untight_opacity,
            transparent: true,
            side: edge_side
        })
        segmented_edge_materials.push([untight, tight])
    }
}
update_segmented_edge_materials()
export function get_segmented_edge_material(is_tight, node_index) {
    return segmented_edge_materials[node_index % segmented_edge_materials.length][is_tight ? 1 : 0]
}
export const subgraph_edge_material = new THREE.MeshStandardMaterial({
    color: 0x0000ff,
    opacity: 1,
    transparent: true,
    side: THREE.FrontSide,
})
export const hover_material = new THREE.MeshStandardMaterial({  // when mouse is on this object (vertex or edge)
    color: 0x6FDFDF,
    side: THREE.DoubleSide,
})
export const selected_material = new THREE.MeshStandardMaterial({  // when mouse is on this object (vertex or edge)
    color: 0x4B7BE5,
    side: THREE.DoubleSide,
})

export var edge_vec_meshes = []
export var edge_caches = []  // store some information that can be useful
window.edge_vec_meshes = edge_vec_meshes


function calculate_edge_to_dual_indices(snapshot) {
    const dual_indices = Array(snapshot.edges.length).fill(null).map(() => Array())
    if (snapshot.dual_nodes != null) {
        for (let [node_index, node] of snapshot.dual_nodes.entries()) {
            for (let edge_index of node.h) {
                dual_indices[edge_index].push(node_index)
            }
        }
    }
    return dual_indices
}
export async function refresh_snapshot_data() {
    // console.log("refresh_snapshot_data")
    if (active_mwpf_data.value != null) {  // no mwpf data provided
        const mwpf_data = active_mwpf_data.value
        const snapshot_idx = active_snapshot_idx.value
        const snapshot = mwpf_data.snapshots[snapshot_idx][1]
        // clear hover and select
        current_hover.value = null
        let current_selected_value = JSON.parse(JSON.stringify(current_selected.value))
        current_selected.value = null
        await Vue.nextTick()
        await Vue.nextTick()

        // draw vertices

        // draw edges
        let subgraph_set = {}
        if (snapshot.subgraph != null) {
            for (let edge_index of snapshot.subgraph) {
                subgraph_set[edge_index] = true
            }
        }
        let edge_offset = 0
        if (scaled_edge_radius.value < scaled_vertex_outline_radius.value) {
            edge_offset = Math.sqrt(Math.pow(scaled_vertex_outline_radius.value, 2) - Math.pow(scaled_edge_radius.value, 2))
        }
        edge_caches = []  // clear cache
        let edge_to_dual_indices = null
        if (segmented.value) {
            edge_to_dual_indices = calculate_edge_to_dual_indices(snapshot)
        }
        let edge_branch_segmented_data = null  // new segment visualization method from Katie: each branch display differently
        for (let [i, edge] of snapshot.edges.entries()) {
            // calculate the center point of all vertices
            let sum_position = new THREE.Vector3(0, 0, 0)
            for (let j = 0; j < edge.v.length; ++j) {
                const vertex_index = edge.v[j]
                const vertex_position = mwpf_data.positions[vertex_index]
                sum_position = sum_position.add(compute_vector3(vertex_position))
            }
            const center_position = sum_position.multiplyScalar(1 / edge.v.length)
            let local_edge_cache = []
            edge_caches.push(local_edge_cache)
            while (edge_vec_meshes.length <= i) {
                edge_vec_meshes.push([])
            }
            let edge_vec_mesh = edge_vec_meshes[i]
            for (let j = 0; j < edge_vec_mesh.length; ++j) {
                scene.remove(edge_vec_mesh[j])
            }
            edge_vec_mesh.splice(0, edge_vec_mesh.length) // clear
            const edge_material = get_edge_material(edge.g, edge.w)
            const create_edge_mesh = () => {
                const edge_mesh = new THREE.Mesh(get_edge_geometry(edge.v.length), edge_material)
                edge_mesh.userData = {
                    type: "edge",
                    edge_index: i,
                }
                edge_mesh.visible = false
                scene.add(edge_mesh)
                edge_vec_mesh.push(edge_mesh)
                return edge_mesh
            }
            // when display in segments, calculate the edge properties for each branch
            if (segmented.value) {
                edge_branch_segmented_data = calculate_edge_branch_segmented(snapshot, edge_to_dual_indices, i)
            }
            for (let j = 0; j < edge.v.length; ++j) {
                const vertex_index = edge.v[j]
                const vertex_position = mwpf_data.positions[vertex_index]
                const relative = center_position.clone().add(compute_vector3(vertex_position).multiplyScalar(-1))
                const direction = relative.clone().normalize()
                // console.log(direction)
                const quaternion = new THREE.Quaternion()
                quaternion.setFromUnitVectors(unit_up_vector, direction)
                let start = edge_offset
                const distance = relative.length()
                let edge_length = distance - edge_offset
                if (edge_length < 0) {  // edge length should be non-negative
                    start = distance
                    edge_length = 0
                }
                const end = start + edge_length
                let start_position = compute_vector3(vertex_position).add(relative.clone().multiplyScalar(start / distance))
                let end_position = compute_vector3(vertex_position).add(relative.clone().multiplyScalar(end / distance))
                if (edge.v.length == 1) {
                    start_position = compute_vector3(vertex_position)
                    end_position = compute_vector3(vertex_position)
                }
                const segment_position_of = (ratio) => {  // 0: start, 1: end
                    return start_position.clone().multiplyScalar(1 - ratio).add(end_position.clone().multiplyScalar(ratio))
                }
                local_edge_cache.push({
                    position: {
                        start: start_position,
                        end: end_position,
                    }
                })
                if (segmented.value) {
                    const grown_end = edge_branch_segmented_data.grown_end[j]
                    const grown_center = edge_branch_segmented_data.grown_center[j]
                    const segments_center = edge_branch_segmented_data.contributor_center[j]
                    const segments_end = edge_branch_segmented_data.contributor_end[j]
                    // calculate the segments of this edge branch
                    let accumulated_ratio = 0
                    const branch_weight = edge.w / edge.v.length
                    const segments = []
                    //     growing from end vertices
                    for (const [ni, length] of segments_end) {
                        const ratio = length / branch_weight
                        segments.push([ni, accumulated_ratio, ratio])
                        accumulated_ratio += ratio
                    }
                    //     the middle empty segment
                    if (grown_end + grown_center < branch_weight) {
                        const ratio = (branch_weight - grown_end - grown_center) / branch_weight
                        segments.push([null, accumulated_ratio, ratio])
                        accumulated_ratio += ratio
                    }
                    //     growing from center vertices
                    for (let index = segments_center.length - 1; index >= 0; index--) {
                        const [ni, length] = segments_center[index]
                        const ratio = length / branch_weight
                        segments.push([ni, accumulated_ratio, ratio])
                        accumulated_ratio += ratio
                    }
                    // create the segments
                    for (const [node_index, accumulated_ratio, segment_ratio] of segments) {
                        const edge_mesh = create_edge_mesh()
                        edge_mesh.position.copy(segment_position_of(accumulated_ratio))
                        if (edge.v.length != 1) {
                            edge_mesh.scale.set(1, edge_length * segment_ratio, 1)
                            edge_mesh.setRotationFromQuaternion(quaternion)
                        } else {
                            let func = (ratio) => 0.5 * outline_ratio.value + (1 - 0.5 * outline_ratio.value) * ratio
                            let inner = func(accumulated_ratio)
                            let outer = func(segment_ratio + accumulated_ratio)
                            edge_mesh.geometry = get_singular_edge_geometry_segments(inner / outer)
                            edge_mesh.scale.set(outer, 1, outer)
                        }
                        edge_mesh.visible = true
                        edge_mesh.renderOrder = 20 - edge.v.length  // better visual effect
                        if (node_index != null) {
                            edge_mesh.material = get_segmented_edge_material(edge.un == 0, node_index)
                        } else {
                            edge_mesh.material = get_edge_material(0, edge.w)
                        }
                        if (snapshot.subgraph != null) {
                            edge_mesh.material = get_edge_material(0, edge.w)  // do not display grown edges
                        }
                        if (subgraph_set[i]) {
                            edge_mesh.material = subgraph_edge_material
                        }
                    }
                } else {
                    const edge_mesh = create_edge_mesh()
                    edge_mesh.position.copy(start_position)
                    if (edge.v.length != 1) {
                        edge_mesh.scale.set(1, edge_length, 1)
                        edge_mesh.setRotationFromQuaternion(quaternion)
                    }
                    edge_mesh.visible = true
                    if (edge.v.length != 1 && edge_length == 0) {
                        edge_mesh.visible = false
                    }
                    edge_mesh.renderOrder = 20 - edge.v.length  // better visual effect
                    if (snapshot.subgraph != null) {
                        edge_mesh.material = get_edge_material(0, edge.w)  // do not display grown edges
                    }
                    if (subgraph_set[i]) {
                        edge_mesh.material = subgraph_edge_material
                    }
                }
            }
        }
        for (let i = snapshot.edges.length; i < edge_vec_meshes.length; ++i) {
            for (let edge_mesh of edge_vec_meshes[i]) {
                edge_mesh.visible = false
            }
        }
        // draw vertex outlines
        for (let [i, vertex] of snapshot.vertices.entries()) {
            if (vertex == null) {
                if (i < vertex_outline_meshes.length) {  // hide
                    vertex_outline_meshes[i].visible = false
                }
                continue
            }
            let position = mwpf_data.positions[i]
            while (vertex_outline_meshes.length <= i) {
                const vertex_outline_mesh = new THREE.Mesh(vertex_geometry, normal_vertex_outline_material)
                vertex_outline_mesh.visible = false
                update_mesh_outline(vertex_outline_mesh)
                scene.add(vertex_outline_mesh)
                vertex_outline_meshes.push(vertex_outline_mesh)
            }
            const vertex_outline_mesh = vertex_outline_meshes[i]
            load_position(vertex_outline_mesh.position, position)
            if (vertex.s) {
                vertex_outline_mesh.material = defect_vertex_outline_material
            } else if (vertex.v) {
                vertex_outline_mesh.material = virtual_vertex_outline_material
            } else {
                vertex_outline_mesh.material = normal_vertex_outline_material
            }
            vertex_outline_mesh.visible = true
        }
        for (let i = snapshot.vertices.length; i < vertex_meshes.length; ++i) {
            vertex_outline_meshes[i].visible = false
        }
        // reset select
        await Vue.nextTick()
        if (is_user_data_valid(current_selected_value)) {
            current_selected.value = current_selected_value
        }
    }
}
watch([active_mwpf_data, active_snapshot_idx, segmented], refresh_snapshot_data)  // call refresh_snapshot_data
export function show_snapshot(snapshot_idx, mwpf_data) {
    active_snapshot_idx.value = snapshot_idx
    active_mwpf_data.value = mwpf_data
}

/*
 * Idea from Katie 2024.10.2: draw each edge branch differently to show which vertices the dual variables contribute
 * 
 * The previous visualization is to display all each hypergraph in deg_v branches, and each branch is identical.
 *     Then for each branch, we print the contribution of all the dual variables. This method is very simple, and essentially
 *     convert the hyperedge printing problem to deg_v number of simple edge printing. However, this method will not 
 *     convey the information of which vertices are the dual variables "flooding" from. For example, a single defect vertex
 *     grows over its adjacent hyperedges, however this method does not allow readers to get the information of which vertex
 *     is growing by looking at the edge along.
 * 
 * We then found a new method to display it better: Since we know the subset of vertices that a dual variable contributes,
 *     namely $e \cap V_S$, we can grow from these vertices and then show the direction of the dual variable. This method
 *     is stable, in a sense that a small change of dual variables corresponds to a small change of the visualization effect,
 *     given that the dual variables have a consistent order (by their indices).
 * 
 * This function outputs an object describing the segments on each edge branch.
 */
function calculate_edge_branch_segmented(snapshot, edge_to_dual_indices, edge_index) {
    // calculate the list of contributing dual variables
    let dual_indices = []
    let edge = snapshot.edges[edge_index]
    if (segmented.value && snapshot.dual_nodes != null) {  // check the non-zero contributing dual variables
        for (let node_index of edge_to_dual_indices[edge_index]) {
            if (snapshot.dual_nodes[node_index].d != 0) {
                dual_indices.push(node_index)
            }
        }
    }
    // the grown value for each edge branch
    let grown_end = Array(edge.v.length).fill(0)
    let grown_center = Array(edge.v.length).fill(0)
    // the contributing dual variables from the end vertex and the center vertex, respectively
    let contributor_end = Array(edge.v.length).fill(null).map(() => Array())
    let contributor_center = Array(edge.v.length).fill(null).map(() => Array())
    // iterate over all dual variables and put them on the edge branches
    let branch_weight = edge.w / edge.v.length
    for (let ni of dual_indices) {
        const node = snapshot.dual_nodes[ni]
        // calculate the contributing vertices of this dual variable: $e \cap V_S$
        let vertices = []
        let v_s = new Set(snapshot.dual_nodes[ni].v)
        for (let [v_eid, v] of edge.v.entries()) {
            if (v_s.has(v)) {
                vertices.push(v_eid)
            }
        }
        console.assert(vertices.length > 0, "contributing dual variable must overlap with at least one end vertex")
        let center_grow = 0  // the amount of growth that must happen at the center because some edge branch is already tight
        let branch_growth = node.d / vertices.length
        // first, grow from end vertices, each with `branch_growth`
        for (let v_eid of vertices) {
            let remain = branch_weight - grown_end[v_eid] - grown_center[v_eid]
            if (branch_growth <= remain) {
                grown_end[v_eid] += branch_growth
                contributor_end[v_eid].push([ni, branch_growth])
            } else {
                grown_end[v_eid] += remain
                contributor_end[v_eid].push([ni, remain])
                center_grow += branch_growth - remain
            }
        }
        // then, grow from center vertices
        while (center_grow > 0) {
            let available_vertices = []
            let min_positive_remain = null
            for (let [v_eid, vi] of edge.v.entries()) {
                let remain = branch_weight - grown_end[v_eid] - grown_center[v_eid]
                if (remain > 0) {
                    available_vertices.push(v_eid)
                    if (min_positive_remain == null) {
                        min_positive_remain = remain
                    } else if (remain < min_positive_remain) {
                        min_positive_remain = remain
                    }
                }
            }
            if (min_positive_remain == null) {
                if (center_grow > 1e-6) {  // tolerance of error
                    console.error(`need to grow from center of ${center_grow} but all branches are fully grown`)
                }
                break
            }
            // in this round, we can only grow `min_positive_remain` on the branches of `available_vertices`
            if (min_positive_remain > center_grow / available_vertices.length) {
                min_positive_remain = center_grow / available_vertices.length
            }
            center_grow -= min_positive_remain * available_vertices.length
            for (let v_eid of available_vertices) {
                grown_center[v_eid] += min_positive_remain
                const center = contributor_center[v_eid]
                // grow from center, potentially merging with existing segments
                if (center.length > 0 && center[center.length - 1][0] == ni) {
                    center[center.length - 1][1] += min_positive_remain
                } else {
                    center.push([ni, min_positive_remain])
                }
            }
        }
    }
    return { grown_end, grown_center, contributor_end, contributor_center }
}


// configurations
const gui = new GUI({ width: 400, title: "render configurations" })
export const show_config = ref(false)
watch(show_config, () => {
    if (show_config.value) {
        gui.domElement.style.display = "block"
    } else {
        gui.domElement.style.display = "none"
    }
}, { immediate: true })
watch(sizes, () => {  // move render configuration GUI to 3D canvas
    // gui.domElement.style.right = sizes.control_bar_width + "px"
    gui.domElement.style.right = 0
}, { immediate: true })
const conf = {
    scene_background: scene.background,
    defect_vertex_color: defect_vertex_material.color,
    defect_vertex_opacity: defect_vertex_material.opacity,
    normal_vertex_color: normal_vertex_material.color,
    normal_vertex_opacity: normal_vertex_material.opacity,
    defect_vertex_outline_color: defect_vertex_outline_material.color,
    defect_vertex_outline_opacity: defect_vertex_outline_material.opacity,
    normal_vertex_outline_color: normal_vertex_outline_material.color,
    normal_vertex_outline_opacity: normal_vertex_outline_material.opacity,
    empty_edge_color: empty_edge_color,
    empty_edge_opacity: empty_edge_opacity,
    grown_edge_color: grown_edge_color,
    grown_edge_opacity: grown_edge_opacity,
    edge_side: edge_side,
    almost_empty_ratio: almost_empty_ratio,
    almost_grown_ratio: almost_grown_ratio,
    subgraph_edge_color: subgraph_edge_material.color,
    subgraph_edge_opacity: subgraph_edge_material.opacity,
    subgraph_edge_side: subgraph_edge_material.side,
    outline_ratio: outline_ratio.value,
    vertex_radius_scale: vertex_radius_scale.value,
    edge_radius_scale: edge_radius_scale.value,
}
const side_options = { "FrontSide": THREE.FrontSide, "BackSide": THREE.BackSide, "DoubleSide": THREE.DoubleSide }
export const controller = {}
window.controller = controller
controller.scene_background = gui.addColor(conf, 'scene_background').onChange(function (value) { scene.background = value })
const vertex_folder = gui.addFolder('vertex')
controller.defect_vertex_color = vertex_folder.addColor(conf, 'defect_vertex_color').onChange(function (value) { defect_vertex_material.color = value })
controller.defect_vertex_opacity = vertex_folder.add(conf, 'defect_vertex_opacity', 0, 1).onChange(function (value) { defect_vertex_material.opacity = Number(value) })
controller.normal_vertex_color = vertex_folder.addColor(conf, 'normal_vertex_color').onChange(function (value) { normal_vertex_material.color = value })
controller.normal_vertex_opacity = vertex_folder.add(conf, 'normal_vertex_opacity', 0, 1).onChange(function (value) { normal_vertex_material.opacity = Number(value) })
const vertex_outline_folder = gui.addFolder('vertex outline')
controller.defect_vertex_outline_color = vertex_outline_folder.addColor(conf, 'defect_vertex_outline_color').onChange(function (value) { defect_vertex_outline_material.color = value })
controller.defect_vertex_outline_opacity = vertex_outline_folder.add(conf, 'defect_vertex_outline_opacity', 0, 1).onChange(function (value) { defect_vertex_outline_material.opacity = Number(value) })
controller.normal_vertex_outline_color = vertex_outline_folder.addColor(conf, 'normal_vertex_outline_color').onChange(function (value) { normal_vertex_outline_material.color = value })
controller.normal_vertex_outline_opacity = vertex_outline_folder.add(conf, 'normal_vertex_outline_opacity', 0, 1).onChange(function (value) { normal_vertex_outline_material.opacity = Number(value) })
const edge_folder = gui.addFolder('edge')
controller.empty_edge_color = edge_folder.addColor(conf, 'empty_edge_color').onChange(function (value) { empty_edge_color = value; update_edge_materials() })
controller.empty_edge_opacity = edge_folder.add(conf, 'empty_edge_opacity', 0, 1).onChange(function (value) { empty_edge_opacity = Number(value); update_edge_materials() })
controller.grown_edge_color = edge_folder.addColor(conf, 'grown_edge_color').onChange(function (value) { grown_edge_color = value; update_edge_materials() })
controller.grown_edge_opacity = edge_folder.add(conf, 'grown_edge_opacity', 0, 1).onChange(function (value) { grown_edge_opacity = Number(value); update_edge_materials() })
controller.edge_side = edge_folder.add(conf, 'edge_side', side_options).onChange(function (value) { edge_side = value; update_edge_materials() })
controller.almost_empty_ratio = edge_folder.add(conf, 'almost_empty_ratio', 0, 1).onChange(function (value) { almost_empty_ratio = Number(value); update_edge_materials() })
controller.almost_grown_ratio = edge_folder.add(conf, 'almost_grown_ratio', 0, 1).onChange(function (value) { almost_grown_ratio = Number(value); update_edge_materials() })
controller.subgraph_edge_color = edge_folder.addColor(conf, 'subgraph_edge_color').onChange(function (value) { subgraph_edge_material.color = value })
controller.subgraph_edge_opacity = edge_folder.add(conf, 'subgraph_edge_opacity', 0, 1).onChange(function (value) { subgraph_edge_material.opacity = Number(value) })
controller.subgraph_edge_side = edge_folder.add(conf, 'subgraph_edge_side', side_options).onChange(function (value) { subgraph_edge_material.side = Number(value) })
const size_folder = gui.addFolder('size')
controller.outline_ratio = size_folder.add(conf, 'outline_ratio', 0.99, 2).onChange(function (value) { outline_ratio.value = Number(value) })
controller.vertex_radius_scale = size_folder.add(conf, 'vertex_radius_scale', 0.1, 5).onChange(function (value) { vertex_radius_scale.value = Number(value) })
controller.edge_radius_scale = size_folder.add(conf, 'edge_radius_scale', 0.1, 10).onChange(function (value) { edge_radius_scale.value = Number(value) })
watch(sizes, () => {
    gui.domElement.style.transform = `scale(${sizes.scale})`
    gui.domElement.style["transform-origin"] = "right top"
}, { immediate: true })

// select logic
const raycaster = new THREE.Raycaster()
const mouse = new THREE.Vector2()
var previous_hover_material = null
export const current_hover = shallowRef(null)
window.current_hover = current_hover
var previous_selected_material = null
export const current_selected = shallowRef(null)
window.current_selected = current_selected
export const show_hover_effect = ref(true)
function is_user_data_valid(user_data) {
    if (user_data == null) return false
    const mwpf_data = active_mwpf_data.value
    const snapshot_idx = active_snapshot_idx.value
    const snapshot = mwpf_data.snapshots[snapshot_idx][1]
    if (user_data.type == "vertex") {
        return user_data.vertex_index < snapshot.vertices.length && snapshot.vertices[user_data.vertex_index] != null
    }
    if (user_data.type == "edge") {
        return user_data.edge_index < snapshot.edges.length && snapshot.edges[user_data.edge_index] != null
    }
    if (user_data.type == "vertices") {
        let is_valid = true
        for (let i = 0; i < user_data.vertices.length && is_valid; ++i) {
            is_valid &= user_data.vertices[i] < snapshot.vertices.length && snapshot.vertices[user_data.vertices[i]] != null
        }
        return is_valid
    }
    if (user_data.type == "edges") {
        let is_valid = true
        for (let i = 0; i < user_data.edges.length && is_valid; ++i) {
            is_valid &= user_data.edges[i] < snapshot.edges.length && snapshot.edges[user_data.edges[i]] != null
        }
        return is_valid
    }
    return false
}
function set_material_with_user_data(user_data, material) {  // return the previous material
    if (user_data.type == "vertex") {
        let vertex_index = user_data.vertex_index
        let vertex_mesh = vertex_meshes[vertex_index]
        let previous_material = vertex_mesh.material
        vertex_mesh.material = material
        return previous_material
    }
    if (user_data.type == "edge") {
        let edge_index = user_data.edge_index
        let edge_vec_mesh = edge_vec_meshes[edge_index]
        let previous_material = []
        for (let [index, mesh] of edge_vec_mesh.entries()) {
            previous_material.push(mesh.material)
            if (Array.isArray(material)) {
                mesh.material = material[index]
            } else {
                mesh.material = material
            }
        }
        return previous_material
    }
    if (user_data.type == "vertices") {
        let previous_material = []
        for (let i = 0; i < user_data.vertices.length; ++i) {
            let vertex_index = user_data.vertices[i]
            let vertex_mesh = vertex_meshes[vertex_index]
            previous_material.push(vertex_mesh.material)
            if (Array.isArray(material)) {
                vertex_mesh.material = material[i]
            } else {
                vertex_mesh.material = material
            }
        }
        return previous_material
    }
    if (user_data.type == "edges") {
        let previous_material_vec = []
        for (let i = 0; i < user_data.edges.length; ++i) {
            let edge_index = user_data.edges[i]
            let edge_vec_mesh = edge_vec_meshes[edge_index]
            let previous_material = []
            previous_material_vec.push(previous_material)
            for (let [index, mesh] of edge_vec_mesh.entries()) {
                previous_material.push(mesh.material)
                if (Array.isArray(material)) {
                    mesh.material = material[i][index]
                } else {
                    mesh.material = material
                }
            }
        }
        return previous_material_vec
    }
    console.error(`unknown type ${user_data.type}`)
}
watch(current_hover, (newVal, oldVal) => {
    // console.log(`${oldVal} -> ${newVal}`)
    if (oldVal != null && previous_hover_material != null) {
        set_material_with_user_data(oldVal, previous_hover_material)
        previous_hover_material = null
    }
    if (newVal != null) {
        previous_hover_material = set_material_with_user_data(newVal, hover_material)
    }
})
watch(current_selected, (newVal, oldVal) => {
    if (newVal != null) {
        current_hover.value = null
    }
    Vue.nextTick(() => {  // wait after hover cleaned its data
        if (oldVal != null && previous_selected_material != null) {
            set_material_with_user_data(oldVal, previous_selected_material)
            previous_selected_material = null
        }
        if (newVal != null) {
            previous_selected_material = set_material_with_user_data(newVal, selected_material)
        }
    })
})
function on_mouse_change(event, is_click) {
    mouse.x = (event.clientX / sizes.canvas_width) * 2 - 1
    mouse.y = - (event.clientY / sizes.canvas_height) * 2 + 1
    raycaster.setFromCamera(mouse, camera.value)
    const intersects = raycaster.intersectObjects(scene.children, false)
    for (let intersect of intersects) {
        if (!intersect.object.visible) continue  // don't select invisible object
        let user_data = intersect.object.userData
        if (user_data.type == null) continue  // doesn't contain enough information
        // swap back to the original material
        if (is_click) {
            current_selected.value = user_data
        } else {
            if (show_hover_effect.value) {
                current_hover.value = user_data
            } else {
                current_hover.value = null
            }
        }
        return
    }
    if (is_click) {
        current_selected.value = null
    } else {
        current_hover.value = null
    }
    return
}
var mousedown_position = null
var is_mouse_currently_down = false
window.addEventListener('mousedown', (event) => {
    if (event.clientX > sizes.canvas_width) return  // don't care events on control panel
    mousedown_position = {
        clientX: event.clientX,
        clientY: event.clientY,
    }
    is_mouse_currently_down = true
})
window.addEventListener('mouseup', (event) => {
    if (event.clientX > sizes.canvas_width) return  // don't care events on control panel
    // to prevent triggering select while moving camera
    if (mousedown_position != null && mousedown_position.clientX == event.clientX && mousedown_position.clientY == event.clientY) {
        on_mouse_change(event, true)
    }
    is_mouse_currently_down = false
})
window.addEventListener('mousemove', (event) => {
    if (event.clientX > sizes.canvas_width) return  // don't care events on control panel
    // to prevent triggering hover while moving camera
    if (!is_mouse_currently_down) {
        on_mouse_change(event, false)
    }
})

// export current scene to high-resolution png, useful when generating figures for publication
// (I tried svg renderer but it doesn't work very well... shaders are poorly supported)
export function render_png(scale = 1) {
    const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true, preserveDrawingBuffer: true, context: webgl_renderer_context() })
    renderer.setSize(sizes.canvas_width * scale, sizes.canvas_height * scale, false)
    renderer.setPixelRatio(window.devicePixelRatio * scale)
    renderer.render(scene, camera.value)
    return renderer.domElement.toDataURL()
}
window.render_png = render_png
export function open_png(data_url) {
    const w = window.open('', '')
    w.document.title = "rendered image"
    w.document.body.style.backgroundColor = "white"
    w.document.body.style.margin = "0"
    const img = new Image()
    img.src = data_url
    img.style = "width: 100%; height: 100%; object-fit: contain;"
    w.document.body.appendChild(img)
}
window.open_png = open_png
export function download_png(data_url) {
    const a = document.createElement('a')
    a.href = data_url.replace("image/png", "image/octet-stream")
    a.download = 'rendered.png'
    a.click()
}
window.download_png = download_png

export async function nodejs_render_png() {  // works only in nodejs
    let context = webgl_renderer_context()
    var pixels = new Uint8Array(context.drawingBufferWidth * context.drawingBufferHeight * 4)
    const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: false, preserveDrawingBuffer: true, context })
    renderer.setSize(sizes.canvas_width, sizes.canvas_height, false)
    renderer.setPixelRatio(window.devicePixelRatio)
    renderer.render(scene, camera.value)
    context.readPixels(0, 0, context.drawingBufferWidth, context.drawingBufferHeight, context.RGBA, context.UNSIGNED_BYTE, pixels)
    return pixels
}

// wait several Vue ticks to make sure all changes have been applied
export async function wait_changes() {
    for (let i = 0; i < 5; ++i) await Vue.nextTick()
}

// https://www.npmjs.com/package/base64-arraybuffer
var chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
function base64_encode(arraybuffer) {
    var bytes = new Uint8Array(arraybuffer), i, len = bytes.length, base64 = ''
    for (i = 0; i < len; i += 3) {
        base64 += chars[bytes[i] >> 2]
        base64 += chars[((bytes[i] & 3) << 4) | (bytes[i + 1] >> 4)]
        base64 += chars[((bytes[i + 1] & 15) << 2) | (bytes[i + 2] >> 6)]
        base64 += chars[bytes[i + 2] & 63]
    }
    if (len % 3 === 2) {
        base64 = base64.substring(0, base64.length - 1) + '='
    }
    else if (len % 3 === 1) {
        base64 = base64.substring(0, base64.length - 2) + '=='
    }
    return base64;
}

// https://javascript.plainenglish.io/union-find-97f0036dff93
class UnionFind {
    constructor(N) {
        this.parent = Array.from({ length: N }, (_, i) => i)
        this.count = new Array(N).fill(1)
    }
    find(x) {
        if (this.parent[x] != x) this.parent[x] = this.find(this.parent[x])
        return this.parent[x]
    }
    union(x, y) {
        const xp = this.find(x), yp = this.find(y)
        if (xp == yp) return
        if (this.count[xp] < this.count[yp]) {
            this.parent[xp] = yp
            this.count[yp] += this.count[xp]
        } else {
            this.parent[yp] = xp
            this.count[xp] += this.count[yp]
        }
    }
}
