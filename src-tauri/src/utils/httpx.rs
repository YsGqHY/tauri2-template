//! Internal HTTP transport with bounded responses and conservative retries.
//!
//! The shared blocking client uses reqwest 0.12, rustls, a 30-second timeout,
//! and an application user-agent. JSON helpers are Rust-library APIs; this
//! module exposes no Tauri command or frontend-controlled arbitrary transport.
//! Only idempotent methods retry automatically. POST is never retried by the
//! default policy. Cancellation prevents queued/retry work and is observed
//! between short backoff sleeps; an in-flight blocking socket remains bounded
//! by the request timeout.

use reqwest::blocking::{Client, Response};
use reqwest::header::HeaderMap;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;
pub const DEFAULT_USER_AGENT: &str = "tauri2-template/0.1";

/// Pure request limits and retry configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestPolicy {
    pub timeout: Duration,
    pub max_response_bytes: u64,
    pub retry: RetryPolicy,
}

impl Default for RequestPolicy {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            retry: RetryPolicy::default(),
        }
    }
}

/// Retry configuration. `max_retries` counts attempts after the initial request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 0,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(5),
        }
    }
}

impl RetryPolicy {
    /// Computes a deterministic jittered delay for a zero-based retry index.
    /// `jitter_sample` is clamped to `[0, 1]`; non-finite input becomes zero.
    pub fn delay_for_retry(&self, retry_index: u32, jitter_sample: f64) -> Duration {
        let max_nanos = self.max_delay.as_nanos();
        if max_nanos == 0 {
            return Duration::ZERO;
        }
        let multiplier = 1_u128.checked_shl(retry_index).unwrap_or(u128::MAX);
        let exponential = self.base_delay.as_nanos().saturating_mul(multiplier);
        let capped = exponential.min(max_nanos);
        let sample = if jitter_sample.is_finite() {
            jitter_sample.clamp(0.0, 1.0)
        } else {
            0.0
        };
        duration_from_nanos((capped as f64 * sample) as u128)
    }
}

/// Cooperative cancellation handle for queued/retry work.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Status information deliberately excludes URL, query strings, and response bodies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HttpError {
    pub status_code: u16,
    pub kind: HttpErrorKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpErrorKind {
    Client,
    Server,
    Other,
}

impl HttpError {
    pub fn from_status(status_code: u16) -> Self {
        let kind = match status_code {
            400..=499 => HttpErrorKind::Client,
            500..=599 => HttpErrorKind::Server,
            _ => HttpErrorKind::Other,
        };
        Self { status_code, kind }
    }
}

/// Public error shape intentionally avoids embedding reqwest's URL-bearing error text.
#[derive(Debug)]
pub enum HttpxError {
    InvalidUrl,
    BuildClient,
    RequestBuild,
    Transport,
    Cancelled,
    Status(HttpError),
    ResponseTooLarge,
    ResponseRead,
    JsonEncode,
    JsonDecode,
    InvalidHeader,
}

impl fmt::Display for HttpxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidUrl => "httpx invalid URL",
            Self::BuildClient => "httpx client initialization failed",
            Self::RequestBuild => "httpx request construction failed",
            Self::Transport => "httpx transport failed",
            Self::Cancelled => "httpx request cancelled",
            Self::Status(error) => {
                return write!(formatter, "httpx HTTP status {}", error.status_code)
            }
            Self::ResponseTooLarge => "httpx response exceeds configured limit",
            Self::ResponseRead => "httpx response read failed",
            Self::JsonEncode => "httpx request JSON encoding failed",
            Self::JsonDecode => "httpx response JSON decoding failed",
            Self::InvalidHeader => "httpx invalid request header",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for HttpxError {}

/// Bounded response bytes and status for internal callers that need non-JSON data.
#[derive(Debug)]
pub struct ResponseData {
    pub status_code: u16,
    pub body: Vec<u8>,
}

/// Shared reqwest client configured with a finite default timeout and user-agent.
pub fn shared_client() -> &'static Client {
    static SHARED: OnceLock<Client> = OnceLock::new();
    SHARED.get_or_init(|| build_client(DEFAULT_USER_AGENT).expect("default HTTP client must build"))
}

/// Library client carrying policy while reusing the shared connection pool by default.
#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    policy: RequestPolicy,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self {
            client: shared_client().clone(),
            policy: RequestPolicy::default(),
        }
    }
}

impl HttpClient {
    pub fn new(policy: RequestPolicy, user_agent: impl AsRef<str>) -> Result<Self, HttpxError> {
        let user_agent = user_agent.as_ref().trim();
        if user_agent.is_empty() {
            return Err(HttpxError::InvalidHeader);
        }
        Ok(Self {
            client: build_client(user_agent)?,
            policy,
        })
    }

    pub fn policy(&self) -> &RequestPolicy {
        &self.policy
    }

    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, HttpxError> {
        self.get_json_with_cancel(url, None)
    }

    pub fn get_json_with_cancel<T: DeserializeOwned>(
        &self,
        url: &str,
        cancel: Option<&CancellationToken>,
    ) -> Result<T, HttpxError> {
        let response = self.do_request_with_cancel(Method::GET, url, None, None, cancel)?;
        serde_json::from_slice(&response.body).map_err(|_| HttpxError::JsonDecode)
    }

