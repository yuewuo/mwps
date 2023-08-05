//! Visualizer
//!
//! This module helps visualize the progress of a mwpf module
//!

use crate::chrono::Local;
use crate::serde::{Deserialize, Serialize};
use crate::serde_json;
use crate::urlencoding;
#[cfg(feature = "python_binding")]
use crate::util::*;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};

pub trait MWPSVisualizer {
    /// take a snapshot, set `abbrev` to true to save space
    fn snapshot(&self, abbrev: bool) -> serde_json::Value;
}

#[macro_export]
macro_rules! bind_trait_mwpf_visualizer {
    ($struct_name:ident) => {
        #[cfg(feature = "python_binding")]
        #[pymethods]
        impl $struct_name {
            #[pyo3(name = "snapshot")]
            #[args(abbrev = "true")]
            fn trait_snapshot(&self, abbrev: bool) -> PyObject {
                json_to_pyobject(self.snapshot(abbrev))
            }
        }
    };
}
#[allow(unused_imports)]
pub use bind_trait_mwpf_visualizer;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct VisualizePosition {
    /// vertical axis, -i is up, +i is down (left-up corner is smallest i,j)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub i: f64,
    /// horizontal axis, -j is left, +j is right (left-up corner is smallest i,j)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub j: f64,
    /// time axis, top and bottom (orthogonal to the initial view, which looks at -t direction)
    #[cfg_attr(feature = "python_binding", pyo3(get, set))]
    pub t: f64,
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl VisualizePosition {
    /// create a visualization position
    #[cfg_attr(feature = "python_binding", new)]
    pub fn new(i: f64, j: f64, t: f64) -> Self {
        Self { i, j, t }
    }
    #[cfg(feature = "python_binding")]
    fn __repr__(&self) -> String {
        format!("{:?}", self)
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pyclass)]
pub struct Visualizer {
    /// save to file if applicable
    file: Option<File>,
    /// if waiting for the first snapshot
    empty_snapshot: bool,
    /// names of the snapshots
    #[cfg_attr(feature = "python_binding", pyo3(get))]
    pub snapshots: Vec<String>,
}

pub fn snapshot_fix_missing_fields(value: &mut serde_json::Value, abbrev: bool) {
    let value = value.as_object_mut().expect("snapshot must be an object");
    // fix vertices missing fields
    let vertices = value
        .get_mut("vertices")
        .expect("missing unrecoverable field")
        .as_array_mut()
        .expect("vertices must be an array");
    for vertex in vertices {
        if vertex.is_null() {
            continue;
        } // vertex not present, probably currently don't care
        let vertex = vertex.as_object_mut().expect("each vertex must be an object");
        let key_is_defect = if abbrev { "s" } else { "is_defect" };
        // recover
        if !vertex.contains_key(key_is_defect) {
            vertex.insert(key_is_defect.to_string(), json!(0)); // by default no syndrome
        }
    }
    // fix edges missing fields
    let edges = value
        .get_mut("edges")
        .expect("missing unrecoverable field")
        .as_array_mut()
        .expect("edges must be an array");
    for edge in edges {
        if edge.is_null() {
            continue;
        } // edge not present, probably currently don't care
        let edge = edge.as_object_mut().expect("each edge must be an object");
        let key_weight = if abbrev { "w" } else { "weight" };
        let key_vertices = if abbrev { "v" } else { "vertices" };
        let key_growth = if abbrev { "g" } else { "growth" };
        // recover
        assert!(edge.contains_key(key_weight), "missing unrecoverable field");
        assert!(edge.contains_key(key_vertices), "missing unrecoverable field");
        if !edge.contains_key(key_growth) {
            edge.insert(key_growth.to_string(), json!(0)); // by default no growth
        }
    }
}

pub type ObjectMap = serde_json::Map<String, serde_json::Value>;
pub fn snapshot_combine_object_known_key(obj: &mut ObjectMap, obj_2: &mut ObjectMap, key: &str) {
    match (obj.contains_key(key), obj_2.contains_key(key)) {
        (_, false) => {} // do nothing
        (false, true) => {
            obj.insert(key.to_string(), obj_2.remove(key).unwrap());
        }
        (true, true) => {
            // println!("[snapshot_combine_object_known_key] {}: {:?} == {:?}", key, obj[key], obj_2[key]);
            assert_eq!(
                obj[key], obj_2[key],
                "cannot combine different values: please make sure values don't conflict"
            );
            obj_2.remove(key).unwrap();
        }
    }
}

pub fn snapshot_copy_remaining_fields(obj: &mut ObjectMap, obj_2: &mut ObjectMap) {
    let mut keys = Vec::<String>::new();
    for key in obj_2.keys() {
        keys.push(key.clone());
    }
    for key in keys.iter() {
        match obj.contains_key(key) {
            false => {
                obj.insert(key.to_string(), obj_2.remove(key).unwrap());
            }
            true => {
                // println!("[snapshot_copy_remaining_fields] {}: {:?} == {:?}", key, obj[key], obj_2[key]);
                // println!("obj: {obj:?}");
                // println!("obj_2: {obj_2:?}");
                assert_eq!(
                    obj[key], obj_2[key],
                    "cannot combine unknown fields: don't know what to do, please modify `snapshot_combine_values` function"
                );
                obj_2.remove(key).unwrap();
            }
        }
    }
}

pub fn snapshot_combine_values(value: &mut serde_json::Value, mut value_2: serde_json::Value, abbrev: bool) {
    let value = value.as_object_mut().expect("snapshot must be an object");
    let value_2 = value_2.as_object_mut().expect("snapshot must be an object");
    match (value.contains_key("vertices"), value_2.contains_key("vertices")) {
        (_, false) => {} // do nothing
        (false, true) => {
            value.insert("vertices".to_string(), value_2.remove("vertices").unwrap());
        }
        (true, true) => {
            // combine
            let vertices = value
                .get_mut("vertices")
                .unwrap()
                .as_array_mut()
                .expect("vertices must be an array");
            let vertices_2 = value_2
                .get_mut("vertices")
                .unwrap()
                .as_array_mut()
                .expect("vertices must be an array");
            assert!(vertices.len() == vertices_2.len(), "vertices must be compatible");
            for (vertex_idx, vertex) in vertices.iter_mut().enumerate() {
                let vertex_2 = &mut vertices_2[vertex_idx];
                if vertex_2.is_null() {
                    continue;
                }
                if vertex.is_null() {
                    *vertex = vertex_2.clone();
                    continue;
                }
                // println!("vertex_idx: {vertex_idx}");
                let vertex = vertex.as_object_mut().expect("each vertex must be an object");
                let vertex_2 = vertex_2.as_object_mut().expect("each vertex must be an object");
                // list known keys
                let key_is_virtual = if abbrev { "v" } else { "is_virtual" };
                let key_is_defect = if abbrev { "s" } else { "is_defect" };
                let known_keys = [key_is_virtual, key_is_defect];
                for key in known_keys {
                    snapshot_combine_object_known_key(vertex, vertex_2, key);
                }
                snapshot_copy_remaining_fields(vertex, vertex_2);
                assert_eq!(vertex_2.len(), 0, "there should be nothing left");
            }
            value_2.remove("vertices").unwrap();
        }
    }
    match (value.contains_key("edges"), value_2.contains_key("edges")) {
        (_, false) => {} // do nothing
        (false, true) => {
            value.insert("edges".to_string(), value_2.remove("edges").unwrap());
        }
        (true, true) => {
            // combine
            let edges = value
                .get_mut("edges")
                .unwrap()
                .as_array_mut()
                .expect("edges must be an array");
            let edges_2 = value_2
                .get_mut("edges")
                .unwrap()
                .as_array_mut()
                .expect("edges must be an array");
            assert!(edges.len() == edges_2.len(), "edges must be compatible");
            for (edge_idx, edge) in edges.iter_mut().enumerate() {
                let edge_2 = &mut edges_2[edge_idx];
                if edge_2.is_null() {
                    continue;
                }
                if edge.is_null() {
                    *edge = edge_2.clone();
                    continue;
                }
                let edge = edge.as_object_mut().expect("each edge must be an object");
                let edge_2 = edge_2.as_object_mut().expect("each edge must be an object");
                // list known keys
                let key_weight = if abbrev { "w" } else { "weight" };
                let key_left = if abbrev { "l" } else { "left" };
                let key_right = if abbrev { "r" } else { "right" };
                let key_growth = if abbrev { "g" } else { "growth" };
                let known_keys = [key_weight, key_left, key_right, key_growth];
                for key in known_keys {
                    snapshot_combine_object_known_key(edge, edge_2, key);
                }
                snapshot_copy_remaining_fields(edge, edge_2);
                assert_eq!(edge_2.len(), 0, "there should be nothing left");
            }
            value_2.remove("edges").unwrap();
        }
    }
    snapshot_copy_remaining_fields(value, value_2);
}

#[cfg_attr(feature = "python_binding", pyfunction)]
pub fn center_positions(mut positions: Vec<VisualizePosition>) -> Vec<VisualizePosition> {
    if !positions.is_empty() {
        let mut max_i = positions[0].i;
        let mut min_i = positions[0].i;
        let mut max_j = positions[0].j;
        let mut min_j = positions[0].j;
        let mut max_t = positions[0].t;
        let mut min_t = positions[0].t;
        for position in positions.iter_mut() {
            if position.i > max_i {
                max_i = position.i;
            }
            if position.j > max_j {
                max_j = position.j;
            }
            if position.t > max_t {
                max_t = position.t;
            }
            if position.i < min_i {
                min_i = position.i;
            }
            if position.j < min_j {
                min_j = position.j;
            }
            if position.t < min_t {
                min_t = position.t;
            }
        }
        let (ci, cj, ct) = ((max_i + min_i) / 2., (max_j + min_j) / 2., (max_t + min_t) / 2.);
        for position in positions.iter_mut() {
            position.i -= ci;
            position.j -= cj;
            position.t -= ct;
        }
    }
    positions
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl Visualizer {
    /// create a new visualizer with target filename and node layout
    #[cfg_attr(feature = "python_binding", new)]
    #[cfg_attr(feature = "python_binding", pyo3(signature = (filepath, positions=vec![], center=true)))]
    pub fn new(mut filepath: Option<String>, mut positions: Vec<VisualizePosition>, center: bool) -> std::io::Result<Self> {
        if cfg!(feature = "disable_visualizer") {
            filepath = None; // do not open file
        }
        if center {
            positions = center_positions(positions);
        }
        let mut file = match filepath {
            Some(filepath) => Some(File::create(filepath)?),
            None => None,
        };
        if let Some(file) = file.as_mut() {
            file.set_len(0)?; // truncate the file
            file.seek(SeekFrom::Start(0))?; // move the cursor to the front
            file.write_all(format!("{{\"format\":\"mwpf\",\"version\":\"{}\"", env!("CARGO_PKG_VERSION")).as_bytes())?;
            file.write_all(b",\"positions\":")?;
            file.write_all(json!(positions).to_string().as_bytes())?;
            file.write_all(b",\"snapshots\":[]}")?;
            file.sync_all()?;
        }
        Ok(Self {
            file,
            empty_snapshot: true,
            snapshots: vec![],
        })
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "snapshot_combined")]
    pub fn snapshot_combined_py(&mut self, name: String, object_pys: Vec<&PyAny>) -> std::io::Result<()> {
        if cfg!(feature = "disable_visualizer") {
            return Ok(());
        }
        let mut values = Vec::<serde_json::Value>::with_capacity(object_pys.len());
        for object_py in object_pys.into_iter() {
            values.push(pyobject_to_json(object_py.call_method0("snapshot")?.extract::<PyObject>()?));
        }
        self.snapshot_combined_value(name, values)
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "snapshot")]
    pub fn snapshot_py(&mut self, name: String, object_py: &PyAny) -> std::io::Result<()> {
        if cfg!(feature = "disable_visualizer") {
            return Ok(());
        }
        let value = pyobject_to_json(object_py.call_method0("snapshot")?.extract::<PyObject>()?);
        self.snapshot_value(name, value)
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "snapshot_combined_value")]
    pub fn snapshot_combined_value_py(&mut self, name: String, value_pys: Vec<PyObject>) -> std::io::Result<()> {
        if cfg!(feature = "disable_visualizer") {
            return Ok(());
        }
        let values: Vec<_> = value_pys.into_iter().map(pyobject_to_json).collect();
        self.snapshot_combined_value(name, values)
    }

    #[cfg(feature = "python_binding")]
    #[pyo3(name = "snapshot_value")]
    pub fn snapshot_value_py(&mut self, name: String, value_py: PyObject) -> std::io::Result<()> {
        if cfg!(feature = "disable_visualizer") {
            return Ok(());
        }
        let value = pyobject_to_json(value_py);
        self.snapshot_value(name, value)
    }
}

