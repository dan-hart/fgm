use serde::{Deserialize, Serialize};

/// User information returned by /v1/me
#[derive(Debug, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub handle: String,
    pub img_url: Option<String>,
}

/// File metadata returned by /v1/files/:key
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub name: String,
    pub last_modified: String,
    pub thumbnail_url: Option<String>,
    pub version: String,
    pub document: Document,
    pub components: std::collections::HashMap<String, Component>,
    pub styles: std::collections::HashMap<String, Style>,
}

/// Document structure
#[derive(Debug, Deserialize)]
pub struct Document {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub children: Option<Vec<Node>>,
}

/// Generic node in the Figma document tree
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub children: Option<Vec<Node>>,
    pub absolute_bounding_box: Option<BoundingBox>,
    pub fills: Option<Vec<Paint>>,
    pub strokes: Option<Vec<Paint>>,
    pub style: Option<TypeStyle>,
}

/// Bounding box for a node
#[derive(Debug, Deserialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Paint (fill or stroke)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paint {
    #[serde(rename = "type")]
    pub paint_type: String,
    pub color: Option<Color>,
    pub opacity: Option<f64>,
}

/// RGBA color
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    /// Convert to hex string (#RRGGBB)
    pub fn to_hex(&self) -> String {
        format!(
            "#{:02X}{:02X}{:02X}",
            (self.r * 255.0) as u8,
            (self.g * 255.0) as u8,
            (self.b * 255.0) as u8
        )
    }

    /// Convert to RGB array [r, g, b]
    pub fn to_rgb(&self) -> [u8; 3] {
        [
            (self.r * 255.0) as u8,
            (self.g * 255.0) as u8,
            (self.b * 255.0) as u8,
        ]
    }
}

/// Typography style
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeStyle {
    pub font_family: Option<String>,
    pub font_weight: Option<f64>,
    pub font_size: Option<f64>,
    pub line_height_px: Option<f64>,
    pub letter_spacing: Option<f64>,
}

/// Published component
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub key: String,
    pub name: String,
    pub description: String,
}

/// Style definition
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    pub key: String,
    pub name: String,
    pub style_type: String,
    pub description: Option<String>,
}

/// Image export response
#[derive(Debug, Deserialize)]
pub struct ImageResponse {
    #[serde(default)]
    pub images: std::collections::HashMap<String, Option<String>>,
    pub err: Option<String>,
    pub status: Option<u16>,
}

/// Project info
#[derive(Debug, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
}

/// Projects list response
#[derive(Debug, Deserialize)]
pub struct ProjectsResponse {
    pub projects: Vec<Project>,
}

/// Project file
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub key: String,
    pub name: String,
    pub thumbnail_url: Option<String>,
    pub last_modified: String,
}

/// Project files response
#[derive(Debug, Deserialize)]
pub struct ProjectFilesResponse {
    pub files: Vec<ProjectFile>,
}

/// Version info
#[derive(Debug, Deserialize)]
pub struct Version {
    pub id: String,
    pub created_at: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub user: Option<VersionUser>,
}

#[derive(Debug, Deserialize)]
pub struct VersionUser {
    pub handle: String,
    pub img_url: Option<String>,
}

/// Versions response
#[derive(Debug, Deserialize)]
pub struct VersionsResponse {
    pub versions: Vec<Version>,
}
