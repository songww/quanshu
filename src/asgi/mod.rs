mod specs;

use std::net::SocketAddr;

use futures::{sink::SinkExt, stream::StreamExt};
use hyper::header::{HeaderName, HeaderValue};
use hyper::http::response;
use hyper::StatusCode;
use hyper::{body::Buf, body::HttpBody, Body, Request, Response};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::HyperWebsocket;
use pyo3::prelude::*;
use pyo3::types::IntoPyDict;
use pyo3_asyncio::tokio::scope as scope_future;
use pyo3_asyncio::TaskLocals;
use tokio::select;
use tokio::sync::mpsc::unbounded_channel as unbounded;
use tokio::time::Instant;

use crate::Options;

#[derive(Debug, Clone)]
pub(crate) struct Context {
    locals: TaskLocals,
    // listening
    begin: Instant,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
}

impl Context {
    pub(crate) fn new(
        locals: TaskLocals,
        begin: Instant,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    ) -> Context {
        Context {
            begin,
            locals,
            local_addr,
            remote_addr,
        }
    }
}

pub(crate) struct Asgi {
    app: PyObject,
    ctx: Context,
    opts: Options,
}

impl Asgi {
    pub(crate) fn new(app: PyObject, ctx: Context, opts: Options) -> Asgi {
        Asgi { ctx, app, opts }
    }

    pub(crate) async fn serve(&self, request: Request<Body>) -> anyhow::Result<Response<Body>> {
        if hyper_tungstenite::is_upgrade_request(&request) {
            let (response, websocket) = hyper_tungstenite::upgrade(request, None)?;

            let ctx = self.ctx.clone();
            // Spawn a task to handle the websocket connection.
            tokio::spawn(scope_future(self.ctx.locals.clone(), async move {
                if let Err(e) = serve_websocket(ctx, websocket).await {
                    log::error!("Error in websocket connection: {}", e);
                }
            }));
            // Return the response so the spawned future can continue.
            Ok(response)
        } else {
            // Handle regular HTTP requests here.
            self.serve_httpx(request).await
        }
    }