impl Visualizer {
    pub fn incremental_save(&mut self, name: String, value: serde_json::Value) -> std::io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            self.snapshots.push(name.clone());
            file.seek(SeekFrom::End(-2))?; // move the cursor before the ending ]}
            if !self.empty_snapshot {
                file.write_all(b",")?;
            }
            self.empty_snapshot = false;
            file.write_all(json!((name, value)).to_string().as_bytes())?;
            file.write_all(b"]}")?;
            file.sync_all()?;
        }
        Ok(())
    }

    /// append another snapshot of the mwpf modules, and also update the file in case
    pub fn snapshot_combined(&mut self, name: String, mwpf_algorithms: Vec<&dyn MWPSVisualizer>) -> std::io::Result<()> {
        if cfg!(feature = "disable_visualizer") {
            return Ok(());
        }
        let abbrev = true;
        let mut value = json!({});
        for mwpf_algorithm in mwpf_algorithms.iter() {
            let value_2 = mwpf_algorithm.snapshot(abbrev);
            snapshot_combine_values(&mut value, value_2, abbrev);
        }
        snapshot_fix_missing_fields(&mut value, abbrev);
        self.incremental_save(name, value)?;
        Ok(())
    }

    /// append another snapshot of the mwpf modules, and also update the file in case
    pub fn snapshot(&mut self, name: String, mwpf_algorithm: &impl MWPSVisualizer) -> std::io::Result<()> {
        if cfg!(feature = "disable_visualizer") {
            return Ok(());
        }
        let abbrev = true;
        let mut value = mwpf_algorithm.snapshot(abbrev);
        snapshot_fix_missing_fields(&mut value, abbrev);
        self.incremental_save(name, value)?;
        Ok(())
    }

    pub fn snapshot_combined_value(&mut self, name: String, values: Vec<serde_json::Value>) -> std::io::Result<()> {
        if cfg!(feature = "disable_visualizer") {
            return Ok(());
        }
        let abbrev = true;
        let mut value = json!({});
        for value_2 in values.into_iter() {
            snapshot_combine_values(&mut value, value_2, abbrev);
        }
        snapshot_fix_missing_fields(&mut value, abbrev);
        self.incremental_save(name, value)?;
        Ok(())
    }

    pub fn snapshot_value(&mut self, name: String, mut value: serde_json::Value) -> std::io::Result<()> {
        if cfg!(feature = "disable_visualizer") {
            return Ok(());
        }
        let abbrev = true;
        snapshot_fix_missing_fields(&mut value, abbrev);
        self.incremental_save(name, value)?;
        Ok(())
    }
}

