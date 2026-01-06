use crate::api::types::{Color, Node};
use crate::api::FigmaClient;
use crate::auth::get_token;
use crate::cli::{TokenFormat, TokensCommands};
use crate::config::Config;
use crate::output;
use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;

pub async fn run(command: TokensCommands) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;
    let config = Config::load().unwrap_or_default();

    match command {
        TokensCommands::Colors { file_key } => colors(&client, &file_key).await,
        TokensCommands::Typography { file_key } => typography(&client, &file_key).await,
        TokensCommands::Spacing { file_key } => spacing(&client, &file_key).await,
        TokensCommands::Export {
            file_key,
            format,
            output,
        } => export(&client, &file_key, format, output, &config).await,
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
    output::print_status(&format!("Color Styles from: {}", file.name).bold().to_string());

    let mut color_styles: Vec<(&String, &str)> = file
        .styles
        .iter()
        .filter(|(_, s)| s.style_type == "FILL")
        .map(|(k, s)| (k, s.name.as_str()))
        .collect();
    color_styles.sort_by(|a, b| a.1.cmp(b.1));

    output::print_status(&format!("\n{}", "Named Styles:".bold()));
    for (key, name) in &color_styles {
        output::print_status(&format!("  {} ({})", name.cyan(), key.dimmed()));
    }

    let mut unique_colors: HashSet<String> = HashSet::new();
    extract_colors_from_node(&file.document, &mut unique_colors);

    output::print_status(&format!(
        "\n{}",
        format!("Unique Colors Found: {}", unique_colors.len()).bold()
    ));
    let mut colors: Vec<_> = unique_colors.into_iter().collect();
    colors.sort();
    for hex in colors.iter().take(20) {
        output::print_status(&format!("  {}", hex.cyan()));
    }
    if colors.len() > 20 {
        output::print_status(&format!(
            "  {} more...",
            format!("... and {}", colors.len() - 20).dimmed()
        ));
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
    output::print_status(&format!("Typography Styles from: {}", file.name).bold().to_string());

    let mut text_styles: Vec<(&String, &str, Option<&str>)> = file
        .styles
        .iter()
        .filter(|(_, s)| s.style_type == "TEXT")
        .map(|(k, s)| (k, s.name.as_str(), s.description.as_deref()))
        .collect();
    text_styles.sort_by(|a, b| a.1.cmp(b.1));

    output::print_status(&format!("\n{}", "Text Styles:".bold()));
    for (key, name, desc) in &text_styles {
        let mut line = format!("  {} ({})", name.cyan(), key.dimmed());
        if let Some(d) = desc {
            if !d.is_empty() {
                line.push_str(&format!(" - {}", d.dimmed()));
            }
        }
        output::print_status(&line);
    }

    let mut unique_fonts: HashSet<String> = HashSet::new();
    extract_fonts_from_node(&file.document, &mut unique_fonts);

    output::print_status(&format!(
        "\n{}",
        format!("Font Families Used: {}", unique_fonts.len()).bold()
    ));
    let mut fonts: Vec<_> = unique_fonts.into_iter().collect();
    fonts.sort();
    for font in &fonts {
        output::print_status(&format!("  {}", font.cyan()));
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
    fn node_name(&self) -> Option<String>;
}

impl HasStyleAndChildren for crate::api::types::Document {
    fn type_style(&self) -> Option<&crate::api::types::TypeStyle> { None }
    fn node_children(&self) -> Option<&Vec<Node>> { self.children.as_ref() }
    fn node_name(&self) -> Option<String> { Some(self.name.clone()) }
}

impl HasStyleAndChildren for Node {
    fn type_style(&self) -> Option<&crate::api::types::TypeStyle> { self.style.as_ref() }
    fn node_children(&self) -> Option<&Vec<Node>> { self.children.as_ref() }
    fn node_name(&self) -> Option<String> { Some(self.name.clone()) }
}

async fn spacing(client: &FigmaClient, file_key: &str) -> Result<()> {
    let file = client.get_file(file_key).await?;
    output::print_status(&format!("Spacing Analysis from: {}", file.name).bold().to_string());
    output::print_status(
        &"Note: Spacing extraction requires parsing auto-layout properties"
            .yellow()
            .to_string(),
    );
    output::print_status(
        &"      which are not fully exposed in the REST API."
            .yellow()
            .to_string(),
    );
    Ok(())
}

async fn export(
    client: &FigmaClient,
    file_key: &str,
    format: TokenFormat,
    output_path: Option<std::path::PathBuf>,
    config: &Config,
) -> Result<()> {
    let file = client.get_file(file_key).await?;
    output::print_status(&format!("Extracting tokens from: {}", file.name).bold().to_string());

    let mut color_map: HashMap<String, Color> = HashMap::new();
    extract_all_colors(&file.document, &mut color_map);

    let mut colors: Vec<ColorToken> = color_map
        .into_iter()
        .map(|(hex, color)| ColorToken {
            name: color_name_from_hex(&hex),
            hex,
            rgb: color.to_rgb(),
            rgba: [color.r, color.g, color.b, color.a],
        })
        .collect();
    colors.sort_by(|a, b| a.hex.cmp(&b.hex));

    let mut typography: Vec<TypographyToken> = Vec::new();
    extract_typography_tokens(&file.document, &mut typography);
    typography.sort_by(|a, b| a.name.cmp(&b.name));

    let tokens = DesignTokens { colors, typography };

    let output_str = match format {
        TokenFormat::Json => export_json(&tokens)?,
        TokenFormat::Css => export_css(&tokens, &config.tokens.css_prefix),
        TokenFormat::Swift => export_swift(&tokens, &config.tokens.swift_prefix),
        TokenFormat::Kotlin => export_kotlin(&tokens, &config.tokens.swift_prefix),
    };

    if let Some(path) = output_path {
        fs::write(&path, &output_str)?;
        output::print_success(&format!("Exported to: {}", path.display()));
    } else {
        output::print_raw(&format!("\n{}", output_str));
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

fn export_css(tokens: &DesignTokens, prefix: &str) -> String {
    let css_prefix = ensure_css_prefix(prefix);
    let mut css = String::from(":root {\n  /* Colors */\n");

    for color in &tokens.colors {
        let name = color.name.to_lowercase();
        css.push_str(&format!("  {}{}: {};\n", css_prefix, name, color.hex));
    }

    css.push_str("\n  /* Typography */\n");
    for token in &tokens.typography {
        let name = sanitize_token_name(&token.name);
        css.push_str(&format!("  /* {} */\n", token.name));
        if let Some(family) = &token.family {
            css.push_str(&format!(
                "  {}font-{}-family: \"{}\";\n",
                css_prefix, name, family
            ));
        }
        if let Some(size) = token.size {
            css.push_str(&format!(
                "  {}font-{}-size: {:.2}px;\n",
                css_prefix, name, size
            ));
        }
        if let Some(weight) = token.weight {
            css.push_str(&format!(
                "  {}font-{}-weight: {:.0};\n",
                css_prefix, name, weight
            ));
        }
        if let Some(line_height) = token.line_height {
            css.push_str(&format!(
                "  {}font-{}-line-height: {:.2}px;\n",
                css_prefix, name, line_height
            ));
        }
        if let Some(letter_spacing) = token.letter_spacing {
            css.push_str(&format!(
                "  {}font-{}-letter-spacing: {:.2}px;\n",
                css_prefix, name, letter_spacing
            ));
        }
    }

    css.push_str("}\n");
    css
}

fn export_swift(tokens: &DesignTokens, prefix: &str) -> String {
    let type_prefix = sanitize_type_name(prefix);
    let mut swift = String::from("import SwiftUI\n\n");

    swift.push_str(&format!("enum {}Colors {{\n", type_prefix));
    for color in &tokens.colors {
        let name = to_pascal_case(&color.name);
        swift.push_str(&format!(
            "    static let {} = Color(red: {:.3}, green: {:.3}, blue: {:.3})\n",
            name, color.rgba[0], color.rgba[1], color.rgba[2]
        ));
    }
    swift.push_str("}\n\n");

    swift.push_str("struct TypographyToken {\n");
    swift.push_str("    let family: String?\n");
    swift.push_str("    let size: Double?\n");
    swift.push_str("    let weight: Double?\n");
    swift.push_str("    let lineHeight: Double?\n");
    swift.push_str("    let letterSpacing: Double?\n");
    swift.push_str("}\n\n");

    swift.push_str(&format!("enum {}Typography {{\n", type_prefix));
    for token in &tokens.typography {
        let name = to_pascal_case(&token.name);
        swift.push_str(&format!(
            "    static let {} = TypographyToken(family: {}, size: {}, weight: {}, lineHeight: {}, letterSpacing: {})\n",
            name,
            optional_string(&token.family),
            optional_number(token.size),
            optional_number(token.weight),
            optional_number(token.line_height),
            optional_number(token.letter_spacing)
        ));
    }
    swift.push_str("}\n");
    swift
}

fn export_kotlin(tokens: &DesignTokens, prefix: &str) -> String {
    let type_prefix = sanitize_type_name(prefix);
    let mut kotlin =
        String::from("package design.tokens\n\nimport androidx.compose.ui.graphics.Color\n\n");

    kotlin.push_str(&format!("object {}Colors {{\n", type_prefix));
    for color in &tokens.colors {
        let name = to_pascal_case(&color.name);
        let hex = color.hex.trim_start_matches('#');
        kotlin.push_str(&format!("    val {} = Color(0xFF{})\n", name, hex));
    }
    kotlin.push_str("}\n\n");

    kotlin.push_str("data class TypographyToken(\n");
    kotlin.push_str("    val family: String?,\n");
    kotlin.push_str("    val size: Float?,\n");
    kotlin.push_str("    val weight: Float?,\n");
    kotlin.push_str("    val lineHeight: Float?,\n");
    kotlin.push_str("    val letterSpacing: Float?\n");
    kotlin.push_str(")\n\n");

    kotlin.push_str(&format!("object {}Typography {{\n", type_prefix));
    for token in &tokens.typography {
        let name = to_pascal_case(&token.name);
        kotlin.push_str(&format!(
            "    val {} = TypographyToken({}, {}, {}, {}, {})\n",
            name,
            optional_string_kotlin(&token.family),
            optional_number_f32(token.size),
            optional_number_f32(token.weight),
            optional_number_f32(token.line_height),
            optional_number_f32(token.letter_spacing)
        ));
    }
    kotlin.push_str("}\n");
    kotlin
}

fn ensure_css_prefix(prefix: &str) -> String {
    if prefix.starts_with("--") {
        prefix.to_string()
    } else {
        format!("--{}", prefix)
    }
}

fn sanitize_token_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c == ' ' || c == '-' || c == '_' || c == '/' {
                '-'
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn sanitize_type_name(prefix: &str) -> String {
    let mut out = String::new();
    for (i, c) in prefix.chars().enumerate() {
        if c.is_ascii_alphanumeric() {
            if i == 0 && c.is_ascii_digit() {
                out.push('_');
            }
            out.push(c);
        }
    }
    if out.is_empty() {
        "Figma".to_string()
    } else {
        out
    }
}

fn to_pascal_case(name: &str) -> String {
    let mut out = String::new();
    let mut next_upper = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if next_upper {
                out.push(c.to_ascii_uppercase());
                next_upper = false;
            } else {
                out.push(c.to_ascii_lowercase());
            }
        } else {
            next_upper = true;
        }
    }
    if out.is_empty() {
        "Token".to_string()
    } else {
        if out.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            out.insert(0, '_');
        }
        out
    }
}

fn optional_string(value: &Option<String>) -> String {
    match value {
        Some(v) => format!("\"{}\"", v.replace('"', "\\\"")),
        None => "nil".to_string(),
    }
}

fn optional_string_kotlin(value: &Option<String>) -> String {
    match value {
        Some(v) => format!("\"{}\"", v.replace('"', "\\\"")),
        None => "null".to_string(),
    }
}

fn optional_number(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{:.2}", v),
        None => "nil".to_string(),
    }
}

fn optional_number_f32(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{:.2}f", v),
        None => "null".to_string(),
    }
}

