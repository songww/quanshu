use std::{net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};

use hyper::body::HttpBody;
use pyo3::{
    exceptions::{PyException, PyValueError},
    prelude::*,
    types::{IntoPyDict, PyDict, PyList, PyString},
};
use pyo3_asyncio::TaskLocals;
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    Mutex,
};

#[non_exhaustive]
#[derive(Clone)]
pub enum SpecVersion {
    V2_0,
    V2_1,
    V2_2,
    V2_3,
}

impl Default for SpecVersion {
    fn default() -> Self {
        SpecVersion::V2_3
    }
}

impl AsRef<str> for SpecVersion {
    fn as_ref(&self) -> &'static str {
        match self {
            SpecVersion::V2_0 => "2.0",
            SpecVersion::V2_1 => "2.1",
            SpecVersion::V2_2 => "2.2",
            SpecVersion::V2_3 => "2.3",
            // _ => unreachable!(),
        }
    }
}

impl ToPyObject for SpecVersion {
    fn to_object(&self, py: Python) -> PyObject {
        self.as_ref().to_object(py)
    }
}

#[non_exhaustive]
#[derive(Clone)]
pub enum AsgiVersion {
    V2,
    V3,
}

impl Default for AsgiVersion {
    fn default() -> Self {
        AsgiVersion::V3
    }
}

impl AsRef<str> for AsgiVersion {
    fn as_ref(&self) -> &str {
        match self {
            AsgiVersion::V2 => "2.0",
            AsgiVersion::V3 => "3.0",
            // _ => unreachable!(),
        }
    }
}

impl ToPyObject for AsgiVersion {
    fn to_object(&self, py: Python) -> PyObject {
        self.as_ref().to_object(py)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Type {
    Http,              // -> "http"
    HttpRequest,       // -> "http.request"
    HttpResponseStart, // -> "http.response.start"
    HttpResponseBody,  // -> "http.response.body"
    HttpDisconnect,    // -> "http.disconnect"

    Websocket,           // -> "websocket"
    WebsocketConnect,    // -> "websocket.connect"
    WebsocketAccept,     // -> "websocket.accept"
    WebsocketReceive,    // -> "websocket.receive"
    WebsocketSend,       // -> "websocket.send"
    WebsocketDisconnect, // -> "websocket.disconnect"
    WebsocketClose,      // -> "websocket.close"
}

impl Type {
    fn as_str(&self) -> &str {
        self.as_ref()
    }

    fn is_websocket(&self) -> bool {
        matches!(
            self,
            Type::Websocket
                | Type::WebsocketReceive
                | Type::WebsocketConnect
                | Type::WebsocketAccept
                | Type::WebsocketSend
                | Type::WebsocketDisconnect
                | Type::WebsocketClose
        )
    }
}

impl ToPyObject for Type {
    fn to_object(&self, py: Python) -> PyObject {
        self.as_str().to_object(py)
    }
}

impl AsRef<str> for Type {
    fn as_ref(&self) -> &'static str {
        match self {
            Type::Http => "http",
            Type::HttpRequest => "http.request",
            Type::HttpResponseStart => "http.response.start",
            Type::HttpResponseBody => "http.response.body",
            Type::HttpDisconnect => "http.disconnect",

            Type::Websocket => "websocket",
            Type::WebsocketConnect => "websocket.connect",
            Type::WebsocketAccept => "websocket.accept",
            Type::WebsocketReceive => "websocket.receive",
            Type::WebsocketSend => "websocket.send",
            Type::WebsocketDisconnect => "websocket.disconnect",
            Type::WebsocketClose => "websocket.close",
        }
    }
}

impl FromStr for Type {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = match s {
            "http" => Type::Http,
            "http.request" => Type::HttpRequest,
            "http.response.start" => Type::HttpResponseStart,
            "http.response.body" => Type::HttpResponseBody,
            "http.disconnect" => Type::HttpDisconnect,

            "websocket" => Type::Websocket,
            "websocket.connect" => Type::WebsocketConnect,
            "websocket.accept" => Type::WebsocketAccept,
            "websocket.receive" => Type::WebsocketReceive,
            "websocket.send" => Type::WebsocketSend,
            "websocket.disconnect" => Type::WebsocketDisconnect,
            "websocket.close" => Type::WebsocketClose,
            _ => return Err("Invalid Type"),
        };
        Ok(t)
    }
}

impl<'source> FromPyObject<'source> for Type {
    fn extract(obj: &'source PyAny) -> PyResult<Type> {
        //
        let s: &PyString = obj.downcast()?;
        Type::from_str(s.to_str()?).map_err(|err| PyErr::new::<PyValueError, _>(err))
    }
}

#[non_exhaustive]
#[derive(Clone)]
pub enum HttpVersion {
    V1_0,
    V1_1,
    V2,
}

impl AsRef<str> for HttpVersion {
    fn as_ref(&self) -> &'static str {
        match self {
            HttpVersion::V1_0 => "1.0",
            HttpVersion::V1_1 => "1.1",
            HttpVersion::V2 => "2",
            // _ => unreachable!(),
        }
    }
}

