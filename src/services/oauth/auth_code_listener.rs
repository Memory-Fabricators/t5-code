//! Port of `src/services/oauth/auth-code-listener.ts`.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, oneshot};

use crate::PortResult;
use crate::constants::oauth::{
    CLAUDEAI_SUCCESS_URL, CONSOLE_SUCCESS_URL, should_use_claude_ai_auth,
};

struct Shared {
    expected_state: Mutex<String>,
    pending_response: Mutex<Option<tokio::net::TcpStream>>,
    code_tx: Mutex<Option<oneshot::Sender<Result<String, String>>>>,
}

/// Temporary localhost HTTP server that captures OAuth redirect codes.
pub struct AuthCodeListener {
    port: u16,
    callback_path: String,
    shared: Arc<Shared>,
    accept_task: Option<tokio::task::JoinHandle<()>>,
}

impl AuthCodeListener {
    pub fn new() -> Self {
        Self {
            port: 0,
            callback_path: "/callback".into(),
            shared: Arc::new(Shared {
                expected_state: Mutex::new(String::new()),
                pending_response: Mutex::new(None),
                code_tx: Mutex::new(None),
            }),
            accept_task: None,
        }
    }

    pub async fn start(&mut self, port: Option<u16>) -> PortResult<u16> {
        let addr = format!("127.0.0.1:{}", port.unwrap_or(0));
        let listener = TcpListener::bind(&addr).await.map_err(|e| {
            rootcause::report!(format!("Failed to start OAuth callback server: {e}"))
        })?;
        let local = listener
            .local_addr()
            .map_err(|e| rootcause::report!(e.to_string()))?;
        self.port = local.port();

        let shared = Arc::clone(&self.shared);
        let callback_path = self.callback_path.clone();
        self.accept_task = Some(tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = vec![0u8; 8192];
                let n = match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => continue,
                    Ok(n) => n,
                };
                let req = String::from_utf8_lossy(&buf[..n]);
                let Some(first_line) = req.lines().next() else {
                    let _ = write_http(&mut stream, 400, "Bad Request").await;
                    continue;
                };
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                if parts.len() < 2 || parts[0] != "GET" {
                    let _ = write_http(&mut stream, 404, "Not Found").await;
                    continue;
                }
                let path_q = parts[1];
                let (path, query) = match path_q.split_once('?') {
                    Some((p, q)) => (p, q),
                    None => (path_q, ""),
                };
                if path != callback_path {
                    let _ = write_http(&mut stream, 404, "Not Found").await;
                    continue;
                }

                let mut code: Option<String> = None;
                let mut state_param: Option<String> = None;
                for pair in query.split('&') {
                    if let Some((k, v)) = pair.split_once('=') {
                        let decoded = urlencoding_decode(v);
                        match k {
                            "code" => code = Some(decoded),
                            "state" => state_param = Some(decoded),
                            _ => {}
                        }
                    }
                }

                let expected = shared.expected_state.lock().await.clone();
                if code.is_none() {
                    if let Some(tx) = shared.code_tx.lock().await.take() {
                        let _ = tx.send(Err("No authorization code received".into()));
                    }
                    let _ = write_http(&mut stream, 400, "Authorization code not found").await;
                    break;
                }
                if state_param.as_deref() != Some(expected.as_str()) {
                    if let Some(tx) = shared.code_tx.lock().await.take() {
                        let _ = tx.send(Err("Invalid state parameter".into()));
                    }
                    let _ = write_http(&mut stream, 400, "Invalid state parameter").await;
                    break;
                }

                let auth_code = code.unwrap();
                *shared.pending_response.lock().await = Some(stream);
                if let Some(tx) = shared.code_tx.lock().await.take() {
                    let _ = tx.send(Ok(auth_code));
                }
                break;
            }
        }));

        Ok(self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn has_pending_response(&self) -> bool {
        self.shared.pending_response.lock().await.is_some()
    }

    /// Wait for redirect. `on_ready` runs once the accept loop is live.
    pub async fn wait_for_authorization<F, Fut>(
        &self,
        state: &str,
        on_ready: F,
    ) -> PortResult<String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let (tx, rx) = oneshot::channel();
        *self.shared.expected_state.lock().await = state.to_string();
        *self.shared.code_tx.lock().await = Some(tx);

        on_ready().await;

        match rx.await {
            Ok(Ok(code)) => Ok(code),
            Ok(Err(e)) => Err(rootcause::report!(e).into()),
            Err(_) => Err(rootcause::report!("OAuth callback cancelled").into()),
        }
    }

    pub async fn handle_success_redirect(&self, scopes: &[String]) {
        let Some(mut stream) = self.shared.pending_response.lock().await.take() else {
            return;
        };
        let success_url = if should_use_claude_ai_auth(Some(scopes)) {
            CLAUDEAI_SUCCESS_URL
        } else {
            CONSOLE_SUCCESS_URL
        };
        let _ = write_redirect(&mut stream, success_url).await;
    }

    pub async fn handle_error_redirect(&self) {
        let Some(mut stream) = self.shared.pending_response.lock().await.take() else {
            return;
        };
        let _ = write_redirect(&mut stream, CLAUDEAI_SUCCESS_URL).await;
    }

    pub async fn close(&self) {
        if let Some(mut stream) = self.shared.pending_response.lock().await.take() {
            let _ = write_redirect(&mut stream, CLAUDEAI_SUCCESS_URL).await;
        }
        *self.shared.code_tx.lock().await = None;
        if let Some(task) = &self.accept_task {
            task.abort();
        }
    }
}

async fn write_http(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.shutdown().await
}

async fn write_redirect(stream: &mut tokio::net::TcpStream, location: &str) -> std::io::Result<()> {
    let resp = format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.shutdown().await
}

fn urlencoding_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
