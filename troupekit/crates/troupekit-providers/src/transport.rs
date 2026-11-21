use crate::status::decode_error;
use async_trait::async_trait;
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Mutex, MutexGuard};
use troupekit_core::error::{ProviderError, ProviderErrorKind};

/// An HTTP POST to a provider: URL, headers and the JSON body bytes.
/// Headers may carry an API key, so `Debug` prints the URL, the header
/// names and the body length, never a header value.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// Looks up header `name`, ignoring ASCII case.
    /// Returns its value, or `None` when the request has no such header.
    /// When a header repeats, the first value wins.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("HttpRequest")
            .field("url", &self.url)
            .field("headers", &names)
            .field("body_len", &self.body.len())
            .finish()
    }
}

/// The body of an HTTP reply, read chunk by chunk so streams stay streams.
/// `chunk` yields the next bytes, then `None` once the body has ended.
/// Transports implement it over their own body type.
#[async_trait]
pub trait Body: Send {
    async fn chunk(&mut self) -> Result<Option<Vec<u8>>, ProviderError>;
}

/// An HTTP reply: status code, headers and a body still to be read.
/// Header names keep the case the transport saw.
/// Read the body with `read_body` or chunk by chunk.
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Box<dyn Body>,
}

/// Sends provider requests. HTTP clients, proxies, in-process runtimes
/// and test fakes implement it, so models never own an HTTP stack.
/// Failures are `Network`, or `Stalled` for timeouts.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError>;
}

/// Reads all of `body` into memory.
/// Returns the bytes, or `Decode` as soon as more than `limit` bytes
/// have arrived, so a hostile server cannot exhaust memory.
pub async fn read_body(body: &mut dyn Body, limit: usize) -> Result<Vec<u8>, ProviderError> {
    let mut out = Vec::new();
    while let Some(chunk) = body.chunk().await? {
        if out.len() + chunk.len() > limit {
            return Err(decode_error(format!("reply body is over {limit} bytes")));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// One reply a `ScriptedTransport` gives back: a status with headers and
/// body chunks, or a transport failure.
/// Build replies with `body`, `chunked` and `with_header`.
#[derive(Debug, Clone)]
pub enum Scripted {
    Reply {
        status: u16,
        headers: Vec<(String, String)>,
        chunks: Vec<Vec<u8>>,
    },
    Fail(ProviderError),
}

impl Scripted {
    /// A reply with `status` and the whole `body` in one chunk.
    /// Returns the scripted reply, without headers.
    /// Add headers with `with_header`.
    pub fn body(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Scripted::Reply {
            status,
            headers: Vec::new(),
            chunks: vec![body.into()],
        }
    }

    /// A 200 reply whose `body` arrives in chunks of `size` bytes, the way
    /// a slow stream would. Chunks may split UTF-8 characters.
    /// Panics when `size` is zero.
    pub fn chunked(body: &str, size: usize) -> Self {
        assert!(size > 0, "chunk size must be above zero");
        let chunks = body.as_bytes().chunks(size).map(<[u8]>::to_vec).collect();
        Scripted::Reply {
            status: 200,
            headers: Vec::new(),
            chunks,
        }
    }

    /// Adds header `name` with `value` to a reply; failures are unchanged.
    /// Returns the scripted reply.
    /// Use it for `retry-after` and similar headers in tests.
    pub fn with_header(self, name: &str, value: &str) -> Self {
        match self {
            Scripted::Reply {
                status,
                mut headers,
                chunks,
            } => {
                headers.push((name.to_owned(), value.to_owned()));
                Scripted::Reply {
                    status,
                    headers,
                    chunks,
                }
            }
            fail => fail,
        }
    }
}

struct ChunkBody(VecDeque<Vec<u8>>);

#[async_trait]
impl Body for ChunkBody {
    async fn chunk(&mut self) -> Result<Option<Vec<u8>>, ProviderError> {
        Ok(self.0.pop_front())
    }
}

/// A fake transport that replays scripted replies in order and records
/// every request sent to it, so tests never touch the network.
/// Once the script runs out, `send` fails with `Network`.
#[derive(Debug, Default)]
pub struct ScriptedTransport {
    replies: Mutex<VecDeque<Scripted>>,
    sent: Mutex<Vec<HttpRequest>>,
}

impl ScriptedTransport {
    /// Builds a transport that answers with `replies`, first to last.
    /// Returns the transport.
    /// Once every reply is used, sends fail with `Network`.
    pub fn new(replies: Vec<Scripted>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            sent: Mutex::new(Vec::new()),
        }
    }

    /// Copies every request sent so far, oldest first.
    /// Returns the copies.
    /// Tests read them to check URLs, headers and bodies.
    pub fn sent(&self) -> Vec<HttpRequest> {
        lock(&self.sent).clone()
    }
}

#[async_trait]
impl Transport for ScriptedTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError> {
        lock(&self.sent).push(request);
        let next = lock(&self.replies).pop_front();
        match next {
            Some(Scripted::Reply {
                status,
                headers,
                chunks,
            }) => Ok(HttpResponse {
                status,
                headers,
                body: Box::new(ChunkBody(chunks.into())),
            }),
            Some(Scripted::Fail(err)) => Err(err),
            None => Err(ProviderError::new(
                ProviderErrorKind::Network,
                "script has no more replies",
            )),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
