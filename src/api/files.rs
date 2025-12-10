use super::client::FigmaClient;
use super::types::*;
use anyhow::Result;

impl FigmaClient {
    /// Get a file by key
    pub async fn get_file(&self, file_key: &str) -> Result<File> {
        let url = format!("{}/files/{}", self.base_url(), file_key);
        let response = self.http().get(&url).send().await?;
        let file: File = response.json().await?;
        Ok(file)
    }

    /// Get specific nodes from a file
    pub async fn get_nodes(&self, file_key: &str, node_ids: &[String]) -> Result<serde_json::Value> {
        let ids = node_ids.join(",");
        let url = format!("{}/files/{}/nodes?ids={}", self.base_url(), file_key, ids);
        let response = self.http().get(&url).send().await?;
        let nodes: serde_json::Value = response.json().await?;
        Ok(nodes)
    }

    /// Get file metadata only (lighter endpoint)
    pub async fn get_file_meta(&self, file_key: &str) -> Result<serde_json::Value> {
        let url = format!("{}/files/{}/meta", self.base_url(), file_key);
        let response = self.http().get(&url).send().await?;
        let meta: serde_json::Value = response.json().await?;
        Ok(meta)
    }

    /// Get version history for a file
    pub async fn get_versions(&self, file_key: &str) -> Result<VersionsResponse> {
        let url = format!("{}/files/{}/versions", self.base_url(), file_key);
        let response = self.http().get(&url).send().await?;
        let versions: VersionsResponse = response.json().await?;
        Ok(versions)
    }

    /// Get projects in a team
    pub async fn get_team_projects(&self, team_id: &str) -> Result<ProjectsResponse> {
        let url = format!("{}/teams/{}/projects", self.base_url(), team_id);
        let response = self.http().get(&url).send().await?;
        let projects: ProjectsResponse = response.json().await?;
        Ok(projects)
    }

    /// Get files in a project
    pub async fn get_project_files(&self, project_id: &str) -> Result<ProjectFilesResponse> {
        let url = format!("{}/projects/{}/files", self.base_url(), project_id);
        let response = self.http().get(&url).send().await?;
        let files: ProjectFilesResponse = response.json().await?;
        Ok(files)
    }
}
