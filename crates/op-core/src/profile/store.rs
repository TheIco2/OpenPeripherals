use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::DeviceProfile;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Profile not found: {0}")]
    NotFound(String),
}

pub type ProfileResult<T> = Result<T, ProfileError>;

/// Stores and retrieves device profiles from disk.
pub struct ProfileStore {
    base_dir: PathBuf,
    /// Cached profiles keyed by profile ID.
    cache: HashMap<String, DeviceProfile>,
}

impl ProfileStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            cache: HashMap::new(),
        }
    }

    /// Load all profiles from the store directory.
    pub fn load_all(&mut self) -> ProfileResult<()> {
        self.cache.clear();

        if !self.base_dir.exists() {
            std::fs::create_dir_all(&self.base_dir)?;
            return Ok(());
        }

        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(profile) = self.load_file(&path)? {
                self.cache.insert(profile.id.clone(), profile);
            }
        }

        log::info!("Loaded {} device profiles", self.cache.len());
        Ok(())
    }

    fn load_file(&self, path: &Path) -> ProfileResult<Option<DeviceProfile>> {
        let ext = path.extension().and_then(|e| e.to_str());
        match ext {
            Some("json") => {
                let data = std::fs::read_to_string(path)?;
                let profile: DeviceProfile = serde_json::from_str(&data)?;
                Ok(Some(profile))
            }
            Some("yaml" | "yml") => {
                let data = std::fs::read_to_string(path)?;
                let profile: DeviceProfile = serde_yaml::from_str(&data)?;
                Ok(Some(profile))
            }
            _ => Ok(None),
        }
    }

    /// Save a profile to disk as JSON.
    pub fn save_json(&self, profile: &DeviceProfile) -> ProfileResult<()> {
        std::fs::create_dir_all(&self.base_dir)?;
        let path = self.base_dir.join(format!("{}.json", profile.id));
        let data = serde_json::to_string_pretty(profile)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Save a profile to disk as YAML.
    pub fn save_yaml(&self, profile: &DeviceProfile) -> ProfileResult<()> {
        std::fs::create_dir_all(&self.base_dir)?;
        let path = self.base_dir.join(format!("{}.yaml", profile.id));
        let data = serde_yaml::to_string(profile)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Find a profile matching the given VID:PID.
    pub fn find_by_vid_pid(&self, vid: u16, pid: u16) -> Option<&DeviceProfile> {
        self.cache.values().find(|p| p.matches(vid, pid))
    }

    /// Get a profile by its ID.
    pub fn get(&self, id: &str) -> Option<&DeviceProfile> {
        self.cache.get(id)
    }

    /// List all loaded profiles.
    pub fn list(&self) -> Vec<&DeviceProfile> {
        self.cache.values().collect()
    }
}
