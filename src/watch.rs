use crate::api::FigmaClient;
use crate::output;
use anyhow::Result;
use std::future::Future;
use std::time::Duration;

pub fn validate_watch_interval(seconds: u64) -> Result<Duration> {
    if seconds == 0 {
        anyhow::bail!("Watch interval must be at least 1 second");
    }
    Ok(Duration::from_secs(seconds))
}

pub fn should_rerun(previous: Option<&str>, current: &str) -> bool {
    match previous {
        Some(previous) => previous != current,
        None => true,
    }
}

pub async fn fetch_file_version(client: &FigmaClient, file_key: &str) -> Result<String> {
    Ok(client.get_file(file_key).await?.version)
}

pub async fn watch_file_changes<F, Fut>(
    client: &FigmaClient,
    file_key: &str,
    interval_secs: u64,
    mut on_change: F,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let interval = validate_watch_interval(interval_secs)?;
    let mut last_version = fetch_file_version(client, file_key).await?;
    output::print_status(&format!(
        "Watching Figma file {} every {}s for version changes...",
        file_key, interval_secs
    ));

    loop {
        tokio::time::sleep(interval).await;
        let current_version = fetch_file_version(client, file_key).await?;
        if should_rerun(Some(&last_version), &current_version) {
            output::print_status(&format!(
                "Detected new file version {} -> {}",
                last_version, current_version
            ));
            last_version = current_version;
            on_change().await?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_interval_rejects_zero() {
        assert!(validate_watch_interval(0).is_err());
    }

    #[test]
    fn rerun_detects_version_change() {
        assert!(should_rerun(Some("v1"), "v2"));
        assert!(!should_rerun(Some("v1"), "v1"));
    }
}