impl From<hyper::Version> for HttpVersion {
    fn from(v: hyper::Version) -> Self {
        match v {
            hyper::Version::HTTP_10 => HttpVersion::V1_0,
            hyper::Version::HTTP_11 => HttpVersion::V1_1,
            hyper::Version::HTTP_2 => HttpVersion::V2,
            _ => unimplemented!("{:?}", v),
        }
    }
}

impl ToPyObject for HttpVersion {
    fn to_object(&self, py: Python) -> PyObject {
        self.as_ref().to_object(py)
    }
}

#[derive(Clone, Default)]
pub struct Asgi {
    version: AsgiVersion,
    spec_version: SpecVersion,
}

impl IntoPyDict for Asgi {
    fn into_py_dict(self, py: Python) -> &PyDict {
        let dict = PyDict::new(py);
        dict.set_item("version", self.version).unwrap();
        dict.set_item("spec_version", self.spec_version).unwrap();
        dict
    }
}

#[derive(Clone)]
pub enum ServerAddr {
    SocketAddr(SocketAddr),
    UnixSocket(PathBuf),
}

impl From<SocketAddr> for ServerAddr {
    fn from(addr: SocketAddr) -> Self {
        ServerAddr::SocketAddr(addr)
    }
}

impl From<PathBuf> for ServerAddr {
    fn from(us: PathBuf) -> Self {
        ServerAddr::UnixSocket(us)
    }
}

#[derive(Clone)]
pub struct Scope<'a> {
    type_: Type,
    asgi: Asgi,
    http_version: HttpVersion,
    method: &'a str,
    scheme: &'a str,
    path: &'a str,
    raw_path: Option<&'a [u8]>,
    query_string: &'a [u8],
    root_path: &'a str,
    headers: &'a [(&'a [u8], &'a [u8])],
    client: Option<SocketAddr>,
    server: ServerAddr,
    // only for websocket
    subprotocols: Option<&'a [&'a str]>,
}

impl<'a> Scope<'a> {
    pub fn new<HV: Into<HttpVersion>>(
        type_: Type,
        asgi: Asgi,
        http_version: HV,
        method: &'a str,
        scheme: &'a str,
        path: &'a str,
        raw_path: Option<&'a [u8]>,
        query_string: &'a [u8],
        root_path: &'a str,
        headers: &'a [(&'a [u8], &'a [u8])],
        client: Option<SocketAddr>,
        server: ServerAddr,
    ) -> Self {
        Scope {
            type_,
            asgi,
            http_version: http_version.into(),
            method,
            scheme,
            path,
            raw_path,
            query_string,
            root_path,
            headers,
            client,
            server,
            subprotocols: None,
        }
    }

    pub fn set_subprotocols(&mut self, subprotocols: &'a [&'a str]) {
        self.subprotocols.replace(subprotocols);
    }
}

impl IntoPyDict for Scope<'_> {
    fn into_py_dict(self, py: Python<'_>) -> &'_ PyDict {
        let dict = PyDict::new(py);
        dict.set_item("type", self.type_).unwrap();
        dict.set_item("asgi", self.asgi.into_py_dict(py)).unwrap();
        dict.set_item("http_version", self.http_version).unwrap();
        dict.set_item("method", self.method).unwrap();
        dict.set_item("scheme", self.scheme).unwrap();
        dict.set_item("path", self.path).unwrap();
        dict.set_item("raw_path", self.raw_path).unwrap();
        dict.set_item("query_string", self.query_string).unwrap();
        dict.set_item("root_path", self.root_path).unwrap();
        dict.set_item("headers", self.headers).unwrap();

        dict.set_item("client", {
            if let Some(ref addr) = self.client {
                let list = PyList::new(py, vec![addr.ip().to_string()]);
                list.append(addr.port()).unwrap();
                list
            } else {
                PyList::new(py, vec![py.None(), py.None()])
            }
        })
        .unwrap();

        dict.set_item("server", {
            match self.server {
                ServerAddr::SocketAddr(addr) => {
                    let list = PyList::new(py, vec![addr.ip().to_string()]);
                    list.append(addr.port()).unwrap();
                    list
                }
                ServerAddr::UnixSocket(path) => {
                    let list = PyList::new(py, vec![path]);
                    list.append(py.None()).unwrap();
                    list
                }
            }
        })
        .unwrap();

        if self.type_.is_websocket() {
            if let Some(subprotocols) = self.subprotocols {
                dict.set_item("subprotocols", subprotocols).unwrap();
            }
        }

        dict
    }
}

#[derive(Debug)]
pub enum Receive {
    HttpRequest {
        // type_: Type,
        body: Vec<u8>,
        more_body: bool,
    },
    Disconnect, // type_: Type, // "http.disconnect"

    // websocket
    // websocket.connect
    WebsocketConnect,
    WebsocketReceive {
        bytes: Option<Vec<u8>>,
        text: Option<String>,
    },
    WebsocketDisconnect {
        // websocket.disconnect
        code: u32,
    },
}

