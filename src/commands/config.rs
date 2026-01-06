use crate::cli::ConfigCommands;
use crate::config::Config;
use crate::output;
use anyhow::{anyhow, Result};

pub async fn run(command: ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Show => show().await,
        ConfigCommands::Path => path().await,
        ConfigCommands::Get { key } => get(&key).await,
        ConfigCommands::Set { key, value, unset } => set(&key, value.as_deref(), unset).await,
    }
}

async fn show() -> Result<()> {
    let mut config = Config::load()?;
    if config.figma_token.is_some() {
        config.figma_token = Some("***".to_string());
    }
    let toml = toml::to_string_pretty(&config)?;
    output::print_raw(&toml);
    Ok(())
}

async fn path() -> Result<()> {
    match Config::config_path() {
        Some(path) => {
            output::print_raw(&path.display().to_string());
            Ok(())
        }
        None => Err(anyhow!("Could not determine config path")),
    }
}

async fn get(key: &str) -> Result<()> {
    let config = Config::load()?;
    let value = get_value(&config, key)?;
    output::print_raw(&value);
    Ok(())
}

async fn set(key: &str, value: Option<&str>, unset: bool) -> Result<()> {
    let mut config = Config::load()?;
    set_value(&mut config, key, value, unset)?;
    config.save()?;
    output::print_success("Config updated");
    Ok(())
}

fn get_value(config: &Config, key: &str) -> Result<String> {
    match normalize_key(key).as_str() {
        "defaults.team_id" => Ok(config.defaults.team_id.clone().unwrap_or_default()),
        "defaults.output_format" => Ok(config.defaults.output_format.clone()),
        "defaults.image_protocol" => Ok(config.defaults.image_protocol.clone()),
        "export.default_format" => Ok(config.export.default_format.clone()),
        "export.default_scale" => Ok(config.export.default_scale.to_string()),
        "export.output_dir" => Ok(config.export.output_dir.clone().unwrap_or_default()),
        "tokens.css_prefix" => Ok(config.tokens.css_prefix.clone()),
        "tokens.swift_prefix" => Ok(config.tokens.swift_prefix.clone()),
        _ => Err(anyhow!("Unknown config key: {}", key)),
    }
}

fn set_value(config: &mut Config, key: &str, value: Option<&str>, unset: bool) -> Result<()> {
    match normalize_key(key).as_str() {
        "defaults.team_id" => {
            if unset {
                config.defaults.team_id = None;
            } else {
                let v = value.ok_or_else(|| anyhow!("Value is required"))?;
                config.defaults.team_id = Some(v.to_string());
            }
        }
        "defaults.output_format" => {
            let v = value.ok_or_else(|| anyhow!("Value is required"))?;
            let v = v.to_lowercase();
            if v != "table" && v != "json" {
                return Err(anyhow!("Invalid output format: {}", v));
            }
            config.defaults.output_format = v;
        }
        "defaults.image_protocol" => {
            let v = value.ok_or_else(|| anyhow!("Value is required"))?.to_lowercase();
            if !matches!(v.as_str(), "auto" | "sixel" | "iterm" | "kitty") {
                return Err(anyhow!("Invalid image protocol: {}", v));
            }
            config.defaults.image_protocol = v;
        }
        "export.default_format" => {
            let v = value.ok_or_else(|| anyhow!("Value is required"))?.to_lowercase();
            if !matches!(v.as_str(), "png" | "svg" | "pdf" | "jpg") {
                return Err(anyhow!("Invalid export format: {}", v));
            }
            config.export.default_format = v;
        }
        "export.default_scale" => {
            let v = value.ok_or_else(|| anyhow!("Value is required"))?;
            let scale: f32 = v.parse().map_err(|_| anyhow!("Invalid scale: {}", v))?;
            if !(1.0..=4.0).contains(&scale) {
                return Err(anyhow!("Scale must be between 1 and 4"));
            }
            config.export.default_scale = scale;
        }
        "export.output_dir" => {
            if unset {
                config.export.output_dir = None;
            } else {
                let v = value.ok_or_else(|| anyhow!("Value is required"))?;
                config.export.output_dir = Some(v.to_string());
            }
        }
        "tokens.css_prefix" => {
            let v = value.ok_or_else(|| anyhow!("Value is required"))?;
            config.tokens.css_prefix = v.to_string();
        }
        "tokens.swift_prefix" => {
            let v = value.ok_or_else(|| anyhow!("Value is required"))?;
            config.tokens.swift_prefix = v.to_string();
        }
        _ => return Err(anyhow!("Unknown config key: {}", key)),
    }

    Ok(())
}

fn normalize_key(key: &str) -> String {
    key.trim()
        .to_lowercase()
        .replace('-', "_")
        .replace(' ', "")
}
