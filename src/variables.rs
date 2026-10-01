use anyhow::{bail, Context, Result};
use clap::{Args, ValueEnum};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    Json,
    Css,
    Swift,
    Kotlin,
}

#[derive(Args)]
pub struct VariableArgs {
    #[arg(long, conflicts_with = "import")]
    pub source: Option<String>,
    /// Local Variables REST response JSON (meta.variables + meta.variableCollections)
    #[arg(long)]
    pub import: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "json")]
    pub target: Format,
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

fn resolve(
    id: &str,
    mode: &str,
    vars: &Value,
    collections: &Value,
    seen: &mut BTreeSet<String>,
) -> Result<Value> {
    if !seen.insert(id.to_owned()) {
        bail!("Variable alias cycle at {id}");
    }
    let var = vars.get(id).context(format!(
        "Missing alias target {id}; import referenced library variables too"
    ))?;
    let collection = &collections[var["variableCollectionId"]
        .as_str()
        .context("Missing collection ID")?];
    let values = var["valuesByMode"]
        .as_object()
        .context("Missing valuesByMode")?;
    let value = values
        .get(mode)
        .or_else(|| {
            collection["defaultModeId"]
                .as_str()
                .and_then(|id| values.get(id))
        })
        .context(format!("Missing mode value for {id}"))?;
    let result = if value["type"] == "VARIABLE_ALIAS" {
        resolve(
            value["id"].as_str().context("Missing alias ID")?,
            mode,
            vars,
            collections,
            seen,
        )?
    } else {
        value.clone()
    };
    seen.remove(id);
    Ok(result)
}

fn flatten(input: &Value) -> Result<Vec<Value>> {
    if input["error"] == true {
        bail!("Variables response reports an error");
    }
    let meta = input.get("meta").unwrap_or(input);
    let vars = &meta["variables"];
    let collections = &meta["variableCollections"];
    let mut rows = vec![];
    for (id, var) in vars.as_object().context("Expected meta.variables object")? {
        let cid = var["variableCollectionId"]
            .as_str()
            .context("Variable has no collection")?;
        let collection = collections
            .get(cid)
            .context("Missing variable collection")?;
        if collection
            .get("parentVariableCollectionId")
            .is_some_and(|v| !v.is_null())
        {
            bail!("Extended variable collections are not yet supported; import resolved root collections");
        }
        for mode in collection["modes"]
            .as_array()
            .context("Collection has no modes")?
        {
            let mid = mode["modeId"].as_str().context("Mode has no ID")?;
            let value = resolve(id, mid, vars, collections, &mut BTreeSet::new())?;
            validate(
                &value,
                var["resolvedType"]
                    .as_str()
                    .context("Missing variable type")?,
            )?;
            rows.push(json!({"id":id,"name":var["name"],"collection":collection["name"],"mode":mode["name"],"type":var["resolvedType"],"value":value}));
        }
    }
    rows.sort_by_key(|r| {
        format!(
            "{}:{}:{}:{}",
            r["collection"], r["mode"], r["name"], r["id"]
        )
    });
    Ok(rows)
}

fn validate(value: &Value, kind: &str) -> Result<()> {
    let valid = match kind {
        "COLOR" => ["r", "g", "b", "a"]
            .iter()
            .all(|k| value[k].as_f64().is_some_and(|n| (0.0..=1.0).contains(&n))),
        "FLOAT" => value.is_number(),
        "STRING" => value.is_string(),
        "BOOLEAN" => value.is_boolean(),
        _ => false,
    };
    if !valid {
        bail!("Invalid {kind} variable value");
    }
    Ok(())
}

pub fn identifier(value: &str) -> String {
    // Prefix every identifier to avoid digits and language keywords; hex-escape separators
    // so semantic paths remain distinct (a/b does not collide with a-b).
    let mut name = String::from("token_");
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() {
            name.push(b as char);
        } else {
            name.push_str(&format!("_{b:02x}"));
        }
    }
    name
}

