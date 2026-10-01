use anyhow::{bail, Result};
use std::{path::PathBuf, sync::OnceLock};

static ACCOUNT: OnceLock<String> = OnceLock::new();

pub fn select(name: Option<&str>) -> Result<()> {
    if let Some(name) = name {
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            bail!("Account names must contain 1-64 letters, digits, hyphens or underscores");
        }
        ACCOUNT
            .set(name.to_owned())
            .map_err(|_| anyhow::anyhow!("Account already selected"))?;
    }
    Ok(())
}

pub fn directory(base: PathBuf) -> PathBuf {
    match ACCOUNT.get() {
        Some(name) => base.join("profiles").join(name),
        None => base,
    }
}

pub fn keychain_user() -> String {
    ACCOUNT
        .get()
        .map(|n| format!("figma_token:{n}"))
        .unwrap_or_else(|| "figma_token".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_path_traversal() {
        for name in ["../private", "a/b", "", ".", "a b"] {
            assert!(super::select(Some(name)).is_err());
        }
    }
}
