use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePlan {
    pub base_dir: PathBuf,
    pub config_path: PathBuf,
    pub state_dir: PathBuf,
    pub reports_dir: PathBuf,
    pub snapshots_dir: PathBuf,
    pub sync_manifest_path: PathBuf,
    pub components_map_path: PathBuf,
}

impl WorkspacePlan {
    pub fn default_in(base_dir: &Path) -> Self {
        let state_dir = base_dir.join(".fgm");
        Self {
            base_dir: base_dir.to_path_buf(),
            config_path: base_dir.join("fgm.toml"),
            reports_dir: state_dir.join("reports"),
            snapshots_dir: state_dir.join("snapshots"),
            sync_manifest_path: state_dir.join("sync.toml"),
            components_map_path: state_dir.join("components.toml"),
            state_dir,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfigFile {
    pub project: ProjectSection,
    #[serde(default)]
    pub export: ProjectExportSection,
    #[serde(default)]
    pub compare: ProjectCompareSection,
    #[serde(default)]
    pub snapshot: ProjectSnapshotSection,
    #[serde(default)]
    pub reports: ProjectReportsSection,
    #[serde(default)]
    pub figma: ProjectFigmaSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSection {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectExportSection {
    #[serde(default = "default_export_output_dir")]
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCompareSection {
    #[serde(default = "default_compare_threshold")]
    pub threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSnapshotSection {
    #[serde(default = "default_snapshot_dir")]
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectReportsSection {
    #[serde(default = "default_reports_dir")]
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectFigmaSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl ProjectConfigFile {
    pub fn starter(project_name: &str) -> Self {
        Self {
            project: ProjectSection {
                name: project_name.to_string(),
            },
            export: ProjectExportSection::default(),
            compare: ProjectCompareSection::default(),
            snapshot: ProjectSnapshotSection::default(),
            reports: ProjectReportsSection::default(),
            figma: ProjectFigmaSection::default(),
        }
    }
}

impl Default for ProjectExportSection {
    fn default() -> Self {
        Self {
            output_dir: default_export_output_dir(),
        }
    }
}

impl Default for ProjectCompareSection {
    fn default() -> Self {
        Self {
            threshold: default_compare_threshold(),
        }
    }
}

impl Default for ProjectSnapshotSection {
    fn default() -> Self {
        Self {
            dir: default_snapshot_dir(),
        }
    }
}

impl Default for ProjectReportsSection {
    fn default() -> Self {
        Self {
            dir: default_reports_dir(),
        }
    }
}

fn default_export_output_dir() -> String {
    "./fgm-exports".to_string()
}

fn default_compare_threshold() -> f32 {
    5.0
}

fn default_snapshot_dir() -> String {
    ".fgm/snapshots".to_string()
}

fn default_reports_dir() -> String {
    ".fgm/reports".to_string()
}

pub fn init_workspace(plan: &WorkspacePlan, project_name: &str, force: bool) -> Result<()> {
    init_workspace_with_source(plan, project_name, None, force)
}

pub fn init_workspace_with_source(
    plan: &WorkspacePlan,
    project_name: &str,
    figma_source: Option<&str>,
    force: bool,
) -> Result<()> {
    fs::create_dir_all(&plan.state_dir)
        .with_context(|| format!("Failed to create {}", plan.state_dir.display()))?;
    fs::create_dir_all(&plan.reports_dir)
        .with_context(|| format!("Failed to create {}", plan.reports_dir.display()))?;
    fs::create_dir_all(&plan.snapshots_dir)
        .with_context(|| format!("Failed to create {}", plan.snapshots_dir.display()))?;

    let mut config = ProjectConfigFile::starter(project_name);
    config.figma.source = figma_source.map(str::to_string);
    write_toml_file(&plan.config_path, &config, force)?;

    let sync_manifest = StarterSyncManifest::starter(project_name);
    write_toml_file(&plan.sync_manifest_path, &sync_manifest, force)?;

    let components_map = StarterComponentsMap::default();
    write_toml_file(&plan.components_map_path, &components_map, force)?;

    Ok(())
}

pub fn find_project_file(start_dir: &Path) -> Option<PathBuf> {
    let mut current = Some(start_dir);
    while let Some(dir) = current {
        let candidate = dir.join("fgm.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

fn write_toml_file<T: Serialize>(path: &Path, value: &T, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(value)?;
    fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StarterSyncManifest {
    project: StarterSyncProject,
}

impl StarterSyncManifest {
    fn starter(project_name: &str) -> Self {
        Self {
            project: StarterSyncProject {
                name: project_name.to_string(),
                output_dir: "./fgm-exports".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StarterSyncProject {
    name: String,
    output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StarterComponentsMap {
    #[serde(default)]
    components: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_workspace_plan_uses_fgm_conventions() {
        let base = tempdir().expect("tempdir");
        let plan = WorkspacePlan::default_in(base.path());

        assert_eq!(plan.config_path, base.path().join("fgm.toml"));
        assert_eq!(plan.state_dir, base.path().join(".fgm"));
        assert_eq!(plan.reports_dir, base.path().join(".fgm/reports"));
        assert_eq!(plan.snapshots_dir, base.path().join(".fgm/snapshots"));
    }

    #[test]
    fn init_workspace_creates_expected_files() {
        let base = tempdir().expect("tempdir");
        let plan = WorkspacePlan::default_in(base.path());

        init_workspace(&plan, "demo-project", false).expect("workspace init should succeed");

        assert!(plan.config_path.exists());
        assert!(plan.state_dir.exists());
        assert!(plan.reports_dir.exists());
        assert!(plan.snapshots_dir.exists());
        assert!(plan.sync_manifest_path.exists());
        assert!(plan.components_map_path.exists());
    }

    #[test]
    fn finds_project_file_in_parent_directory() {
        let base = tempdir().expect("tempdir");
        let nested = base.path().join("nested").join("more");
        fs::create_dir_all(&nested).expect("nested dirs");
        fs::write(base.path().join("fgm.toml"), "test = true").expect("fgm.toml");

        let found = find_project_file(&nested).expect("project file should be found");
        assert_eq!(found, base.path().join("fgm.toml"));
    }
}