fn color_name_from_hex(hex: &str) -> String {
    format!("color-{}", hex.trim_start_matches('#').to_lowercase())
}

#[derive(Hash, Eq, PartialEq)]
struct TypographyKey {
    family: Option<String>,
    size: Option<u64>,
    weight: Option<u64>,
    line_height: Option<u64>,
    letter_spacing: Option<u64>,
}

fn extract_typography_tokens(document: &crate::api::types::Document, tokens: &mut Vec<TypographyToken>) {
    let mut seen: HashSet<TypographyKey> = HashSet::new();
    let mut used_names: HashSet<String> = HashSet::new();
    collect_typography(document, tokens, &mut seen, &mut used_names);
}

fn collect_typography(
    node: &impl HasStyleAndChildren,
    tokens: &mut Vec<TypographyToken>,
    seen: &mut HashSet<TypographyKey>,
    used_names: &mut HashSet<String>,
) {
    if let Some(style) = node.type_style() {
        let key = TypographyKey {
            family: style.font_family.clone(),
            size: style.font_size.map(|v| v.to_bits()),
            weight: style.font_weight.map(|v| v.to_bits()),
            line_height: style.line_height_px.map(|v| v.to_bits()),
            letter_spacing: style.letter_spacing.map(|v| v.to_bits()),
        };
        if !seen.contains(&key) {
            seen.insert(key);
            let base_name = node.node_name().unwrap_or_else(|| "text-style".to_string());
            let mut name = sanitize_token_name(&base_name);
            if name.is_empty() {
                name = "text-style".to_string();
            }
            let mut final_name = name.clone();
            let mut counter = 2;
            while used_names.contains(&final_name) {
                final_name = format!("{}-{}", name, counter);
                counter += 1;
            }
            used_names.insert(final_name.clone());

            tokens.push(TypographyToken {
                name: final_name,
                family: style.font_family.clone(),
                size: style.font_size,
                weight: style.font_weight,
                line_height: style.line_height_px,
                letter_spacing: style.letter_spacing,
            });
        }
    }

    if let Some(children) = node.node_children() {
        for child in children {
            collect_typography(child, tokens, seen, used_names);
        }
    }
}
