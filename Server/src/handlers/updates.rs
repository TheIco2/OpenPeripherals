use actix_multipart::Multipart;
use actix_web::{web, HttpResponse};
use chrono::Utc;
use futures_util::StreamExt;

use crate::models::{AppVersion, ErrorResponse, PublishUpdateRequest};
use crate::AppState;

pub async fn latest_version(state: web::Data<AppState>) -> HttpResponse {
    let db = state.db.lock().unwrap();
    match db.latest_app_version() {
        Ok(Some(v)) => HttpResponse::Ok().json(v),
        Ok(None) => HttpResponse::NotFound().json(ErrorResponse {
            error: "no releases published yet".into(),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("database error: {e}"),
        }),
    }
}

pub async fn download_update(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let version = path.into_inner();

    match state.storage.read_update(&version).await {
        Ok(data) => HttpResponse::Ok()
            .content_type("application/octet-stream")
            .append_header((
                "Content-Disposition",
                format!("attachment; filename=\"openperipheral-{version}.zip\""),
            ))
            .body(data),
        Err(_) => HttpResponse::NotFound().json(ErrorResponse {
            error: "update archive not found".into(),
        }),
    }
}

pub async fn publish_update(
    state: web::Data<AppState>,
    mut payload: Multipart,
) -> HttpResponse {
    let mut meta: Option<PublishUpdateRequest> = None;
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
            "archive" => {
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
            error: "multipart form must include 'metadata' (JSON) and 'archive' (file)".into(),
        });
    };

    let (sha256, size) = match state.storage.store_update(&meta.version, &data).await {
        Ok(v) => v,
        Err(e) => {
            return HttpResponse::InternalServerError().json(ErrorResponse {
                error: format!("storage error: {e}"),
            });
        }
    };

    let entry = AppVersion {
        version: meta.version,
        release_notes: meta.release_notes,
        sha256,
        size,
        published_at: Utc::now(),
    };

    let db = state.db.lock().unwrap();
    match db.upsert_app_version(&entry) {
        Ok(_) => HttpResponse::Created().json(entry),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("database error: {e}"),
        }),
    }
}
