use anyhow::{anyhow, Result};
use std::io::{self, IsTerminal, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectOption {
    pub id: String,
    pub label: String,
}

pub fn ensure_interactive() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        anyhow::bail!("Interactive selection requires a TTY");
    }
    Ok(())
}

pub fn parse_selection(input: &str, max: usize, allow_multiple: bool) -> Result<Vec<usize>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("No selection provided");
    }

    let mut indexes = Vec::new();
    for part in trimmed.split(',') {
        let idx: usize = part
            .trim()
            .parse()
            .map_err(|_| anyhow!("Invalid selection '{}'", part.trim()))?;
        if idx == 0 || idx > max {
            anyhow::bail!("Selection {} is out of range 1-{}", idx, max);
        }
        indexes.push(idx - 1);
    }

    if !allow_multiple && indexes.len() > 1 {
        anyhow::bail!("Only one selection is allowed");
    }

    indexes.sort_unstable();
    indexes.dedup();
    Ok(indexes)
}

pub fn pick_options(options: &[SelectOption], allow_multiple: bool) -> Result<Vec<SelectOption>> {
    ensure_interactive()?;
    if options.is_empty() {
        anyhow::bail!("No selectable options found");
    }

    println!(
        "Select {}option(s):",
        if allow_multiple { "one or more " } else { "" }
    );
    for (index, option) in options.iter().enumerate() {
        println!("  {}. {} ({})", index + 1, option.label, option.id);
    }
    print!(
        "Enter selection{}: ",
        if allow_multiple {
            "s (comma-separated)"
        } else {
            ""
        }
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let indexes = parse_selection(&input, options.len(), allow_multiple)?;

    Ok(indexes
        .into_iter()
        .map(|idx| options[idx].clone())
        .collect())
}

pub fn top_level_frame_options(document: &crate::api::types::Document) -> Vec<SelectOption> {
    let mut options = Vec::new();
    if let Some(children) = &document.children {
        for page in children {
            if page.node_type != "CANVAS" {
                continue;
            }
            if let Some(nodes) = &page.children {
                for node in nodes {
                    if node.node_type == "FRAME" || node.node_type == "COMPONENT" {
                        options.push(SelectOption {
                            id: node.id.clone(),
                            label: format!("{} / {}", page.name, node.name),
                        });
                    }
                }
            }
        }
    }
    options
}

pub fn component_options(entries: impl Iterator<Item = (String, String)>) -> Vec<SelectOption> {
    entries
        .map(|(id, label)| SelectOption { id, label })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_selection_supports_multiple_values() {
        let parsed = parse_selection("1, 3,2", 4, true).expect("selection should parse");
        assert_eq!(parsed, vec![0, 1, 2]);
    }

    #[test]
    fn parse_selection_rejects_out_of_range_values() {
        let err = parse_selection("5", 3, false).expect_err("selection should fail");
        assert!(err.to_string().contains("out of range"));
    }
}
