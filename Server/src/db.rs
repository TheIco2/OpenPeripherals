use rusqlite::{params, Connection, Result as SqlResult};

use crate::models::{AddonEntry, AppVersion, FirmwareEntry};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &std::path::Path) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    pub fn migrate(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS addons (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                author TEXT NOT NULL,
                description TEXT NOT NULL,
                brands TEXT NOT NULL DEFAULT '[]',
                device_types TEXT NOT NULL DEFAULT '[]',
                supported_devices TEXT NOT NULL DEFAULT '[]',
                downloads INTEGER NOT NULL DEFAULT 0,
                sha256 TEXT NOT NULL,
                size INTEGER NOT NULL DEFAULT 0,
                published_at TEXT NOT NULL,
                min_app_version TEXT
            );

            CREATE TABLE IF NOT EXISTS firmware (
                id TEXT PRIMARY KEY,
                brand TEXT NOT NULL,
                device_name TEXT NOT NULL,
                version TEXT NOT NULL,
                vendor_id INTEGER NOT NULL,
                product_ids TEXT NOT NULL DEFAULT '[]',
                sha256 TEXT NOT NULL,
                size INTEGER NOT NULL DEFAULT 0,
                protection TEXT NOT NULL DEFAULT 'none',
                release_notes TEXT,
                updater_addon_id TEXT NOT NULL,
                downloads INTEGER NOT NULL DEFAULT 0,
                published_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS app_versions (
                version TEXT PRIMARY KEY,
                release_notes TEXT,
                sha256 TEXT NOT NULL,
                size INTEGER NOT NULL DEFAULT 0,
                published_at TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    // ──── Addons ────

    pub fn list_addons(&self, page: u32, per_page: u32) -> SqlResult<(Vec<AddonEntry>, u64)> {
        let total: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM addons", [], |row| row.get(0))?;

        let offset = (page.saturating_sub(1)) * per_page;
        let mut stmt = self.conn.prepare(
            "SELECT id, name, version, author, description, brands, device_types,
                    supported_devices, downloads, sha256, size, published_at, min_app_version
             FROM addons ORDER BY published_at DESC LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map(params![per_page, offset], |row| {
            Ok(AddonEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                version: row.get(2)?,
                author: row.get(3)?,
                description: row.get(4)?,
                brands: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                device_types: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                supported_devices: serde_json::from_str(&row.get::<_, String>(7)?)
                    .unwrap_or_default(),
                downloads: row.get(8)?,
                sha256: row.get(9)?,
                size: row.get(10)?,
                published_at: row
                    .get::<_, String>(11)?
                    .parse()
                    .unwrap_or_default(),
                min_app_version: row.get(12)?,
            })
        })?;

        let items: Vec<AddonEntry> = rows.filter_map(|r| r.ok()).collect();
        Ok((items, total))
    }

    pub fn get_addon(&self, id: &str) -> SqlResult<Option<AddonEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, version, author, description, brands, device_types,
                    supported_devices, downloads, sha256, size, published_at, min_app_version
             FROM addons WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            Ok(AddonEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                version: row.get(2)?,
                author: row.get(3)?,
                description: row.get(4)?,
                brands: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                device_types: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                supported_devices: serde_json::from_str(&row.get::<_, String>(7)?)
                    .unwrap_or_default(),
                downloads: row.get(8)?,
                sha256: row.get(9)?,
                size: row.get(10)?,
                published_at: row
                    .get::<_, String>(11)?
                    .parse()
                    .unwrap_or_default(),
                min_app_version: row.get(12)?,
            })
        })?;

        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn search_addons(
        &self,
        query: &str,
        brand: Option<&str>,
        device_type: Option<&str>,
        page: u32,
        per_page: u32,
    ) -> SqlResult<(Vec<AddonEntry>, u64)> {
        let like_query = format!("%{query}%");
        let offset = (page.saturating_sub(1)) * per_page;

        // Build dynamic WHERE clause
        let mut conditions = vec!["(name LIKE ?1 OR description LIKE ?1 OR id LIKE ?1)".to_string()];
        if brand.is_some() {
            conditions.push("brands LIKE ?3".to_string());
        }
        if device_type.is_some() {
            conditions.push("device_types LIKE ?4".to_string());
        }

        let where_clause = conditions.join(" AND ");
        let count_sql = format!("SELECT COUNT(*) FROM addons WHERE {where_clause}");
        let select_sql = format!(
            "SELECT id, name, version, author, description, brands, device_types,
                    supported_devices, downloads, sha256, size, published_at, min_app_version
             FROM addons WHERE {where_clause} ORDER BY downloads DESC LIMIT ?5 OFFSET ?6"
        );

        let brand_like = brand.map(|b| format!("%{b}%")).unwrap_or_default();
        let type_like = device_type.map(|t| format!("%{t}%")).unwrap_or_default();

        let total: u64 = self.conn.query_row(
            &count_sql,
            params![like_query, like_query, brand_like, type_like],
            |row| row.get(0),
        )?;

        let mut stmt = self.conn.prepare(&select_sql)?;
        let rows = stmt.query_map(
            params![like_query, like_query, brand_like, type_like, per_page, offset],
            |row| {
                Ok(AddonEntry {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    version: row.get(2)?,
                    author: row.get(3)?,
                    description: row.get(4)?,
                    brands: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                    device_types: serde_json::from_str(&row.get::<_, String>(6)?)
                        .unwrap_or_default(),
                    supported_devices: serde_json::from_str(&row.get::<_, String>(7)?)
                        .unwrap_or_default(),
                    downloads: row.get(8)?,
                    sha256: row.get(9)?,
                    size: row.get(10)?,
                    published_at: row
                        .get::<_, String>(11)?
                        .parse()
                        .unwrap_or_default(),
                    min_app_version: row.get(12)?,
                })
            },
        )?;

        let items: Vec<AddonEntry> = rows.filter_map(|r| r.ok()).collect();
        Ok((items, total))
    }

    pub fn upsert_addon(&self, entry: &AddonEntry) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO addons (id, name, version, author, description, brands, device_types,
                                  supported_devices, downloads, sha256, size, published_at, min_app_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                name=?2, version=?3, author=?4, description=?5, brands=?6, device_types=?7,
                supported_devices=?8, sha256=?10, size=?11, published_at=?12, min_app_version=?13",
            params![
                entry.id,
                entry.name,
                entry.version,
                entry.author,
                entry.description,
                serde_json::to_string(&entry.brands).unwrap_or_default(),
                serde_json::to_string(&entry.device_types).unwrap_or_default(),
                serde_json::to_string(&entry.supported_devices).unwrap_or_default(),
                entry.downloads,
                entry.sha256,
                entry.size,
                entry.published_at.to_rfc3339(),
                entry.min_app_version,
            ],
        )?;
        Ok(())
    }

    pub fn increment_addon_downloads(&self, id: &str) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE addons SET downloads = downloads + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // ──── Firmware ────

    pub fn list_firmware(&self, page: u32, per_page: u32) -> SqlResult<(Vec<FirmwareEntry>, u64)> {
        let total: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM firmware", [], |row| row.get(0))?;

        let offset = (page.saturating_sub(1)) * per_page;
        let mut stmt = self.conn.prepare(
            "SELECT id, brand, device_name, version, vendor_id, product_ids, sha256,
                    size, protection, release_notes, updater_addon_id, downloads, published_at
             FROM firmware ORDER BY published_at DESC LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map(params![per_page, offset], |row| {
            Ok(FirmwareEntry {
                id: row.get(0)?,
                brand: row.get(1)?,
                device_name: row.get(2)?,
                version: row.get(3)?,
                vendor_id: row.get(4)?,
                product_ids: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                sha256: row.get(6)?,
                size: row.get(7)?,
                protection: row.get(8)?,
                release_notes: row.get(9)?,
                updater_addon_id: row.get(10)?,
                downloads: row.get(11)?,
                published_at: row
                    .get::<_, String>(12)?
                    .parse()
                    .unwrap_or_default(),
            })
        })?;

        let items: Vec<FirmwareEntry> = rows.filter_map(|r| r.ok()).collect();
        Ok((items, total))
    }

    pub fn find_firmware_by_device(
        &self,
        vendor_id: u16,
        product_id: u16,
    ) -> SqlResult<Vec<FirmwareEntry>> {
        let pid_str = format!("{product_id}");
        let mut stmt = self.conn.prepare(
            "SELECT id, brand, device_name, version, vendor_id, product_ids, sha256,
                    size, protection, release_notes, updater_addon_id, downloads, published_at
             FROM firmware WHERE vendor_id = ?1 AND product_ids LIKE ?2
             ORDER BY published_at DESC",
        )?;

        let rows = stmt.query_map(params![vendor_id, format!("%{pid_str}%")], |row| {
            Ok(FirmwareEntry {
                id: row.get(0)?,
                brand: row.get(1)?,
                device_name: row.get(2)?,
                version: row.get(3)?,
                vendor_id: row.get(4)?,
                product_ids: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                sha256: row.get(6)?,
                size: row.get(7)?,
                protection: row.get(8)?,
                release_notes: row.get(9)?,
                updater_addon_id: row.get(10)?,
                downloads: row.get(11)?,
                published_at: row
                    .get::<_, String>(12)?
                    .parse()
                    .unwrap_or_default(),
            })
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn upsert_firmware(&self, entry: &FirmwareEntry) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO firmware (id, brand, device_name, version, vendor_id, product_ids, sha256,
                                    size, protection, release_notes, updater_addon_id, downloads, published_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                brand=?2, device_name=?3, version=?4, vendor_id=?5, product_ids=?6, sha256=?7,
                size=?8, protection=?9, release_notes=?10, updater_addon_id=?11, published_at=?13",
            params![
                entry.id,
                entry.brand,
                entry.device_name,
                entry.version,
                entry.vendor_id,
                serde_json::to_string(&entry.product_ids).unwrap_or_default(),
                entry.sha256,
                entry.size,
                entry.protection,
                entry.release_notes,
                entry.updater_addon_id,
                entry.downloads,
                entry.published_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn increment_firmware_downloads(&self, id: &str) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE firmware SET downloads = downloads + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // ──── App versions ────

    pub fn latest_app_version(&self) -> SqlResult<Option<AppVersion>> {
        let mut stmt = self.conn.prepare(
            "SELECT version, release_notes, sha256, size, published_at
             FROM app_versions ORDER BY published_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], |row| {
            Ok(AppVersion {
                version: row.get(0)?,
                release_notes: row.get(1)?,
                sha256: row.get(2)?,
                size: row.get(3)?,
                published_at: row
                    .get::<_, String>(4)?
                    .parse()
                    .unwrap_or_default(),
            })
        })?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn upsert_app_version(&self, entry: &AppVersion) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO app_versions (version, release_notes, sha256, size, published_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(version) DO UPDATE SET
                release_notes=?2, sha256=?3, size=?4, published_at=?5",
            params![
                entry.version,
                entry.release_notes,
                entry.sha256,
                entry.size,
                entry.published_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }
}
