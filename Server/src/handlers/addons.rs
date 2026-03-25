use actix_multipart::Multipart;
use actix_web::{web, HttpResponse};
use chrono::Utc;
use futures_util::StreamExt;

use crate::models::{AddonEntry, ErrorResponse, ListResponse, PublishAddonRequest, SearchQuery};
use crate::AppState;

pub async fn list_addons(
    state: web::Data<AppState>,
    query: web::Query<SearchQuery>,
) -> HttpResponse {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20).min(100);

    let db = state.db.lock().unwrap();
    let result = if let Some(ref q) = query.q {
        db.search_addons(q, query.brand.as_deref(), query.device_type.as_deref(), page, per_page)
    } else {
        db.list_addons(page, per_page)
    };

    match result {
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

pub async fn get_addon(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = path.into_inner();
    let db = state.db.lock().unwrap();
    match db.get_addon(&id) {
        Ok(Some(entry)) => HttpResponse::Ok().json(entry),
        Ok(None) => HttpResponse::NotFound().json(ErrorResponse {
            error: "addon not found".into(),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("database error: {e}"),
        }),
    }
}

pub async fn download_addon(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = path.into_inner();

    let version = {
        let db = state.db.lock().unwrap();
        match db.get_addon(&id) {
            Ok(Some(entry)) => {
                let _ = db.increment_addon_downloads(&id);
                entry.version.clone()
            }
            Ok(None) => {
                return HttpResponse::NotFound().json(ErrorResponse {
                    error: "addon not found".into(),
                });
            }
            Err(e) => {
                return HttpResponse::InternalServerError().json(ErrorResponse {
                    error: format!("database error: {e}"),
                });
            }
        }
    };

    match state.storage.read_addon(&id, &version).await {
        Ok(data) => HttpResponse::Ok()
            .content_type("application/octet-stream")
            .append_header((
                "Content-Disposition",
                format!("attachment; filename=\"{id}-{version}.opx\""),
            ))
            .body(data),
        Err(_) => HttpResponse::NotFound().json(ErrorResponse {
            error: "addon package not found on disk".into(),
        }),
    }
}

pub async fn publish_addon(
    state: web::Data<AppState>,
    mut payload: Multipart,
) -> HttpResponse {
    let mut meta: Option<PublishAddonRequest> = None;
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
            "package" => {
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
            error: "multipart form must include 'metadata' (JSON) and 'package' (file)".into(),
        });
    };

    let (sha256, size) = match state.storage.store_addon(&meta.id, &meta.version, &data).await {
        Ok(v) => v,
        Err(e) => {
            return HttpResponse::InternalServerError().json(ErrorResponse {
                error: format!("storage error: {e}"),
            });
        }
    };

    let entry = AddonEntry {
        id: meta.id,
        name: meta.name,
        version: meta.version,
        author: meta.author,
        description: meta.description,
        brands: meta.brands,
        device_types: meta.device_types,
        supported_devices: meta.supported_devices,
        downloads: 0,
        sha256,
        size,
        published_at: Utc::now(),
        min_app_version: meta.min_app_version,
    };

    let db = state.db.lock().unwrap();
    match db.upsert_addon(&entry) {
        Ok(_) => HttpResponse::Created().json(entry),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("database error: {e}"),
        }),
    }
}
