//! Entangle Desktop Client - Tauri Application
//!
//! Main entry point for the Tauri v2 desktop application.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod session;
mod signaling;
mod state;

use std::sync::Arc;
use tauri::Manager;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

use state::AppState;

fn main() {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_target(true)
        .with_thread_ids(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    info!("Starting Entangle Desktop Client");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let state = AppState::new();
            app.manage(Arc::new(state));

            info!("Application setup complete");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_peer_id,
            commands::get_permissions,
            commands::request_permissions,
            commands::open_permission_settings,
            commands::start_session,
            commands::stop_session,
            commands::get_session_status,
            commands::send_input,
            commands::request_keyframe,
            commands::set_quality,
            commands::accept_connection,
            commands::reject_connection,
        ])
        .run(tauri::generate_context!())
        .expect("Error running Entangle");
}
