use crate::models::VpnProfile;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppState {
    pub active_profile: Option<VpnProfile>,
    pub pid: Option<u32>,
}

pub struct StateManager {
    path: PathBuf,
}

impl StateManager {
    pub fn new() -> Self {
        let base_dirs = directories::ProjectDirs::from("com", "tuneli", "tuneli-tui")
            .expect("Failed to get project directories");
        let path = base_dirs.data_local_dir().join("state.json");
        Self { path }
    }

    pub async fn load(&self) -> anyhow::Result<AppState> {
        if !self.path.exists() {
            return Ok(AppState { active_profile: None, pid: None });
        }
        let content = fs::read_to_string(&self.path).await?;
        let state = serde_json::from_str(&content)?;
        Ok(state)
    }

    pub async fn save(&self, state: &AppState) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(state)?;
        fs::write(&self.path, content).await?;
        Ok(())
    }
}
