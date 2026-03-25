mod db;
mod handlers;
mod models;
mod storage;

use actix_web::{web, App, HttpServer};
use std::sync::Mutex;

use db::Database;
use storage::FileStorage;

pub struct AppState {
    pub db: Mutex<Database>,
    pub storage: FileStorage,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    op_core::logging::init("Server", cfg!(debug_assertions));

    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("OpenPeripheral")
        .join("server");

    std::fs::create_dir_all(&data_dir)?;

    let db = Database::open(&data_dir.join("registry.db"))
        .expect("Failed to open database");
    db.migrate().expect("Failed to run migrations");

    let storage = FileStorage::new(data_dir.join("packages"));
    storage.init().await?;

    let state = web::Data::new(AppState { db: Mutex::new(db), storage });

    let bind = std::env::var("OP_BIND").unwrap_or_else(|_| "127.0.0.1:8088".to_string());
    log::info!("OpenPeripheral server starting on {bind}");

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            // -- Addon routes --
            .route("/api/v1/addons", web::get().to(handlers::addons::list_addons))
            .route("/api/v1/addons/{addon_id}", web::get().to(handlers::addons::get_addon))
            .route("/api/v1/addons/{addon_id}/download", web::get().to(handlers::addons::download_addon))
            .route("/api/v1/addons", web::post().to(handlers::addons::publish_addon))
            // -- Firmware routes --
            .route("/api/v1/firmware", web::get().to(handlers::firmware::list_firmware))
            .route("/api/v1/firmware/check", web::get().to(handlers::firmware::check_firmware))
            .route("/api/v1/firmware/{firmware_id}/download", web::get().to(handlers::firmware::download_firmware))
            .route("/api/v1/firmware", web::post().to(handlers::firmware::publish_firmware))
            // -- App update routes --
            .route("/api/v1/updates/latest", web::get().to(handlers::updates::latest_version))
            .route("/api/v1/updates/{version}/download", web::get().to(handlers::updates::download_update))
            .route("/api/v1/updates", web::post().to(handlers::updates::publish_update))
            // -- Health --
            .route("/api/v1/health", web::get().to(handlers::health))
    })
    .bind(&bind)?
    .run()
    .await
}
