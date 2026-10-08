use reqwest::Client;
use reqwest::redirect::Policy;
use tokio::runtime::{Builder, Runtime};

use super::{ErrorKind, PreparedRequest, RunOptions};

pub(super) struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}

pub(super) trait Transport {
    fn send(
        &self,
        request: &PreparedRequest,
        options: &RunOptions,
    ) -> Result<HttpResponse, ErrorKind>;
}

/// The public executor stays synchronous; this private runtime provides a
/// deadline that covers sending and reading the *entire* response, not a
/// separate timeout for each chunk received from a slow server.
pub(super) struct ReqwestTransport {
    runtime: Runtime,
    client: Client,
}

impl ReqwestTransport {
    pub fn new(_options: &RunOptions) -> Result<Self, ErrorKind> {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| ErrorKind::Transport(error.to_string()))?;
        let client = {
            let _entered = runtime.enter();
            Client::builder()
                .redirect(Policy::none())
                .build()
                .map_err(|error| ErrorKind::Transport(error.to_string()))?
        };
        Ok(Self { runtime, client })
    }
}

impl Transport for ReqwestTransport {
    fn send(
        &self,
        request: &PreparedRequest,
        options: &RunOptions,
    ) -> Result<HttpResponse, ErrorKind> {
        let method = match request.method {
            crate::parser::nova::Method::Get => reqwest::Method::GET,
            crate::parser::nova::Method::Post => reqwest::Method::POST,
            crate::parser::nova::Method::Put => reqwest::Method::PUT,
            crate::parser::nova::Method::Patch => reqwest::Method::PATCH,
            crate::parser::nova::Method::Delete => reqwest::Method::DELETE,
            crate::parser::nova::Method::Head => reqwest::Method::HEAD,
            crate::parser::nova::Method::Options => reqwest::Method::OPTIONS,
        };
        let mut outgoing = self
            .client
            .request(method, request.url.clone())
            .headers(request.headers.clone());
        if let Some(body) = &request.body {
            outgoing = outgoing.body(body.clone());
        }
        let result = self.runtime.block_on(async {
            tokio::time::timeout(options.timeout, async {
                let mut response = outgoing.send().await.map_err(request_error)?;
                let status = response.status().as_u16();
                let headers = response
                    .headers()
                    .iter()
                    .map(|(name, value)| (name.as_str().to_string(), value.as_bytes().to_vec()))
                    .collect();
                let mut body = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(request_error)? {
                    if chunk.len() > options.max_response_bytes.saturating_sub(body.len()) {
                        return Err(ErrorKind::ResponseTooLarge {
                            limit: options.max_response_bytes,
                        });
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(HttpResponse {
                    status,
                    headers,
                    body,
                })
            })
            .await
        });
        result.unwrap_or_else(|_| {
            Err(ErrorKind::Timeout(format!(
                "request exceeded {:?}",
                options.timeout
            )))
        })
    }
}

fn request_error(error: reqwest::Error) -> ErrorKind {
    if error.is_timeout() {
        ErrorKind::Timeout(error.to_string())
    } else {
        ErrorKind::Transport(error.to_string())
    }
}
