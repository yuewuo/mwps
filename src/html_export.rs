//! HTML Export
//!
//! This module helps generate standalone HTML files for visualization.
//!

#[cfg(feature = "python_binding")]
use crate::util::*;
use base64::prelude::*;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use include_optional::include_str_optional;
#[cfg(feature = "python_binding")]
use pyo3::prelude::*;
use std::io::prelude::*;

pub struct HTMLExport {}

impl HTMLExport {
    fn begin(name: &str) -> String {
        format!("/* {name}_BEGIN */")
    }

    fn end(name: &str) -> String {
        format!("/* {name}_END */")
    }
}

#[cfg_attr(feature = "python_binding", cfg_eval)]
#[cfg_attr(feature = "python_binding", pymethods)]
impl HTMLExport {
    pub fn get_template_html() -> Option<&'static str> {
        cfg_if::cfg_if! {
            if #[cfg(feature="embed_visualizer")] {
                include_str_optional!("../visualize/dist/standalone.html")
            } else {
                None
            }
        }
    }

    #[cfg_attr(feature = "python_binding", pymethods)]
    fn slice_content<'a>(content: &'a str, name: &str) -> (&'a str, &'a str, &'a str) {
        let begin = Self::begin(name);
        let begin_flag = begin.as_str();
        let end = Self::end(name);
        let end_flag = end.as_str();
        let start_index = content
            .find(begin_flag)
            .unwrap_or_else(|| panic!("begin flag {} not found in content", begin_flag));
        let end_index = content
            .find(end_flag)
            .unwrap_or_else(|| panic!("end flag {} not found in content", end_flag));
        assert!(
            start_index + begin.len() < end_index,
            "start and end flag misplaced in index.html"
        );
        (
            &content[0..start_index],
            &content[start_index + begin.len()..end_index].trim(),
            &content[end_index + end.len()..].trim(),
        )
    }

    pub fn compress_content(data: &str) -> String {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        BASE64_STANDARD.encode(compressed).to_string()
    }

    pub fn decompress_content(base64_str: &str) -> String {
        let compressed = BASE64_STANDARD.decode(base64_str.as_bytes()).unwrap();
        let mut decoder = GzDecoder::new(compressed.as_slice());
        let mut uncompressed = String::new();
        decoder.read_to_string(&mut uncompressed).unwrap();
        uncompressed
    }

    pub fn generate_html(visualizer_data: serde_json::Value, mut override_config: serde_json::Value) -> String {
        let template_html =
            Self::get_template_html().expect("template html not available, please rebuild with `embed_visualizer` feature");
        // force full screen because we're generating standalone html
        override_config
            .as_object_mut()
            .expect("config must be an object")
            .insert("full_screen".to_string(), json!(true));
        let override_str = serde_json::to_string(&override_config).expect("override config must be serializable");
        // compress visualizer data; user can then use the webGUI to export uncompressed JSON or HTML
        let visualizer_json = serde_json::to_string(&visualizer_data).expect("data must be serializable");
        let javascript_data = HTMLExport::compress_content(visualizer_json.as_str());
        // process the frontend code
        let data_flag = "HYPERION_VISUAL_DATA";
        let (vis_data_head, _, vis_data_tail) = Self::slice_content(template_html, data_flag);
        let override_config_flag = "HYPERION_VISUAL_OVERRIDE_CONFIG";
        let (override_head, _, override_tail) = Self::slice_content(vis_data_tail, override_config_flag);
        // construct standalone html
        let new_vis_data_tail = format!(
            "{}\n{}\n{}\n{}\n{}",
            override_head,
            Self::begin(override_config_flag),
            override_str,
            Self::end(override_config_flag),
            override_tail
        );
        let new_html = format!(
            "{}\n{}\n'{}'\n{}\n{}",
            vis_data_head,
            Self::begin(data_flag),
            javascript_data,
            Self::end(data_flag),
            new_vis_data_tail
        );
        new_html
    }
}

#[cfg(all(test, feature = "embed_visualizer"))]
mod tests {
    use super::*;

    #[test]
    fn html_export_compress_js() {
        // cargo test html_export_compress_js -- --nocapture
        let data = "hello world".to_string();
        let compressed = HTMLExport::compress_content(data.as_str());
        println!("compressed: {compressed}");
        let decompressed = HTMLExport::decompress_content(compressed.as_str());
        println!("decompressed: {decompressed}");
        assert_eq!(data, decompressed);
    }
}