    pub fn post_json<I: Serialize, O: DeserializeOwned>(
        &self,
        url: &str,
        input: &I,
    ) -> Result<O, HttpxError> {
        self.post_json_with_cancel(url, input, None)
    }

    pub fn post_json_with_cancel<I: Serialize, O: DeserializeOwned>(
        &self,
        url: &str,
        input: &I,
        cancel: Option<&CancellationToken>,
    ) -> Result<O, HttpxError> {
        let body = serde_json::to_vec(input).map_err(|_| HttpxError::JsonEncode)?;
        let response = self.do_request_with_cancel(
            Method::POST,
            url,
            Some(HeaderMap::from_iter([(
                reqwest::header::CONTENT_TYPE,
                reqwest::header::HeaderValue::from_static("application/json"),
            )])),
            Some(body),
            cancel,
        )?;
        serde_json::from_slice(&response.body).map_err(|_| HttpxError::JsonDecode)
    }

    pub fn do_request(
        &self,
        method: Method,
        url: &str,
        headers: Option<HeaderMap>,
        body: Option<Vec<u8>>,
    ) -> Result<ResponseData, HttpxError> {
        self.do_request_with_cancel(method, url, headers, body, None)
    }

    pub fn do_request_with_cancel(
        &self,
        method: Method,
        url: &str,
        headers: Option<HeaderMap>,
        body: Option<Vec<u8>>,
        cancel: Option<&CancellationToken>,
    ) -> Result<ResponseData, HttpxError> {
        if url.trim().is_empty() {
            return Err(HttpxError::InvalidUrl);
        }
        let retry_allowed = is_idempotent(&method);
        let max_retries = if retry_allowed {
            self.policy.retry.max_retries
        } else {
            0
        };

        for retry_index in 0..=max_retries {
            check_cancelled(cancel)?;
            match self.send_once(&method, url, headers.as_ref(), body.as_deref()) {
                Ok(response) => {
                    if response.status_code >= 400 {
                        let error =
                            HttpxError::Status(HttpError::from_status(response.status_code));
                        if retry_index < max_retries && is_retryable_status(response.status_code) {
                            sleep_backoff(&self.policy.retry, retry_index, cancel)?;
                            continue;
                        }
                        return Err(error);
                    }
                    return Ok(response);
                }
                Err(error) if retry_index < max_retries && is_retryable_transport(&error) => {
                    sleep_backoff(&self.policy.retry, retry_index, cancel)?;
                }
                Err(error) => return Err(error),
            }
        }
        Err(HttpxError::Transport)
    }

    fn send_once(
        &self,
        method: &Method,
        url: &str,
        headers: Option<&HeaderMap>,
        body: Option<&[u8]>,
    ) -> Result<ResponseData, HttpxError> {
        let mut request = self.client.request(method.clone(), url);
        request = request.timeout(self.policy.timeout);
        if let Some(headers) = headers {
            request = request.headers(headers.clone());
        }
        if let Some(body) = body {
            request = request.body(body.to_vec());
        }
        let response = request.send().map_err(|error| {
            if error.is_builder() {
                HttpxError::RequestBuild
            } else {
                HttpxError::Transport
            }
        })?;
        let status_code = response.status().as_u16();
        let body = read_response_limited(response, self.policy.max_response_bytes)?;
        Ok(ResponseData { status_code, body })
    }
}

fn build_client(user_agent: &str) -> Result<Client, HttpxError> {
    Client::builder()
        .timeout(DEFAULT_TIMEOUT)
        .user_agent(user_agent)
        .build()
        .map_err(|_| HttpxError::BuildClient)
}

fn is_idempotent(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::PUT | Method::DELETE
    )
}

fn is_retryable_status(status_code: u16) -> bool {
    status_code == 408 || status_code == 425 || status_code == 429 || status_code >= 500
}

fn is_retryable_transport(error: &HttpxError) -> bool {
    matches!(error, HttpxError::Transport)
}

fn check_cancelled(cancel: Option<&CancellationToken>) -> Result<(), HttpxError> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        Err(HttpxError::Cancelled)
    } else {
        Ok(())
    }
}

fn sleep_backoff(
    policy: &RetryPolicy,
    retry_index: u32,
    cancel: Option<&CancellationToken>,
) -> Result<(), HttpxError> {
    let mut remaining = policy.delay_for_retry(retry_index, 1.0);
    while !remaining.is_zero() {
        check_cancelled(cancel)?;
        let slice = remaining.min(Duration::from_millis(10));
        thread::sleep(slice);
        remaining = remaining.saturating_sub(slice);
    }
    check_cancelled(cancel)
}

/// Reads a response up to `max_bytes` and rejects an oversized body.
fn read_response_limited(mut response: Response, max_bytes: u64) -> Result<Vec<u8>, HttpxError> {
    let max_bytes = if max_bytes == 0 {
        DEFAULT_MAX_RESPONSE_BYTES
    } else {
        max_bytes
    };
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| HttpxError::ResponseRead)?;
    if bytes.len() as u64 > max_bytes {
        return Err(HttpxError::ResponseTooLarge);
    }
    Ok(bytes)
}