    // Handle regular HTTP requests here.
    async fn serve_httpx(&self, mut request: Request<Body>) -> anyhow::Result<Response<Body>> {
        let uri = request.uri().clone();
        let request_headers = request.headers();
        let mut headers: Vec<_> = request_headers
            .iter()
            .filter(|(k, _)| !k.as_str().starts_with(':'))
            .map(|(k, v)| (k.as_ref(), v.as_bytes()))
            .collect();
        if !request_headers.contains_key("Host") && request_headers.contains_key(":Authority") {
            let host = request_headers.get(":Authority").unwrap();
            headers.push((b"Host".as_slice(), host.as_bytes()));
            let last = headers.len();
            headers.swap(0, last);
        };
        let connection_close = request_headers
            .get("Connection")
            .filter(|conn| conn.as_bytes() == b"close".as_slice())
            .is_some();

        let scope = specs::Scope::new(
            specs::Type::Http,
            specs::Asgi::default(),
            request.version(),
            request.method().as_str(),
            uri.scheme_str().unwrap_or("http"),
            uri.path(),
            None,
            uri.query().unwrap_or(""),
            &self.opts.root_path,
            &headers,
            Some(self.ctx.remote_addr.into()),
            self.ctx.local_addr.into(),
        );

        let locals = self.ctx.locals.clone();

        let (req_tx, rx) = unbounded();

        let (tx, mut resp_rx) = unbounded();

        let fut = pyo3::Python::with_gil(|py| -> PyResult<_> {
            let receive = Py::new(py, specs::Receiver::new(locals.clone(), rx))?;
            let send = Py::new(py, specs::Sender::new(locals.clone(), tx))?;
            let coro = self
                .app
                .call1(py, (scope.into_py_dict(py), receive, send))?;
            let el = self.ctx.locals.event_loop(py);
            let task = el.call_method1("create_task", (coro,))?;
            pyo3_asyncio::tokio::into_future(task)
        })?;
        let _app_process = tokio::spawn(scope_future(self.ctx.locals.clone(), fut));
        while let Some(buf) = request.body_mut().data().await {
            let buf = buf?;
            while buf.has_remaining() {
                let chunk = buf.chunk();
                req_tx.send(specs::Receive::HttpRequest {
                    body: chunk.to_vec(),
                    more_body: true,
                })?;
            }
        }
        req_tx.send(specs::Receive::HttpRequest {
            body: vec![],
            more_body: false,
        })?;

        log::trace!("waiting resp");
        let head = resp_rx.recv().await;
        log::trace!("waiting resp..");
        // let mut response_builder = Response::builder();
        let http_version = request.version();
        let (mut body_sender, body) = Body::channel();
        let mut resp = response::Response::new(body);
        if let Some(specs::Send::ResponseStart {
            type_: _,
            status,
            headers,
        }) = head
        {
            *resp.status_mut() = StatusCode::from_u16(status)?;
            let headers_map = resp.headers_mut();
            for (k, v) in headers.iter() {
                headers_map.append(HeaderName::from_bytes(&k)?, v.as_slice().try_into()?);
            }
            for (k, v) in self.opts.headers.iter() {
                headers_map.append(HeaderName::try_from(k)?, v.try_into()?);
            }
        } else {
            anyhow::bail!("ResponseStart required!");
        };
        if connection_close {
            resp.headers_mut().insert(
                HeaderName::from_static("connection"),
                HeaderValue::from_static("close"),
            );
        }

        let access_log = self.opts.access_log;
        let remote_addr = self.ctx.remote_addr;
        let method = request.method().clone();
        let uri = uri.path_and_query().unwrap().clone();
        let begin = self.ctx.begin;

        log::trace!("waiting resp body");
        if let Some(specs::Send::ResponseBody {
            type_: _,
            body: chunk,
            more_body,
        }) = resp_rx.recv().await
        {
            if !more_body {
                *resp.body_mut() = chunk.into();
                crate::access::log(remote_addr, method, uri, http_version, resp.status(), begin);
                return Ok(resp);
            }
            body_sender.send_data(chunk.into()).await?;
        } else {
            anyhow::bail!("ResponseBody required!")
        }

        let status = resp.status();
        let _further = tokio::spawn(async move {
            while let Some(specs::Send::ResponseBody {
                type_: _,
                body: chunk,
                more_body,
            }) = resp_rx.recv().await
            {
                log::trace!("waiting more resp body");
                body_sender
                    .send_data(chunk.into())
                    .await
                    .expect("Send response body failed");
                if !more_body {
                    break;
                }
            }
            if access_log {
                crate::access::log(remote_addr, method, uri, http_version, status, begin);
            }
        });
        Ok(resp)
    }
}

/// Handle a websocket connection.
async fn serve_websocket(_ctx: Context, websocket: HyperWebsocket) -> anyhow::Result<()> {
    let mut websocket = websocket.await?;
    loop {
        select! {
             message = websocket.next() => {
                if message.is_none() {
                    break;
                }
                let message = message.unwrap();
                match message? {
                    Message::Text(msg) => {
                        log::trace!("Received text message: {}", msg);
                        websocket
                            .send(Message::text("Thank you, come again."))
                            .await?;
                    }
                    Message::Binary(msg) => {
                        println!("Received binary message: {:02X?}", msg);
                        websocket
                            .send(Message::binary(b"Thank you, come again.".to_vec()))
                            .await?;
                    }
                    Message::Ping(msg) => {
                        // No need to send a reply: tungstenite takes care of this for you.
                        println!("Received ping message: {:02X?}", msg);
                    }
                    Message::Pong(msg) => {
                        println!("Received pong message: {:02X?}", msg);
                    }
                    Message::Close(msg) => {
                        // No need to send a reply: tungstenite takes care of this for you.
                        if let Some(msg) = &msg {
                            println!(
                                "Received close message with code {} and message: {}",
                                msg.code, msg.reason
                            );
                        } else {
                            println!("Received close message");
                        }
                    }
                    Message::Frame(_) => {
                        unreachable!();
                    }
                };
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                websocket.send(Message::Ping(Vec::new())).await?;
            }
        };
    }

    Ok(())
}
