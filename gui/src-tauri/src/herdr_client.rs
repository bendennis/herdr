use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

/// Env var override for the herdr JSON API socket path, matching the
/// resolution documented at docs/next/website/src/content/docs/socket-api.mdx.
const SOCKET_PATH_ENV_VAR: &str = "HERDR_SOCKET_PATH";

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct HerdrClientError(pub String);

impl std::fmt::Display for HerdrClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for HerdrClientError {}

impl From<std::io::Error> for HerdrClientError {
    fn from(err: std::io::Error) -> Self {
        Self(format!("herdr socket I/O error: {err}"))
    }
}

impl From<serde_json::Error> for HerdrClientError {
    fn from(err: serde_json::Error) -> Self {
        Self(format!("herdr response parse error: {err}"))
    }
}

/// Resolves the herdr JSON API socket path: `HERDR_SOCKET_PATH` override,
/// else the default session socket under the herdr config directory.
fn socket_path() -> PathBuf {
    if let Ok(path) = std::env::var(SOCKET_PATH_ENV_VAR) {
        return PathBuf::from(path);
    }
    config_dir().join("herdr.sock")
}

fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("herdr");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config").join("herdr");
    }
    std::env::temp_dir().join("herdr")
}

fn connect(path: &std::path::Path) -> std::io::Result<interprocess::local_socket::Stream> {
    #[cfg(unix)]
    {
        use interprocess::local_socket::{prelude::*, GenericFilePath};

        let name = path.to_fs_name::<GenericFilePath>()?;
        interprocess::local_socket::Stream::connect(name)
    }

    #[cfg(windows)]
    {
        use interprocess::local_socket::{prelude::*, GenericNamespaced};

        let name = path.to_string_lossy().to_string();
        let name = name.to_ns_name::<GenericNamespaced>()?;
        interprocess::local_socket::Stream::connect(name)
    }
}

/// Sends a single request to the herdr JSON API and returns the `result`
/// field of the response. Speaks the documented newline-delimited JSON wire
/// format directly (one request per connection) rather than depending on
/// herdr's internal Rust modules.
pub fn request(method: &str, params: Value) -> Result<Value, HerdrClientError> {
    let path = socket_path();
    let mut stream = connect(&path).map_err(|err| {
        HerdrClientError(format!(
            "failed to connect to herdr server at {}: {err}",
            path.display()
        ))
    })?;

    let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let request = json!({
        "id": format!("gui-{id}"),
        "method": method,
        "params": params,
    });

    let mut line = serde_json::to_string(&request)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    if response_line.trim().is_empty() {
        return Err(HerdrClientError(
            "herdr server closed the connection with no response".into(),
        ));
    }

    let response: Value = serde_json::from_str(response_line.trim())?;

    if let Some(error) = response.get("error") {
        return Err(HerdrClientError(format!("herdr returned an error: {error}")));
    }

    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;
    use interprocess::local_socket::traits::Listener as _;
    use interprocess::local_socket::{prelude::*, GenericFilePath, ListenerOptions};

    /// Verifies `request()` against a stub server that speaks the exact
    /// newline-delimited JSON wire format documented at
    /// docs/next/website/src/content/docs/socket-api.mdx, without depending
    /// on a real running herdr server.
    #[test]
    fn request_round_trips_against_documented_wire_format() {
        let socket_path =
            std::env::temp_dir().join(format!("herdr-gui-test-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket_path);

        let name = socket_path
            .as_path()
            .to_fs_name::<GenericFilePath>()
            .expect("fs name");
        let listener = ListenerOptions::new()
            .name(name)
            .reclaim_name(false)
            .create_sync()
            .expect("bind stub listener");

        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let mut conn = listener.accept().expect("accept");
                let mut reader = BufReader::new(&mut conn);
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                let request: Value = serde_json::from_str(line.trim()).expect("parse request");

                let response = match request.get("method").and_then(Value::as_str) {
                    Some("ping") => json!({"id": request["id"], "result": {"type": "pong"}}),
                    Some("session.snapshot") => json!({
                        "id": request["id"],
                        "result": {"type": "session_snapshot", "workspaces": []}
                    }),
                    _ => json!({"id": request["id"], "error": {"message": "unknown method"}}),
                };

                let mut out = serde_json::to_string(&response).expect("serialize response");
                out.push('\n');
                conn.write_all(out.as_bytes()).expect("write response");
            }
        });

        std::env::set_var(SOCKET_PATH_ENV_VAR, &socket_path);

        let pong = request("ping", json!({})).expect("ping should succeed");
        assert_eq!(pong, json!({"type": "pong"}));

        let snapshot = request("session.snapshot", json!({})).expect("snapshot should succeed");
        assert_eq!(snapshot["type"], json!("session_snapshot"));

        std::env::remove_var(SOCKET_PATH_ENV_VAR);
        server.join().expect("stub server thread panicked");
        let _ = std::fs::remove_file(&socket_path);
    }
}
