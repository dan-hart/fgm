use crate::cli::InitArgs;
use crate::output;
use crate::project::{init_workspace_with_source, WorkspacePlan};
use anyhow::Result;
use colored::Colorize;
use std::io::{self, Write};

pub async fn run(args: InitArgs) -> Result<()> {
    let base_dir = if args.path == std::path::PathBuf::from(".") {
        std::env::current_dir()?
    } else {
        args.path.clone()
    };

    let mut project_name = args
        .name
        .clone()
        .unwrap_or_else(|| infer_project_name(&base_dir));

    if args.interactive && args.name.is_none() {
        print!("Project name [{}]: ", project_name);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if !trimmed.is_empty() {
            project_name = trimmed.to_string();
        }
    }

    let plan = WorkspacePlan::default_in(&base_dir);
    init_workspace_with_source(&plan, &project_name, args.figma.as_deref(), args.force)?;

    output::print_success(&format!(
        "Initialized fgm workspace in {}",
        base_dir.display()
    ));
    output::print_status(&format!("  Project: {}", project_name.bold()));
    output::print_status(&format!("  Config: {}", plan.config_path.display()));
    output::print_status(&format!("  Reports: {}", plan.reports_dir.display()));
    output::print_status(&format!("  Snapshots: {}", plan.snapshots_dir.display()));
    output::print_status(&format!(
        "  Interactive mode: {}",
        if args.interactive {
            "enabled"
        } else {
            "disabled"
        }
    ));

    if let Some(figma) = args.figma {
        output::print_status(&format!("  Figma source: {}", figma.cyan()));
    }

    Ok(())
}

fn infer_project_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "fgm-project".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn infer_project_name_uses_directory_name() {
        assert_eq!(infer_project_name(Path::new("/tmp/demo-app")), "demo-app");
    }
}
