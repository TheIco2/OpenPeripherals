use anyhow::{bail, Context, Result};
use reqwest::multipart;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use op_core::profile::DeviceProfile;

// ──── Manifest (matches op-addon's AddonManifest) ────

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct AddonManifest {
    id: String,
    name: String,
    version: String,
    author: String,
    description: String,
    #[serde(default)]
    brands: Vec<String>,
    #[serde(default)]
    device_types: Vec<String>,
    supported_devices: Vec<SupportedDevice>,
    library: String,
    #[serde(default)]
    min_app_version: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct SupportedDevice {
    vendor_id: u16,
    product_id: u16,
    name: String,
}

// ──── Firmware metadata (for publish-firmware command) ────

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct FirmwareMeta {
    id: String,
    brand: String,
    device_name: String,
    version: String,
    vendor_id: u16,
    product_ids: Vec<u16>,
    protection: String,
    release_notes: Option<String>,
    updater_addon_id: String,
}

// ──── validate ────

pub fn validate(project_path: &str) -> Result<()> {
    let manifest_path = Path::new(project_path).join("addon.yaml");
    if !manifest_path.exists() {
        bail!("No addon.yaml found at {}", manifest_path.display());
    }

    let text = fs::read_to_string(&manifest_path)
        .context("Failed to read addon.yaml")?;
    let manifest: AddonManifest =
        serde_yaml::from_str(&text).context("Invalid addon.yaml")?;

    // Basic validation
    if manifest.id.is_empty() {
        bail!("addon.yaml: 'id' must not be empty");
    }
    if manifest.version.is_empty() {
        bail!("addon.yaml: 'version' must not be empty");
    }
    if manifest.supported_devices.is_empty() {
        log::warn!("addon.yaml: no supported_devices listed — addon won't match any device");
    }
    if manifest.library.is_empty() {
        bail!("addon.yaml: 'library' must specify the shared library filename");
    }

    // Validate any profile.yaml if present
    let profile_path = Path::new(project_path).join("profile.yaml");
    if profile_path.exists() {
        let profile_text = fs::read_to_string(&profile_path)?;
        let _profile: DeviceProfile =
            serde_yaml::from_str(&profile_text).context("Invalid profile.yaml")?;
        log::info!("profile.yaml: valid");
    }

    log::info!(
        "addon.yaml: valid — {} v{} by {} ({} devices)",
        manifest.name,
        manifest.version,
        manifest.author,
        manifest.supported_devices.len()
    );

    Ok(())
}

// ──── build ────

pub fn build(project_path: &str, output_dir: &str) -> Result<()> {
    // Validate first
    validate(project_path)?;

    let manifest_path = Path::new(project_path).join("addon.yaml");
    let text = fs::read_to_string(&manifest_path)?;
    let manifest: AddonManifest = serde_yaml::from_str(&text)?;

    fs::create_dir_all(output_dir)?;

    let out_name = format!("{}-{}.opx", manifest.id, manifest.version);
    let out_path = Path::new(output_dir).join(&out_name);

    let file = fs::File::create(&out_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Add addon.yaml
    zip.start_file("addon.yaml", options)?;
    zip.write_all(text.as_bytes())?;

    // Add profile.yaml if present
    let profile_path = Path::new(project_path).join("profile.yaml");
    if profile_path.exists() {
        zip.start_file("profile.yaml", options)?;
        zip.write_all(&fs::read(&profile_path)?)?;
    }

    // Add the shared library
    let lib_path = Path::new(project_path).join(&manifest.library);
    if lib_path.exists() {
        zip.start_file(&manifest.library, options)?;
        zip.write_all(&fs::read(&lib_path)?)?;
    } else {
        log::warn!(
            "Library '{}' not found — package will be incomplete. Build the library first.",
            manifest.library
        );
    }

    // Add any other .yaml or .json config files in the project root
    for entry in fs::read_dir(project_path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if (name.ends_with(".yaml") || name.ends_with(".json"))
            && name != "addon.yaml"
            && name != "profile.yaml"
        {
            zip.start_file(&name, options)?;
            zip.write_all(&fs::read(entry.path())?)?;
        }
    }

    zip.finish()?;

    let data = fs::read(&out_path)?;
    let sha = sha256_hex(&data);
    log::info!(
        "Built {} ({} bytes, sha256: {})",
        out_path.display(),
        data.len(),
        sha
    );

    Ok(())
}

// ──── publish-addon ────

pub async fn publish_addon(server: &str, package_path: &str, manifest_path: &str) -> Result<()> {
    let text = fs::read_to_string(manifest_path).context("Failed to read manifest")?;
    let manifest: AddonManifest = serde_yaml::from_str(&text)?;

    let package_data = fs::read(package_path).context("Failed to read package file")?;

    let vid_pid_pairs: Vec<String> = manifest
        .supported_devices
        .iter()
        .map(|d| format!("{:04X}:{:04X}", d.vendor_id, d.product_id))
        .collect();

    let metadata = serde_json::json!({
        "id": manifest.id,
        "name": manifest.name,
        "version": manifest.version,
        "author": manifest.author,
        "description": manifest.description,
        "brands": manifest.brands,
        "device_types": manifest.device_types,
        "supported_devices": vid_pid_pairs,
        "min_app_version": manifest.min_app_version,
    });

    let form = multipart::Form::new()
        .part(
            "metadata",
            multipart::Part::text(metadata.to_string())
                .mime_str("application/json")?,
        )
        .part(
            "package",
            multipart::Part::bytes(package_data)
                .file_name(Path::new(package_path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string())
                .mime_str("application/octet-stream")?,
        );

    let url = format!("{server}/api/v1/addons");
    let resp = reqwest::Client::new()
        .post(&url)
        .multipart(form)
        .send()
        .await?;

    if resp.status().is_success() {
        log::info!("Published {} v{} successfully!", manifest.name, manifest.version);
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Server returned {status}: {body}");
    }

    Ok(())
}

// ──── publish-firmware ────

pub async fn publish_firmware(
    server: &str,
    binary_path: &str,
    metadata_path: &str,
) -> Result<()> {
    let meta_text = fs::read_to_string(metadata_path)?;
    let meta: FirmwareMeta = serde_json::from_str(&meta_text)?;
    let binary_data = fs::read(binary_path)?;

    let form = multipart::Form::new()
        .part(
            "metadata",
            multipart::Part::text(meta_text)
                .mime_str("application/json")?,
        )
        .part(
            "binary",
            multipart::Part::bytes(binary_data)
                .file_name(
                    Path::new(binary_path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                )
                .mime_str("application/octet-stream")?,
        );

    let url = format!("{server}/api/v1/firmware");
    let resp = reqwest::Client::new()
        .post(&url)
        .multipart(form)
        .send()
        .await?;

    if resp.status().is_success() {
        log::info!(
            "Published firmware {} v{} for {}",
            meta.id,
            meta.version,
            meta.brand
        );
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Server returned {status}: {body}");
    }

    Ok(())
}

// ──── publish-update ────

pub async fn publish_update(
    server: &str,
    archive_path: &str,
    version: &str,
    notes: Option<&str>,
) -> Result<()> {
    let data = fs::read(archive_path)?;

    let metadata = serde_json::json!({
        "version": version,
        "release_notes": notes,
    });

    let form = multipart::Form::new()
        .part(
            "metadata",
            multipart::Part::text(metadata.to_string())
                .mime_str("application/json")?,
        )
        .part(
            "archive",
            multipart::Part::bytes(data)
                .file_name(
                    Path::new(archive_path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                )
                .mime_str("application/octet-stream")?,
        );

    let url = format!("{server}/api/v1/updates");
    let resp = reqwest::Client::new()
        .post(&url)
        .multipart(form)
        .send()
        .await?;

    if resp.status().is_success() {
        log::info!("Published app update v{version}");
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Server returned {status}: {body}");
    }

    Ok(())
}

// ──── init ────

pub fn init_addon(name: &str) -> Result<()> {
    let dir = Path::new(name);
    if dir.exists() {
        bail!("Directory '{}' already exists", name);
    }

    fs::create_dir_all(dir)?;

    let manifest = format!(
        r#"id: "{name}"
name: "{name}"
version: "0.1.0"
author: "Your Name"
description: "OpenPeripheral addon for ..."
brands: []
device_types: []
supported_devices:
  - vendor_id: 0x0000
    product_id: 0x0000
    name: "Example Device"
library: "lib{lib_name}.dll"
"#,
        name = name,
        lib_name = name.replace('-', "_"),
    );

    fs::write(dir.join("addon.yaml"), manifest)?;

    let profile = r#"name: "Example Device"
vendor_id: 0
product_id: 0
device_type: "Other"
capabilities: []
signals: []
"#;
    fs::write(dir.join("profile.yaml"), profile)?;

    log::info!("Scaffolded new addon project in '{name}/'");
    log::info!("  addon.yaml   — addon manifest (edit this)");
    log::info!("  profile.yaml — device profile template");

    Ok(())
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