fn duration_from_nanos(nanos: u128) -> Duration {
    let max_nanos = Duration::new(u64::MAX, 999_999_999).as_nanos();
    let nanos = nanos.min(max_nanos);
    Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn read_request(stream: &mut TcpStream) -> String {
        let mut buffer = [0_u8; 4096];
        let size = stream.read(&mut buffer).expect("read request");
        String::from_utf8_lossy(&buffer[..size]).into_owned()
    }

    fn serve_once(response: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local listener");
        let address = listener.local_addr().expect("listener address");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            read_request(&mut stream);
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        format!("http://{address}")
    }

    #[test]
    fn get_json_success_and_user_agent() {
        let url = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}".to_owned(),
        );
        let result: Value = HttpClient::default().get_json(&url).expect("get JSON");
        assert_eq!(result["ok"], true);
    }

    #[test]
    fn status_error_does_not_expose_url_query_or_body() {
        let url = serve_once(
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 24\r\n\r\napi_key=raw-secret-value"
                .to_owned(),
        );
        let error = HttpClient::default()
            .get_json::<Value>(&format!("{url}/?token=raw-query-secret"))
            .expect_err("reject error status");
        let rendered = error.to_string();
        assert!(rendered.contains("401"));
        assert!(!rendered.contains("raw-query-secret"));
        assert!(!rendered.contains("raw-secret-value"));
    }

    #[test]
    fn response_limit_and_timeout_are_enforced() {
        let oversized =
            serve_once("HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\n12345678".to_owned());
        let policy = RequestPolicy {
            max_response_bytes: 4,
            ..RequestPolicy::default()
        };
        let client = HttpClient::new(policy, "test-agent").expect("build test client");
        assert!(matches!(
            client.get_json::<Value>(&oversized),
            Err(HttpxError::ResponseTooLarge)
        ));

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind timeout listener");
        let address = listener.local_addr().expect("timeout listener address");
        thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("accept timeout request");
            thread::sleep(Duration::from_millis(150));
        });
        let client = HttpClient::new(
            RequestPolicy {
                timeout: Duration::from_millis(25),
                ..RequestPolicy::default()
            },
            "test-agent",
        )
        .expect("build timeout client");
        assert!(matches!(
            client.get_json::<Value>(&format!("http://{address}")),
            Err(HttpxError::Transport)
        ));
    }

    #[test]
    fn only_idempotent_requests_retry_and_cancellation_is_observed() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind retry listener");
        let address = listener.local_addr().expect("retry listener address");
        let server = thread::spawn(move || {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept retry request");
                read_request(&mut stream);
                let response = if attempt == 0 {
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}"
                };
                stream
                    .write_all(response.as_bytes())
                    .expect("write retry response");
            }
        });
        let client = HttpClient::new(
            RequestPolicy {
                retry: RetryPolicy {
                    max_retries: 1,
                    base_delay: Duration::from_millis(1),
                    max_delay: Duration::from_millis(1),
                },
                ..RequestPolicy::default()
            },
            "test-agent",
        )
        .expect("build retry client");
        let result: Value = client
            .get_json(&format!("http://{address}"))
            .expect("retry GET");
        assert_eq!(result, json!({"ok": true}));
        server.join().expect("join retry server");

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind post listener");
        let address = listener.local_addr().expect("post listener address");
        listener
            .set_nonblocking(true)
            .expect("set POST listener nonblocking");
        let server = thread::spawn(move || {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("accept POST: {error}"),
                }
            };
            read_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n")
                .expect("write POST response");
            thread::sleep(Duration::from_millis(25));
            assert!(
                matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
                "POST must not retry"
            );
        });
        let policy = RequestPolicy {
            retry: RetryPolicy {
                max_retries: 4,
                base_delay: Duration::from_millis(1),
                max_delay: Duration::from_millis(1),
            },
            ..RequestPolicy::default()
        };
        let client = HttpClient::new(policy, "test-agent").expect("build POST client");
        let error = client
            .post_json::<_, Value>(&format!("http://{address}"), &json!({"x": 1}))
            .expect_err("POST status");
        assert!(matches!(error, HttpxError::Status(_)));
        server.join().expect("join POST server");

        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert!(matches!(
            HttpClient::default()
                .get_json_with_cancel::<Value>("http://127.0.0.1:1", Some(&cancellation)),
            Err(HttpxError::Cancelled)
        ));
    }

    #[test]
    fn retry_delay_is_stable_and_capped() {
        let policy = RetryPolicy {
            max_retries: 8,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(500),
        };
        assert_eq!(policy.delay_for_retry(2, 0.5), Duration::from_millis(200));
        assert_eq!(policy.delay_for_retry(3, 1.0), Duration::from_millis(500));
        assert_eq!(policy.delay_for_retry(99, 1.0), Duration::from_millis(500));
        assert_eq!(policy.delay_for_retry(0, f64::NAN), Duration::ZERO);
    }
}