const DEFAULT_VISUALIZE_DATA_FOLDER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/visualize/data/");

// only used locally, because this is compile time directory
pub fn visualize_data_folder() -> String {
    DEFAULT_VISUALIZE_DATA_FOLDER.to_string()
}

#[cfg_attr(feature = "python_binding", pyfunction)]
pub fn static_visualize_data_filename() -> String {
    "visualizer.json".to_string()
}

#[cfg_attr(feature = "python_binding", pyfunction)]
pub fn auto_visualize_data_filename() -> String {
    format!("{}.json", Local::now().format("%Y%m%d-%H-%M-%S%.3f"))
}

#[cfg_attr(feature = "python_binding", pyfunction)]
pub fn print_visualize_link_with_parameters(filename: String, parameters: Vec<(String, String)>) {
    let default_port = if cfg!(feature = "python_binding") { 51672 } else { 8072 };
    let mut link = format!("http://localhost:{}?filename={}", default_port, filename);
    for (key, value) in parameters.iter() {
        link.push('&');
        link.push_str(&urlencoding::encode(key));
        link.push('=');
        link.push_str(&urlencoding::encode(value));
    }
    if cfg!(feature = "python_binding") {
        println!(
            "opening link {} (use `mwpf.open_visualizer(filename)` to start a server and open it in browser)",
            link
        )
    } else {
        println!("opening link {} (start local server by running ./visualize/server.sh) or call `node index.js <link>` to render locally", link)
    }
}

#[cfg_attr(feature = "python_binding", pyfunction)]
pub fn print_visualize_link(filename: String) {
    print_visualize_link_with_parameters(filename, Vec::new())
}

// #[cfg(feature = "python_binding")]
// #[pyfunction]
// pub(crate) fn register(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
//     m.add_class::<VisualizePosition>()?;
//     m.add_class::<Visualizer>()?;
//     m.add_function(wrap_pyfunction!(static_visualize_data_filename, m)?)?;
//     m.add_function(wrap_pyfunction!(auto_visualize_data_filename, m)?)?;
//     m.add_function(wrap_pyfunction!(print_visualize_link_with_parameters, m)?)?;
//     m.add_function(wrap_pyfunction!(print_visualize_link, m)?)?;
//     m.add_function(wrap_pyfunction!(center_positions, m)?)?;
//     Ok(())
// }

#[cfg(test)]
mod tests {}
