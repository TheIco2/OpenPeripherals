use actix_multipart::Multipart;
use actix_web::{web, HttpResponse};
use chrono::Utc;
use futures_util::StreamExt;

use crate::models::{ErrorResponse, FirmwareCheckQuery, FirmwareEntry, ListResponse, PublishFirmwareRequest, SearchQuery};
use crate::AppState;

pub async fn list_firmware(
    state: web::Data<AppState>,
    query: web::Query<SearchQuery>,
) -> HttpResponse {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20).min(100);

    let db = state.db.lock().unwrap();
    match db.list_firmware(page, per_page) {
        Ok((items, total)) => {
            let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;
            HttpResponse::Ok().json(ListResponse {
                items,
                total,
                page,
                per_page,
                total_pages,
            })
        }
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("database error: {e}"),
        }),
    }
}

pub async fn check_firmware(
    state: web::Data<AppState>,
    query: web::Query<FirmwareCheckQuery>,
) -> HttpResponse {
    let db = state.db.lock().unwrap();
    match db.find_firmware_by_device(query.vendor_id, query.product_id) {
        Ok(entries) => HttpResponse::Ok().json(entries),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("database error: {e}"),
        }),
    }
}

pub async fn download_firmware(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = path.into_inner();

    let version = {
        let db = state.db.lock().unwrap();
        match db.list_firmware(1, 10000) {
            Ok((items, _)) => {
                if let Some(entry) = items.iter().find(|e| e.id == id) {
                    let _ = db.increment_firmware_downloads(&id);
                    entry.version.clone()
                } else {
                    return HttpResponse::NotFound().json(ErrorResponse {
                        error: "firmware not found".into(),
                    });
                }
            }
            Err(e) => {
                return HttpResponse::InternalServerError().json(ErrorResponse {
                    error: format!("database error: {e}"),
                });
            }
        }
    };

    match state.storage.read_firmware(&id, &version).await {
        Ok(data) => HttpResponse::Ok()
            .content_type("application/octet-stream")
            .append_header((
                "Content-Disposition",
                format!("attachment; filename=\"{id}-{version}.bin\""),
            ))
            .body(data),
        Err(_) => HttpResponse::NotFound().json(ErrorResponse {
            error: "firmware binary not found on disk".into(),
        }),
    }
}

pub async fn publish_firmware(
    state: web::Data<AppState>,
    mut payload: Multipart,
) -> HttpResponse {
    let mut meta: Option<PublishFirmwareRequest> = None;
    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(Ok(mut field)) = payload.next().await {
        let name = field.name().map(|s| s.to_string()).unwrap_or_default();
        match name.as_str() {
            "metadata" => {
                let mut buf = Vec::new();
                while let Some(Ok(chunk)) = field.next().await {
                    buf.extend_from_slice(&chunk);
                }
                meta = serde_json::from_slice(&buf).ok();
            }
            "binary" => {
                let mut buf = Vec::new();
                while let Some(Ok(chunk)) = field.next().await {
                    buf.extend_from_slice(&chunk);
                }
                file_bytes = Some(buf);
            }
            _ => {}
        }
    }

    let (Some(meta), Some(data)) = (meta, file_bytes) else {
        return HttpResponse::BadRequest().json(ErrorResponse {
            error: "multipart form must include 'metadata' (JSON) and 'binary' (file)".into(),
        });
    };

    let (sha256, size) = match state
        .storage
        .store_firmware(&meta.id, &meta.version, &data)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            return HttpResponse::InternalServerError().json(ErrorResponse {
                error: format!("storage error: {e}"),
            });
        }
    };

    let entry = FirmwareEntry {
        id: meta.id,
        brand: meta.brand,
        device_name: meta.device_name,
        version: meta.version,
        vendor_id: meta.vendor_id,
        product_ids: meta.product_ids,
        sha256,
        size,
        protection: meta.protection,
        release_notes: meta.release_notes,
        updater_addon_id: meta.updater_addon_id,
        downloads: 0,
        published_at: Utc::now(),
    };

    let db = state.db.lock().unwrap();
    match db.upsert_firmware(&entry) {
        Ok(_) => HttpResponse::Created().json(entry),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("database error: {e}"),
        }),
    }
}
