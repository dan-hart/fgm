use crate::cli::PreviewArgs;
use anyhow::Result;
use colored::Colorize;

pub async fn run(args: PreviewArgs) -> Result<()> {
    println!("{}", "Terminal image preview".bold());
    println!("  File: {}", args.file_key);
    if let Some(node) = &args.node {
        println!("  Node: {}", node);
    }
    println!();
    println!(
        "{}",
        "Note: Terminal image preview coming in Phase 5".yellow()
    );
    println!("This feature will use viuer for multi-protocol terminal image rendering.");
    println!("Supported protocols: Sixel, iTerm2, Kitty graphics, ASCII fallback");
    Ok(())
}