impl IntoPy<Py<PyAny>> for Receive {
    fn into_py(self, py: Python) -> Py<PyAny> {
        let d = PyDict::new(py);
        match self {
            Receive::HttpRequest {
                // type_,
                body,
                more_body,
            } => {
                // d.set_item("type", type_.as_ref());
                d.set_item("type", Type::HttpRequest).unwrap();
                d.set_item("body", body).unwrap();
                d.set_item("more_body", more_body).unwrap();
            }
            Receive::Disconnect => {
                d.set_item("type", Type::HttpDisconnect).unwrap();
            }
            Receive::WebsocketConnect => {
                d.set_item("type", Type::WebsocketConnect).unwrap();
            }
            Receive::WebsocketReceive { bytes, text } => {
                d.set_item("type", Type::WebsocketReceive).unwrap();
                if let Some(bytes) = bytes {
                    d.set_item("bytes", bytes).unwrap();
                }
                if let Some(text) = text {
                    d.set_item("text", text).unwrap();
                }
            }
            Receive::WebsocketDisconnect { code } => {
                d.set_item("type", Type::WebsocketDisconnect).unwrap();
                d.set_item("code", code).unwrap();
            }
        };
        d.into()
    }
}

#[derive(FromPyObject, Clone, Debug)]
pub enum Send {
    ResponseStart {
        #[pyo3(item("type"))]
        type_: Type,
        #[pyo3(item)]
        status: u16,
        #[pyo3(item)]
        headers: Vec<(Vec<u8>, Vec<u8>)>,
    },
    ResponseBody {
        #[pyo3(item("type"))]
        type_: Type,
        #[pyo3(item)]
        body: Vec<u8>,
        #[pyo3(item)]
        more_body: bool,
    },

    // websocket
    WebsocketAccept {
        #[pyo3(item("type"))]
        type_: Type,
        #[pyo3(item)]
        subprotocols: Option<String>,
        #[pyo3(item)]
        headers: Vec<(Vec<u8>, Vec<u8>)>,
        // sec-websocket-protocol
        #[pyo3(item("sec-websocket-protocol"))]
        sec_websocket_protocol: Option<String>,
    },
    WebsocketSend {
        #[pyo3(item("type"))]
        type_: Type,
        #[pyo3(item)]
        bytes: Option<Vec<u8>>,
        #[pyo3(item)]
        text: Option<String>,
    },
    WebsocketClose {
        #[pyo3(item("type"))]
        type_: Type,
        // default 1000
        #[pyo3(item)]
        code: u32,
        #[pyo3(item)]
        reason: Option<String>,
    },
}

#[pyclass]
pub struct SenderAwaitable {
    sender: Sender,
}

#[pymethods]
impl SenderAwaitable {
    // fn __await__(
}

#[pyclass]
pub struct Sender {
    locals: TaskLocals,
    inner: UnboundedSender<Send>,
}

impl Sender {
    pub fn new(locals: TaskLocals, tx: UnboundedSender<Send>) -> Sender {
        Sender { locals, inner: tx }
    }
}

#[pymethods]
impl Sender {
    fn __call__<'a>(&'a self, py: Python<'a>, event: &'a PyDict) -> PyResult<&'a PyAny> {
        log::trace!("Sender: {:?}", event);
        let event = event.extract().expect("extract failed");
        log::trace!("event: {:?}", event);
        let sender = self.inner.clone();
        log::trace!("sender");
        pyo3_asyncio::tokio::future_into_py_with_locals(py, self.locals.clone(), async move {
            log::trace!("sender sending");
            sender
                .send(event)
                .map_err(|err| PyErr::new::<PyException, _>(err.to_string()))
        })
    }
}

#[pyclass]
pub struct Receiver {
    locals: TaskLocals,
    rx: Arc<Mutex<UnboundedReceiver<Receive>>>,
}

impl Receiver {
    pub fn new(locals: TaskLocals, rx: UnboundedReceiver<Receive>) -> Receiver {
        Receiver {
            locals,
            rx: Arc::new(Mutex::new(rx)),
        }
    }
}

#[pyclass]
pub struct ReceiverAwaitable {
    // receiver: Receiver,
}

#[pymethods]
impl ReceiverAwaitable {
    // fn __await__
}

#[pymethods]
impl Receiver {
    fn __call__<'a>(&'a self, py: Python<'a>) -> PyResult<&'a PyAny> {
        // let mut body = &mut self.body;
        let rx = self.rx.clone();
        pyo3_asyncio::tokio::future_into_py_with_locals(py, self.locals.clone(), async move {
            // receiver
            //     .recv()
            //     .map_err(|err| PyErr::new::<PyException, _>(err.to_string()))
            rx.lock()
                .await
                .recv()
                .await
                .ok_or_else(|| PyErr::new::<PyException, _>("Connection Closed"))
            // if let Some(data) = body.data().await {
            //     let data = data.map_err(|err| anyhow::anyhow!("ReadBuf failed {}", err))?;
            //     Ok(Receive::HttpRequest {
            //         body: &data,
            //         more_body: true,
            //     })
            // } else {
            //     Ok(Receive::HttpRequest {
            //         body: &[],
            //         more_body: false,
            //     })
            // }
        })
    }
}
