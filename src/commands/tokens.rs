use crate::api::types::{Color, Node};
use crate::api::FigmaClient;
use crate::auth::get_token;
use crate::cli::{TokenFormat, TokensCommands};
use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;

pub async fn run(command: TokensCommands) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    match command {
        TokensCommands::Colors { file_key } => colors(&client, &file_key).await,
        TokensCommands::Typography { file_key } => typography(&client, &file_key).await,
        TokensCommands::Spacing { file_key } => spacing(&client, &file_key).await,
        TokensCommands::Export {
            file_key,
            format,
            output,
        } => export(&client, &file_key, format, output).await,
    }
}

#[derive(Debug, Clone, Serialize)]
struct ColorToken {
    name: String,
    hex: String,
    rgb: [u8; 3],
    rgba: [f64; 4],
}

#[derive(Debug, Clone, Serialize)]
struct TypographyToken {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    letter_spacing: Option<f64>,
}

#[derive(Debug, Serialize)]
struct DesignTokens {
    colors: Vec<ColorToken>,
    typography: Vec<TypographyToken>,
}

async fn colors(client: &FigmaClient, file_key: &str) -> Result<()> {
    let file = client.get_file(file_key).await?;
    println!("{}", format!("Color Styles from: {}", file.name).bold());

    // Extract colors from style definitions
    let mut color_styles: Vec<(&String, &str)> = file
        .styles
        .iter()
        .filter(|(_, s)| s.style_type == "FILL")
        .map(|(k, s)| (k, s.name.as_str()))
        .collect();
    color_styles.sort_by(|a, b| a.1.cmp(b.1));

    println!("\n{}", "Named Styles:".bold());
    for (key, name) in &color_styles {
        println!("  {} ({})", name.cyan(), key.dimmed());
    }

    // Extract unique colors from document
    let mut unique_colors: HashSet<String> = HashSet::new();
    extract_colors_from_node(&file.document, &mut unique_colors);

    println!("\n{}", format!("Unique Colors Found: {}", unique_colors.len()).bold());
    let mut colors: Vec<_> = unique_colors.into_iter().collect();
    colors.sort();
    for hex in colors.iter().take(20) {
        println!("  {}", hex.cyan());
    }
    if colors.len() > 20 {
        println!("  {} more...", format!("... and {}", colors.len() - 20).dimmed());
    }

    Ok(())
}

fn extract_colors_from_node(node: &impl HasFillsAndChildren, colors: &mut HashSet<String>) {
    if let Some(fills) = node.fills() {
        for fill in fills {
            if fill.paint_type == "SOLID" {
                if let Some(color) = &fill.color {
                    colors.insert(color.to_hex());
                }
            }
        }
    }
    if let Some(children) = node.children() {
        for child in children {
            extract_colors_from_node(child, colors);
        }
    }
}

trait HasFillsAndChildren {
    fn fills(&self) -> Option<&Vec<crate::api::types::Paint>>;
    fn children(&self) -> Option<&Vec<Node>>;
}

impl HasFillsAndChildren for crate::api::types::Document {
    fn fills(&self) -> Option<&Vec<crate::api::types::Paint>> { None }
    fn children(&self) -> Option<&Vec<Node>> { self.children.as_ref() }
}

impl HasFillsAndChildren for Node {
    fn fills(&self) -> Option<&Vec<crate::api::types::Paint>> { self.fills.as_ref() }
    fn children(&self) -> Option<&Vec<Node>> { self.children.as_ref() }
}

async fn typography(client: &FigmaClient, file_key: &str) -> Result<()> {
    let file = client.get_file(file_key).await?;
    println!("{}", format!("Typography Styles from: {}", file.name).bold());

    // Extract typography from style definitions
    let mut text_styles: Vec<(&String, &str, Option<&str>)> = file
        .styles
        .iter()
        .filter(|(_, s)| s.style_type == "TEXT")
        .map(|(k, s)| (k, s.name.as_str(), s.description.as_deref()))
        .collect();
    text_styles.sort_by(|a, b| a.1.cmp(b.1));

    println!("\n{}", "Text Styles:".bold());
    for (key, name, desc) in &text_styles {
        print!("  {} ({})", name.cyan(), key.dimmed());
        if let Some(d) = desc {
            if !d.is_empty() {
                print!(" - {}", d.dimmed());
            }
        }
        println!();
    }

    // Extract unique typography from document
    let mut unique_fonts: HashSet<String> = HashSet::new();
    extract_fonts_from_node(&file.document, &mut unique_fonts);

    println!("\n{}", format!("Font Families Used: {}", unique_fonts.len()).bold());
    let mut fonts: Vec<_> = unique_fonts.into_iter().collect();
    fonts.sort();
    for font in &fonts {
        println!("  {}", font.cyan());
    }

    Ok(())
}

fn extract_fonts_from_node(node: &impl HasStyleAndChildren, fonts: &mut HashSet<String>) {
    if let Some(style) = node.type_style() {
        if let Some(family) = &style.font_family {
            fonts.insert(family.clone());
        }
    }
    if let Some(children) = node.node_children() {
        for child in children {
            extract_fonts_from_node(child, fonts);
        }
    }
}

