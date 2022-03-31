use std::net::SocketAddr;

use hyper::{http::uri::PathAndQuery, Method, StatusCode, Version};
use tokio::time::Instant;

// 127.0.0.1:43532 - "GET / HTTP/1.1" 200 OK
pub(crate) fn log(
    remove_addr: SocketAddr,
    method: Method,
    uri: PathAndQuery,
    http_version: Version,
    status: StatusCode,
    begin: Instant,
) {
    let elapsed = begin.elapsed();
    log::info!(
        "{} - \"{} {} {:?}\" {} {:.3}ms",
        remove_addr,
        method.as_str(),
        uri.as_str(),
        http_version,
        status,
        elapsed.as_secs_f64() * 1000.,
    )
}
