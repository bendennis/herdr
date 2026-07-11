mod herdr_client;

use serde_json::{json, Value};

#[tauri::command]
fn herdr_ping() -> Result<Value, String> {
    herdr_client::request("ping", json!({})).map_err(|err| err.to_string())
}

#[tauri::command]
fn herdr_snapshot() -> Result<Value, String> {
    herdr_client::request("session.snapshot", json!({})).map_err(|err| err.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![herdr_ping, herdr_snapshot])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