trait HasStyleAndChildren {
    fn type_style(&self) -> Option<&crate::api::types::TypeStyle>;
    fn node_children(&self) -> Option<&Vec<Node>>;
}

impl HasStyleAndChildren for crate::api::types::Document {
    fn type_style(&self) -> Option<&crate::api::types::TypeStyle> { None }
    fn node_children(&self) -> Option<&Vec<Node>> { self.children.as_ref() }
}

impl HasStyleAndChildren for Node {
    fn type_style(&self) -> Option<&crate::api::types::TypeStyle> { self.style.as_ref() }
    fn node_children(&self) -> Option<&Vec<Node>> { self.children.as_ref() }
}

async fn spacing(client: &FigmaClient, file_key: &str) -> Result<()> {
    let file = client.get_file(file_key).await?;
    println!("{}", format!("Spacing Analysis from: {}", file.name).bold());
    println!("{}", "Note: Spacing extraction requires parsing auto-layout properties".yellow());
    println!("{}", "      which are not fully exposed in the REST API.".yellow());
    Ok(())
}

async fn export(
    client: &FigmaClient,
    file_key: &str,
    format: TokenFormat,
    output: Option<std::path::PathBuf>,
) -> Result<()> {
    let file = client.get_file(file_key).await?;
    println!("{}", format!("Extracting tokens from: {}", file.name).bold());

    // Extract colors
    let mut color_map: HashMap<String, Color> = HashMap::new();
    extract_all_colors(&file.document, &mut color_map);

    let colors: Vec<ColorToken> = color_map
        .into_iter()
        .map(|(hex, color)| ColorToken {
            name: hex.clone(),
            hex,
            rgb: color.to_rgb(),
            rgba: [color.r, color.g, color.b, color.a],
        })
        .collect();

    // Extract typography styles (just names from styles for now)
    let typography: Vec<TypographyToken> = file
        .styles
        .iter()
        .filter(|(_, s)| s.style_type == "TEXT")
        .map(|(_, s)| TypographyToken {
            name: s.name.clone(),
            family: None,
            size: None,
            weight: None,
            line_height: None,
            letter_spacing: None,
        })
        .collect();

    let tokens = DesignTokens { colors, typography };

    let output_str = match format {
        TokenFormat::Json => export_json(&tokens)?,
        TokenFormat::Css => export_css(&tokens),
        TokenFormat::Swift => export_swift(&tokens),
        TokenFormat::Kotlin => export_kotlin(&tokens),
    };

    if let Some(path) = output {
        fs::write(&path, &output_str)?;
        println!("{}", format!("Exported to: {}", path.display()).green());
    } else {
        println!("\n{}", output_str);
    }

    Ok(())
}

fn extract_all_colors(node: &impl HasFillsAndChildren, colors: &mut HashMap<String, Color>) {
    if let Some(fills) = node.fills() {
        for fill in fills {
            if fill.paint_type == "SOLID" {
                if let Some(color) = &fill.color {
                    colors.insert(color.to_hex(), color.clone());
                }
            }
        }
    }
    if let Some(children) = node.children() {
        for child in children {
            extract_all_colors(child, colors);
        }
    }
}

fn export_json(tokens: &DesignTokens) -> Result<String> {
    Ok(serde_json::to_string_pretty(tokens)?)
}

fn export_css(tokens: &DesignTokens) -> String {
    let mut css = String::from(":root {\n  /* Colors */\n");

    for (i, color) in tokens.colors.iter().enumerate() {
        let name = format!("color-{}", i + 1);
        css.push_str(&format!("  --figma-{}: {};\n", name, color.hex));
    }

    css.push_str("\n  /* Typography */\n");
    for token in &tokens.typography {
        let name = token.name.to_lowercase().replace(' ', "-").replace('/', "-");
        css.push_str(&format!("  /* {} */\n", token.name));
        css.push_str(&format!("  --figma-font-{}: inherit;\n", name));
    }

    css.push_str("}\n");
    css
}

fn export_swift(tokens: &DesignTokens) -> String {
    let mut swift = String::from("import SwiftUI\n\nextension Color {\n    enum Figma {\n");

    for (i, color) in tokens.colors.iter().enumerate() {
        let name = format!("color{}", i + 1);
        swift.push_str(&format!(
            "        static let {} = Color(red: {:.3}, green: {:.3}, blue: {:.3})\n",
            name, color.rgba[0], color.rgba[1], color.rgba[2]
        ));
    }

    swift.push_str("    }\n}\n");
    swift
}

fn export_kotlin(tokens: &DesignTokens) -> String {
    let mut kotlin = String::from("package design.tokens\n\nimport androidx.compose.ui.graphics.Color\n\nobject FigmaColors {\n");

    for (i, color) in tokens.colors.iter().enumerate() {
        let name = format!("Color{}", i + 1);
        let hex = color.hex.trim_start_matches('#');
        kotlin.push_str(&format!("    val {} = Color(0xFF{})\n", name, hex));
    }

    kotlin.push_str("}\n");
    kotlin
}