fn literal(value: &Value, kind: &str, format: Format) -> Result<String> {
    Ok(match kind {
        "COLOR" => {
            let [r, g, b, a] = ["r", "g", "b", "a"].map(|k| value[k].as_f64().unwrap());
            match format {
                Format::Swift => format!("Color(red: {r}, green: {g}, blue: {b}, opacity: {a})"),
                Format::Kotlin => {
                    format!("Color(red = {r}f, green = {g}f, blue = {b}f, alpha = {a}f)")
                }
                _ => format!(
                    "rgba({}, {}, {}, {a})",
                    (r * 255.0).round() as u8,
                    (g * 255.0).round() as u8,
                    (b * 255.0).round() as u8
                ),
            }
        }
        "STRING" => {
            let quoted = serde_json::to_string(value.as_str().unwrap())?;
            if matches!(format, Format::Kotlin) {
                native_string(value.as_str().unwrap(), true)
            } else if matches!(format, Format::Swift) {
                native_string(value.as_str().unwrap(), false)
            } else {
                quoted
            }
        }
        "FLOAT" if matches!(format, Format::Kotlin) => format!("{}f", value.as_f64().unwrap()),
        _ => value.to_string(),
    })
}

pub fn native_string(value: &str, kotlin: bool) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '$' if kotlin => out.push_str("\\$"),
            c if c.is_control() => {
                if kotlin {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                } else {
                    out.push_str(&format!("\\u{{{:x}}}", c as u32));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn render(rows: &[Value], target: Format) -> Result<String> {
    if matches!(target, Format::Json) {
        return Ok(serde_json::to_string_pretty(rows)?);
    }
    let mut groups: BTreeMap<(String, String), Vec<&Value>> = BTreeMap::new();
    for row in rows {
        let key = (
            row["collection"]
                .as_str()
                .context("Missing collection name")?
                .to_owned(),
            row["mode"]
                .as_str()
                .context("Missing mode name")?
                .to_owned(),
        );
        groups.entry(key).or_default().push(row);
    }
    let mut out = match target {
        Format::Swift => "import SwiftUI\n\nenum FigmaVariables {\n".to_owned(),
        Format::Kotlin => "package design.tokens\n\nimport androidx.compose.ui.graphics.Color\n\nobject FigmaVariables {\n".to_owned(),
        _ => String::new(),
    };
    for ((collection, mode), rows) in groups {
        let name = format!("{}__{}", identifier(&collection), identifier(&mode));
        out.push_str(&match target {
            Format::Swift => format!("    enum {name} {{\n"),
            Format::Kotlin => format!("    object {name} {{\n"),
            _ => format!("[data-figma-mode=\"{name}\"] {{\n"),
        });
        let mut used = BTreeSet::new();
        for row in rows {
            let name = identifier(row["name"].as_str().context("Missing variable name")?);
            if !used.insert(name.clone()) {
                bail!("Duplicate variable name within collection/mode");
            }
            let value = literal(&row["value"], row["type"].as_str().unwrap(), target)?;
            out.push_str(&match target {
                Format::Swift => format!("        static let {name} = {value}\n"),
                Format::Kotlin => format!("        val {name} = {value}\n"),
                _ => format!("    --{name}: {value};\n"),
            });
        }
        out.push_str("    }\n");
    }
    if matches!(target, Format::Swift | Format::Kotlin) {
        out.push_str("}\n");
    }
    Ok(out)
}

pub async fn run(args: VariableArgs) -> Result<()> {
    let input: Value = if let Some(path) = args.import {
        serde_json::from_str(&fs::read_to_string(path)?)?
    } else {
        let parsed =
            crate::api::FigmaUrl::parse(&crate::workflows::source(args.source.as_deref())?)?;
        let client = crate::api::FigmaClient::new(crate::auth::get_token()?)?;
        client.get_json(&format!("{}/files/{}/variables/local",client.base_url(),parsed.file_key)).await.context("Variables API unavailable: check plan, account seat, scopes and file permissions; use --import with a local Variables JSON response as a fallback")?
    };
    let out = render(&flatten(&input)?, args.target)?;
    if let Some(path) = args.output {
        fs::write(path, out)?;
    } else {
        crate::output::print_raw(&out);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        json!({"meta":{"variableCollections":{"c":{"name":"Theme","defaultModeId":"l","modes":[{"modeId":"l","name":"Light"},{"modeId":"d","name":"Dark"}]}},"variables":{"a":{"name":"surface/default","variableCollectionId":"c","resolvedType":"COLOR","valuesByMode":{"l":{"r":1,"g":1,"b":1,"a":0.5},"d":{"r":0,"g":0,"b":0,"a":1}}},"b":{"name":"surface/alias","variableCollectionId":"c","resolvedType":"COLOR","valuesByMode":{"l":{"type":"VARIABLE_ALIAS","id":"a"},"d":{"type":"VARIABLE_ALIAS","id":"a"}}}}}})
    }
    #[test]
    fn aliases_resolve_per_mode_and_preserve_opacity() {
        let rows = flatten(&fixture()).unwrap();
        assert_eq!(rows.len(), 4);
        let out = render(&rows, Format::Swift).unwrap();
        assert!(out.contains("opacity: 0.5"));
        assert!(out.contains("token_Theme__token_Dark"));
    }
    #[test]
    fn cycles_and_missing_targets_fail() {
        let mut v = fixture();
        v["meta"]["variables"]["a"]["valuesByMode"]["l"] =
            json!({"type":"VARIABLE_ALIAS","id":"b"});
        assert!(flatten(&v).unwrap_err().to_string().contains("cycle"));
        v["meta"]["variables"]["a"]["valuesByMode"]["l"]["id"] = json!("missing");
        assert!(flatten(&v)
            .unwrap_err()
            .to_string()
            .contains("Missing alias"));
    }
    #[test]
    fn identifiers_are_safe_and_distinct() {
        assert_ne!(identifier("a/b"), identifier("a-b"));
        assert_eq!(identifier("class"), "token_class");
        assert_eq!(identifier("123"), "token_123");
    }
    #[test]
    fn generated_output_is_deterministic() {
        let rows = flatten(&fixture()).unwrap();
        for f in [Format::Swift, Format::Kotlin, Format::Css] {
            assert_eq!(render(&rows, f).unwrap(), render(&rows, f).unwrap());
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn variable_swift_output_typechecks() {
        let input: Value =
            serde_json::from_str(include_str!("../tests/fixtures/variables.json")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Variables.swift");
        fs::write(
            &path,
            render(&flatten(&input).unwrap(), Format::Swift).unwrap(),
        )
        .unwrap();
        let out = std::process::Command::new("swiftc")
            .args(["-typecheck", "-module-cache-path"])
            .arg(dir.path().join("cache"))
            .arg(path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    #[ignore = "Requires FGM_KOTLIN_COMPILER_CLASSPATH and FGM_KOTLIN_COMPOSE_CLASSPATH"]
    fn variable_kotlin_output_typechecks() {
        let input: Value =
            serde_json::from_str(include_str!("../tests/fixtures/variables.json")).unwrap();
        super::typecheck_kotlin(&render(&flatten(&input).unwrap(), Format::Kotlin).unwrap());
    }
}

#[cfg(test)]
pub fn typecheck_kotlin(source: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Tokens.kt");
    fs::write(&path, source).unwrap();
    let out = std::process::Command::new("java")
        .arg("-cp")
        .arg(std::env::var("FGM_KOTLIN_COMPILER_CLASSPATH").expect("Set Kotlin compiler classpath"))
        .arg("org.jetbrains.kotlin.cli.jvm.K2JVMCompiler")
        .args(["-no-stdlib", "-no-reflect", "-classpath"])
        .arg(
            std::env::var("FGM_KOTLIN_COMPOSE_CLASSPATH")
                .expect("Set Compose and stdlib classpath"),
        )
        .arg("-d")
        .arg(dir.path().join("classes"))
        .arg(path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
