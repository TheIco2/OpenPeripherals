use sha2::{Digest, Sha256};
use std::io;
use std::path::PathBuf;
use tokio::fs;

/// Manages on-disk blob storage for addon packages, firmware binaries,
/// and application update archives.
pub struct FileStorage {
    root: PathBuf,
}

impl FileStorage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub async fn init(&self) -> io::Result<()> {
        for sub in &["addons", "firmware", "updates"] {
            fs::create_dir_all(self.root.join(sub)).await?;
        }
        Ok(())
    }

    // ──── Addons ────

    pub fn addon_path(&self, addon_id: &str, version: &str) -> PathBuf {
        self.root
            .join("addons")
            .join(format!("{addon_id}-{version}.opx"))
    }

    pub async fn store_addon(
        &self,
        addon_id: &str,
        version: &str,
        data: &[u8],
    ) -> io::Result<(String, u64)> {
        let path = self.addon_path(addon_id, version);
        fs::write(&path, data).await?;
        let sha = sha256_hex(data);
        Ok((sha, data.len() as u64))
    }

    pub async fn read_addon(&self, addon_id: &str, version: &str) -> io::Result<Vec<u8>> {
        fs::read(self.addon_path(addon_id, version)).await
    }

    pub async fn addon_exists(&self, addon_id: &str, version: &str) -> bool {
        self.addon_path(addon_id, version).exists()
    }

    // ──── Firmware ────

    pub fn firmware_path(&self, firmware_id: &str, version: &str) -> PathBuf {
        self.root
            .join("firmware")
            .join(format!("{firmware_id}-{version}.bin"))
    }

    pub async fn store_firmware(
        &self,
        firmware_id: &str,
        version: &str,
        data: &[u8],
    ) -> io::Result<(String, u64)> {
        let path = self.firmware_path(firmware_id, version);
        fs::write(&path, data).await?;
        let sha = sha256_hex(data);
        Ok((sha, data.len() as u64))
    }

    pub async fn read_firmware(&self, firmware_id: &str, version: &str) -> io::Result<Vec<u8>> {
        fs::read(self.firmware_path(firmware_id, version)).await
    }

    // ──── App updates ────

    pub fn update_path(&self, version: &str) -> PathBuf {
        self.root
            .join("updates")
            .join(format!("openperipheral-{version}.zip"))
    }

    pub async fn store_update(
        &self,
        version: &str,
        data: &[u8],
    ) -> io::Result<(String, u64)> {
        let path = self.update_path(version);
        fs::write(&path, data).await?;
        let sha = sha256_hex(data);
        Ok((sha, data.len() as u64))
    }

    pub async fn read_update(&self, version: &str) -> io::Result<Vec<u8>> {
        fs::read(self.update_path(version)).await
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
