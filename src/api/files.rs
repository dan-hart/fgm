//! Figma file API endpoints with caching support

use super::cache::{CacheEntryFreshness, CacheKey, CacheTTL};
use super::client::FigmaClient;
use super::types::*;
use anyhow::Result;
use std::time::Duration;

impl FigmaClient {
    async fn get_cached_endpoint<T>(
        &self,
        cache_key: CacheKey,
        url: String,
        ttl: Duration,
    ) -> Result<T>
    where
        T: for<'de> serde::Deserialize<'de> + serde::Serialize + Send + 'static,
    {
        if let Some((cached, freshness)) = self.cache().get_with_freshness::<T>(&cache_key) {
            match freshness {
                CacheEntryFreshness::Fresh => return Ok(cached),
                CacheEntryFreshness::Stale if self.stale_while_revalidate_enabled() => {
                    let client = self.clone();
                    let refresh_key = cache_key.clone();
                    let refresh_url = url.clone();
                    tokio::spawn(async move {
                        if let Ok(fresh) = client.get_json::<T>(&refresh_url).await {
                            client.cache().set(&refresh_key, &fresh, ttl);
                        }
                    });
                    return Ok(cached);
                }
                CacheEntryFreshness::Stale => {}
            }
        }

        let singleflight_key = format!("api:{}", cache_key.as_string());
        self.run_singleflight(singleflight_key, || async move {
            if let Some(cached) = self.cache().get::<T>(&cache_key) {
                return Ok(cached);
            }

            let fresh: T = self.get_json(&url).await?;
            self.cache().set(&cache_key, &fresh, ttl);
            Ok(fresh)
        })
        .await
    }

    fn canonical_node_ids(node_ids: &[String]) -> Vec<String> {
        let mut ids: Vec<String> = node_ids
            .iter()
            .map(|id| {
                let mut normalized = id.trim().to_string();
                if normalized.contains("%3A") || normalized.contains("%3a") {
                    normalized = normalized.replace("%3A", ":").replace("%3a", ":");
                }
                if normalized.contains('-') && !normalized.contains(':') {
                    normalized = normalized.replace('-', ":");
                }
                normalized
            })
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Get a file by key (with caching)
    ///
    /// Checks cache first, fetches from API if not cached or expired.
    /// Results are cached for 5 minutes.
    pub async fn get_file(&self, file_key: &str) -> Result<File> {
        let cache_key = CacheKey::File(file_key.to_string());
        let url = format!("{}/files/{}", self.base_url(), file_key);
        self.get_cached_endpoint(cache_key, url, CacheTTL::FILE_METADATA)
            .await
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
        let canonical_ids = Self::canonical_node_ids(node_ids);
        let hash = CacheKey::hash_node_ids(&canonical_ids);
        let cache_key = CacheKey::Nodes(file_key.to_string(), hash);
        let ids = canonical_ids.join(",");
        let url = format!("{}/files/{}/nodes?ids={}", self.base_url(), file_key, ids);
        self.get_cached_endpoint(cache_key, url, CacheTTL::FILE_METADATA)
            .await
    }

    /// Get file metadata only (lighter endpoint, cached longer)
    pub async fn get_file_meta(&self, file_key: &str) -> Result<serde_json::Value> {
        let cache_key = CacheKey::FileMeta(file_key.to_string());
        let url = format!("{}/files/{}/meta", self.base_url(), file_key);
        self.get_cached_endpoint(cache_key, url, CacheTTL::FILE_META_LIGHT)
            .await
    }

    /// Get version history for a file (cached briefly - changes frequently)
    pub async fn get_versions(&self, file_key: &str) -> Result<VersionsResponse> {
        let cache_key = CacheKey::Versions(file_key.to_string());
        let url = format!("{}/files/{}/versions", self.base_url(), file_key);
        self.get_cached_endpoint(cache_key, url, CacheTTL::VERSIONS)
            .await
    }

    /// Get projects in a team (cached for 1 hour)
    pub async fn get_team_projects(&self, team_id: &str) -> Result<ProjectsResponse> {
        let cache_key = CacheKey::TeamProjects(team_id.to_string());
        let url = format!("{}/teams/{}/projects", self.base_url(), team_id);
        self.get_cached_endpoint(cache_key, url, CacheTTL::TEAM_DATA)
            .await
    }

    /// Get files in a project (cached for 1 hour)
    pub async fn get_project_files(&self, project_id: &str) -> Result<ProjectFilesResponse> {
        let cache_key = CacheKey::ProjectFiles(project_id.to_string());
        let url = format!("{}/projects/{}/files", self.base_url(), project_id);
        self.get_cached_endpoint(cache_key, url, CacheTTL::TEAM_DATA)
            .await
    }

    /// Get published components in a team library (cached for 30 min)
    pub async fn get_team_components(&self, team_id: &str) -> Result<TeamComponentsResponse> {
        let cache_key = CacheKey::TeamComponents(team_id.to_string());
        let url = format!("{}/teams/{}/components", self.base_url(), team_id);
        self.get_cached_endpoint(cache_key, url, CacheTTL::COMPONENTS)
            .await
    }

    /// Get published styles in a team library (cached for 30 min)
    pub async fn get_team_styles(&self, team_id: &str) -> Result<TeamStylesResponse> {
        let cache_key = CacheKey::TeamStyles(team_id.to_string());
        let url = format!("{}/teams/{}/styles", self.base_url(), team_id);
        self.get_cached_endpoint(cache_key, url, CacheTTL::COMPONENTS)
            .await
    }

    /// Get component by key (cached for 30 min)
    pub async fn get_component(&self, component_key: &str) -> Result<ComponentDetailResponse> {
        let cache_key = CacheKey::Component(component_key.to_string());
        let url = format!("{}/components/{}", self.base_url(), component_key);
        self.get_cached_endpoint(cache_key, url, CacheTTL::COMPONENTS)
            .await
    }
}
