mod specs;

use std::net::SocketAddr;

use futures::{sink::SinkExt, stream::StreamExt};
use hyper::{body::Buf, body::HttpBody, Body, Request, Response};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::HyperWebsocket;
use pyo3::prelude::*;
use pyo3::types::IntoPyDict;
use pyo3_asyncio::tokio::scope as scope_future;
use pyo3_asyncio::TaskLocals;
use tokio::select;
use tokio::sync::mpsc::channel as bounded;

use crate::Options;

#[derive(Debug, Clone)]
pub(crate) struct Context {
    locals: TaskLocals,
    // listening
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
}

impl Context {
    pub(crate) fn new(
        locals: TaskLocals,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    ) -> Context {
        Context {
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

    pub(crate) async fn serve(&self, mut request: Request<Body>) -> anyhow::Result<Response<Body>> {
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
            let uri = request.uri();
            println!("uri: {}", uri);
            let headers: Vec<_> = request
                .headers()
                .iter()
                .map(|(k, v)| (k.as_ref(), v.as_bytes()))
                .collect();
            let scope = specs::Scope::new(
                specs::Type::Http,
                specs::Asgi::default(),
                request.version(),
                request.method().as_str(),
                uri.scheme_str().unwrap_or("http"),
                uri.path(),
                None,
                uri.query().unwrap_or("").as_bytes(),
                &self.opts.root_path,
                &headers,
                Some(self.ctx.remote_addr.into()),
                self.ctx.local_addr.into(),
            );
            let (req_tx, rx) = bounded(4);

            let (tx, mut resp_rx) = bounded(4);

            let locals = self.ctx.locals.clone();
            println!("acquiring gil");
            let fut = pyo3::Python::with_gil(|py| -> PyResult<_> {
                println!("acquired gil");
                let receive = Py::new(py, specs::Receiver::new(locals.clone(), rx))?;
                let send = Py::new(py, specs::Sender::new(locals, tx))?;
                let coro = self
                    .app
                    .call1(py, (scope.into_py_dict(py), receive, send))
                    .unwrap();
                let el = self.ctx.locals.event_loop(py);
                let threading = py.import("threading").unwrap();
                println!(
                    "current thread: {:?}",
                    threading
                        .getattr("current_thread")
                        .unwrap()
                        .call0()
                        .unwrap()
                );
                let asyncio = py.import("asyncio").unwrap();
                // println!(
                //     "running loop: {:?} cache event loop: {:?}",
                //     asyncio
                //         .getattr("get_running_loop")
                //         .unwrap()
                //         .call0()
                //         .unwrap(),
                //     el
                // );
                let run_coroutine_threadsafe = asyncio.getattr("run_coroutine_threadsafe").unwrap();
                let fut = run_coroutine_threadsafe.call1((coro, el)).unwrap();
                // let inspect = PyModule::from_code(
                //     py,
                //     "def inspect(fut): print('fut done:', fut)",
                //     "inspect.py",
                //     "inspect",
                // )
                // .unwrap();
                // fut.call_method1("add_done_callback", (inspect.getattr("inspect").unwrap(),))
                //     .unwrap();

                Ok(fut.to_object(py))
                // let task = el.call_method1("create_task", (coro,)).unwrap();
                // pyo3_asyncio::tokio::into_future(task)
            })?;
            // let _app_process = tokio::spawn(scope_future(self.ctx.locals.clone(), fut));
            while let Some(buf) = request.body_mut().data().await {
                println!("reading more body");
                let buf = buf?;
                while buf.has_remaining() {
                    let chunk = buf.chunk();
                    req_tx
                        .send(specs::Receive::HttpRequest {
                            body: chunk.to_vec(),
                            more_body: true,
                        })
                        .await?;
                }
            }
            println!("request read done.");
            req_tx
                .send(specs::Receive::HttpRequest {
                    body: vec![],
                    more_body: false,
                })
                .await?;
            println!("waiting response");
            println!("waiting resp");
            let head = resp_rx.recv().await;
            println!("waiting resp..");
            let mut response_builder = Response::builder();
            if let Some(specs::Send::ResponseStart {
                type_: _,
                status,
                headers,
            }) = head
            {
                response_builder = response_builder.status(status);
                for (k, v) in headers.iter() {
                    response_builder =
                        response_builder.header(std::str::from_utf8(&k)?, std::str::from_utf8(&v)?);
                }
            } else {
                anyhow::bail!("ResponseStart required!");
            };
            println!("waiting resp body");
            let (mut body_sender, body) = Body::channel();
            if let Some(specs::Send::ResponseBody {
                type_: _,
                body: chunk,
                more_body,
            }) = resp_rx.recv().await
            {
                if !more_body {
                    return Ok(response_builder.body(chunk.into())?);
                }
                body_sender.send_data(chunk.into()).await?;
            } else {
                anyhow::bail!("ResponseBody required!")
            }
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
                println!("last");
                // Python::with_gil(|py| {
                //     let fut = fut.into_ref(py);
                //     println!("{:?}", fut.call_method0("result").unwrap());
                //     assert!(fut.call_method0("done").unwrap().is_true().unwrap());
                // })
            });
            Ok(response_builder.body(body)?)
        }
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
