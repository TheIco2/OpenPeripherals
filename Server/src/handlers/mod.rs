pub mod addons;
pub mod firmware;
pub mod updates;

use actix_web::HttpResponse;

pub async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}
