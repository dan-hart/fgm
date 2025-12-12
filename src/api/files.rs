//! Figma file API endpoints with caching support

use super::cache::{CacheKey, CacheTTL};
use super::client::FigmaClient;
use super::types::*;
use anyhow::Result;

impl FigmaClient {
    /// Get a file by key (with caching)
    ///
    /// Checks cache first, fetches from API if not cached or expired.
    /// Results are cached for 5 minutes.
    pub async fn get_file(&self, file_key: &str) -> Result<File> {
        let cache_key = CacheKey::File(file_key.to_string());

        // Check cache first
        if let Some(cached) = self.cache().get::<File>(&cache_key) {
            return Ok(cached);
        }

        // Fetch from API
        let url = format!("{}/files/{}", self.base_url(), file_key);
        let file: File = self.get_json(&url).await?;

        // Store in cache
        self.cache().set(&cache_key, &file, CacheTTL::FILE_METADATA);

        Ok(file)
    }

    /// Get file with explicit cache control
    ///
    /// # Arguments
    /// * `file_key` - The Figma file key
    /// * `force_refresh` - If true, bypasses cache and fetches fresh data
    pub async fn get_file_cached(&self, file_key: &str, force_refresh: bool) -> Result<File> {
        if force_refresh {
            self.cache()
                .invalidate(&CacheKey::File(file_key.to_string()));
        }
        self.get_file(file_key).await
    }

    /// Get specific nodes from a file
    ///
    /// Node requests are cached based on the file key and node IDs.
    pub async fn get_nodes(
        &self,
        file_key: &str,
        node_ids: &[String],
    ) -> Result<serde_json::Value> {
        let hash = CacheKey::hash_node_ids(node_ids);
        let cache_key = CacheKey::Nodes(file_key.to_string(), hash);

        // Check cache
        if let Some(cached) = self.cache().get::<serde_json::Value>(&cache_key) {
            return Ok(cached);
        }

        // Fetch from API
        let ids = node_ids.join(",");
        let url = format!("{}/files/{}/nodes?ids={}", self.base_url(), file_key, ids);
        let nodes: serde_json::Value = self.get_json(&url).await?;

        // Cache result
        self.cache()
            .set(&cache_key, &nodes, CacheTTL::FILE_METADATA);

        Ok(nodes)
    }

    /// Get file metadata only (lighter endpoint, cached longer)
    pub async fn get_file_meta(&self, file_key: &str) -> Result<serde_json::Value> {
        let cache_key = CacheKey::FileMeta(file_key.to_string());

        if let Some(cached) = self.cache().get::<serde_json::Value>(&cache_key) {
            return Ok(cached);
        }

        let url = format!("{}/files/{}/meta", self.base_url(), file_key);
        let meta: serde_json::Value = self.get_json(&url).await?;

        self.cache()
            .set(&cache_key, &meta, CacheTTL::FILE_META_LIGHT);

        Ok(meta)
    }

    /// Get version history for a file (cached briefly - changes frequently)
    pub async fn get_versions(&self, file_key: &str) -> Result<VersionsResponse> {
        let cache_key = CacheKey::Versions(file_key.to_string());

        if let Some(cached) = self.cache().get::<VersionsResponse>(&cache_key) {
            return Ok(cached);
        }

        let url = format!("{}/files/{}/versions", self.base_url(), file_key);
        let versions: VersionsResponse = self.get_json(&url).await?;

        self.cache().set(&cache_key, &versions, CacheTTL::VERSIONS);

        Ok(versions)
    }

    /// Get projects in a team (cached for 1 hour)
    pub async fn get_team_projects(&self, team_id: &str) -> Result<ProjectsResponse> {
        let cache_key = CacheKey::TeamProjects(team_id.to_string());

        if let Some(cached) = self.cache().get::<ProjectsResponse>(&cache_key) {
            return Ok(cached);
        }

        let url = format!("{}/teams/{}/projects", self.base_url(), team_id);
        let projects: ProjectsResponse = self.get_json(&url).await?;

        self.cache().set(&cache_key, &projects, CacheTTL::TEAM_DATA);

        Ok(projects)
    }

    /// Get files in a project (cached for 1 hour)
    pub async fn get_project_files(&self, project_id: &str) -> Result<ProjectFilesResponse> {
        let cache_key = CacheKey::ProjectFiles(project_id.to_string());

        if let Some(cached) = self.cache().get::<ProjectFilesResponse>(&cache_key) {
            return Ok(cached);
        }

        let url = format!("{}/projects/{}/files", self.base_url(), project_id);
        let files: ProjectFilesResponse = self.get_json(&url).await?;

        self.cache().set(&cache_key, &files, CacheTTL::TEAM_DATA);

        Ok(files)
    }

    /// Get published components in a team library (cached for 30 min)
    pub async fn get_team_components(&self, team_id: &str) -> Result<TeamComponentsResponse> {
        let cache_key = CacheKey::TeamComponents(team_id.to_string());

        if let Some(cached) = self.cache().get::<TeamComponentsResponse>(&cache_key) {
            return Ok(cached);
        }

        let url = format!("{}/teams/{}/components", self.base_url(), team_id);
        let components: TeamComponentsResponse = self.get_json(&url).await?;

        self.cache()
            .set(&cache_key, &components, CacheTTL::COMPONENTS);

        Ok(components)
    }

    /// Get published styles in a team library (cached for 30 min)
    pub async fn get_team_styles(&self, team_id: &str) -> Result<TeamStylesResponse> {
        let cache_key = CacheKey::TeamStyles(team_id.to_string());

        if let Some(cached) = self.cache().get::<TeamStylesResponse>(&cache_key) {
            return Ok(cached);
        }

        let url = format!("{}/teams/{}/styles", self.base_url(), team_id);
        let styles: TeamStylesResponse = self.get_json(&url).await?;

        self.cache().set(&cache_key, &styles, CacheTTL::COMPONENTS);

        Ok(styles)
    }

    /// Get component by key (cached for 30 min)
    pub async fn get_component(&self, component_key: &str) -> Result<ComponentDetailResponse> {
        let cache_key = CacheKey::Component(component_key.to_string());

        if let Some(cached) = self.cache().get::<ComponentDetailResponse>(&cache_key) {
            return Ok(cached);
        }

        let url = format!("{}/components/{}", self.base_url(), component_key);
        let component: ComponentDetailResponse = self.get_json(&url).await?;

        self.cache()
            .set(&cache_key, &component, CacheTTL::COMPONENTS);

        Ok(component)
    }
}
