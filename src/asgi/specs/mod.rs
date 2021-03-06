use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use hyper::{
    body::{Buf, HttpBody},
    header::{HeaderName, HeaderValue},
    Body as HyperBody,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex as PMutex;
use pyo3::{
    exceptions::{PyException, PyKeyError, PyValueError},
    prelude::*,
    types::{IntoPyDict, PyBool, PyBytes, PyDict, PyList, PySequence, PyString},
};
use pyo3_futures::PyAsync;
use tokio::sync::{mpsc::UnboundedSender, Mutex};

#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum SpecVersion {
    V2_0,
    V2_1,
    V2_2,
    V2_3,
}

impl Default for SpecVersion {
    #[inline]
    fn default() -> Self {
        SpecVersion::V2_3
    }
}

impl AsRef<str> for SpecVersion {
    #[inline]
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
    #[inline]
    fn to_object(&self, py: Python) -> PyObject {
        self.as_ref().to_object(py)
    }
}

#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum AsgiVersion {
    V2,
    V3,
}

impl Default for AsgiVersion {
    #[inline]
    fn default() -> Self {
        AsgiVersion::V3
    }
}

impl AsRef<str> for AsgiVersion {
    #[inline]
    fn as_ref(&self) -> &str {
        match self {
            AsgiVersion::V2 => "2.0",
            AsgiVersion::V3 => "3.0",
            // _ => unreachable!(),
        }
    }
}

impl ToPyObject for AsgiVersion {
    #[inline]
    fn to_object(&self, py: Python) -> PyObject {
        self.as_ref().to_object(py)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    #[inline]
    fn as_str(&self) -> &str {
        self.as_ref()
    }

    #[inline]
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
    #[inline]
    fn to_object(&self, py: Python) -> PyObject {
        self.as_str().to_object(py)
    }
}

impl AsRef<str> for Type {
    #[inline]
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

    #[inline]
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
    #[inline]
    fn extract(obj: &'source PyAny) -> PyResult<Type> {
        let s: &PyString = obj.cast_as()?;
        Type::from_str(s.to_str()?).map_err(|err| PyErr::new::<PyValueError, _>(err))
    }
}

#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum HttpVersion {
    V1_0,
    V1_1,
    V2,
}

impl AsRef<str> for HttpVersion {
    #[inline]
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
    #[inline]
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
    #[inline]
    fn to_object(&self, py: Python) -> PyObject {
        self.as_ref().to_object(py)
    }
}

#[pyclass]
#[derive(Clone, Default, Debug)]
pub struct Asgi {
    version: AsgiVersion,
    spec_version: SpecVersion,
}

#[pymethods]
impl Asgi {
    fn as_dict<'py>(&self, py: Python<'py>) -> &'py PyDict {
        let dict = PyDict::new(py);
        dict.set_item("version", self.version.as_ref()).unwrap();
        dict.set_item("spec_version", self.spec_version.as_ref())
            .unwrap();
        dict
    }
}

impl IntoPyDict for Asgi {
    #[inline]
    fn into_py_dict(self, py: Python) -> &PyDict {
        let dict = PyDict::new(py);
        dict.set_item("version", self.version).unwrap();
        dict.set_item("spec_version", self.spec_version).unwrap();
        dict
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
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

#[pyclass]
#[derive(Clone, Debug)]
pub struct Scope {
    type_: Type,
    asgi: Asgi,
    http_version: HttpVersion,
    method: String,
    scheme: String,
    path: String,
    raw_path: Option<String>,
    query_string: String,
    root_path: String,
    headers: Vec<(HeaderName, HeaderValue)>,
    client: Option<SocketAddr>,
    server: ServerAddr,
    // only for websocket
    subprotocols: Option<Vec<String>>,
}

impl Scope {
    #[inline]
    pub fn new<HV: Into<HttpVersion>>(
        type_: Type,
        asgi: Asgi,
        http_version: HV,
        method: String,
        scheme: String,
        path: String,
        raw_path: Option<String>,
        query_string: String,
        root_path: String,
        headers: Vec<(HeaderName, HeaderValue)>,
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

    pub fn set_subprotocols(&mut self, subprotocols: Vec<String>) {
        self.subprotocols.replace(subprotocols);
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
// Type, HttpVersion, Method, Scheme, ServerAddr
struct CacheKey((Type, HttpVersion, String, String, ServerAddr));

#[pymethods]
impl Scope {
    pub(crate) fn as_dict<'py>(&'py self, py: Python<'py>) -> PyObject {
        static mut CACHED_SCOPES: Lazy<HashMap<CacheKey, Py<PyDict>>> =
            Lazy::new(|| HashMap::new());
        // TODO: use lru cache to prevent too many items.
        // SAFETY: With GilGuard.
        let caches = unsafe { &mut CACHED_SCOPES };
        let cache_key = CacheKey((
            self.type_,
            self.http_version.clone(),
            self.method.clone(),
            self.scheme.clone(),
            self.server.clone(),
        ));
        let entry = caches.entry(cache_key);
        let dict = entry
            .or_insert_with(|| {
                let dict = PyDict::new(py);
                dict.set_item("type", self.type_).unwrap();
                dict.set_item("asgi", self.asgi.as_dict(py)).unwrap();
                dict.set_item("http_version", self.http_version.as_ref())
                    .unwrap();
                dict.set_item("scheme", self.scheme.as_str()).unwrap();
                dict.set_item("method", self.method.as_str()).unwrap();
                dict.set_item("root_path", self.root_path.as_str()).unwrap();

                dict.set_item("server", {
                    match &self.server {
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
                dict.into_py(py)
            })
            .clone();
        let dict = dict.as_ref(py);
        static mut PATH_INTERNS: Lazy<HashMap<String, Py<PyString>>> = Lazy::new(|| HashMap::new());
        // TODO: use lru cache to prevent too many items.
        // SAFETY: With GilGuard.
        let interns = unsafe { &mut PATH_INTERNS };
        let path = interns
            .entry(self.path.clone())
            .or_insert_with(|| PyString::new(py, &self.path).into_py(py));
        dict.set_item("path", path.as_ref(py)).unwrap();
        if let Some(ref raw_path) = self.raw_path {
            dict.set_item("raw_path", PyBytes::new(py, raw_path.as_bytes()))
                .unwrap();
        } else {
            dict.set_item("raw_path", py.None()).unwrap();
        }
        // TODO: with lru cache to increase performance.
        dict.set_item("query_string", self.query_string.as_bytes())
            .unwrap();
        let headers = PyList::new(
            py,
            self.headers
                .iter()
                .map(|(k, v)| (PyBytes::new(py, k.as_ref()), PyBytes::new(py, v.as_bytes()))),
        );
        // TODO: How to optimize there?
        dict.set_item("headers", headers).unwrap();

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

        if self.type_.is_websocket() {
            if let Some(ref subprotocols) = self.subprotocols {
                dict.set_item("subprotocols", subprotocols).unwrap();
            }
        }

        dict.into()
    }
}

#[derive(Debug)]
pub enum Receive {
    /// "http.request"
    HttpRequest { body: Vec<u8>, more_body: bool },
    /// "http.disconnect"
    Disconnect,

    /// websocket
    /// websocket.connect
    WebsocketConnect,
    /// websocket.receive
    WebsocketReceive {
        bytes: Option<Vec<u8>>,
        text: Option<String>,
    },
    /// websocket.disconnect
    WebsocketDisconnect { code: u32 },
}

impl IntoPy<Py<PyAny>> for Receive {
    #[inline]
    fn into_py(self, py: Python) -> Py<PyAny> {
        let d = PyDict::new(py);
        match self {
            Receive::HttpRequest { body, more_body } => {
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
                    d.set_item("bytes", PyBytes::new(py, &bytes)).unwrap();
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
#[derive(Clone, Debug)]
pub struct Sender {
    inner: UnboundedSender<Send>,
}

impl Sender {
    #[inline]
    pub fn new(tx: UnboundedSender<Send>) -> Sender {
        Sender { inner: tx }
    }
}

#[inline]
fn parse_headers(headers: &PyAny) -> PyResult<Vec<(Vec<u8>, Vec<u8>)>> {
    headers
        .iter()?
        .map(|header| -> PyResult<(Vec<u8>, Vec<u8>)> {
            let header = header?.cast_as::<PySequence>()?;
            if header.len()? != 2 {
                Err(PyErr::new::<PyValueError, _>(format!(
                    "header required two elements for `name` and `value`, but not `{}`",
                    header
                )))
            } else {
                Ok((
                    header
                        .get_item(0)?
                        .cast_as::<PyBytes>()?
                        .as_bytes()
                        .to_vec(),
                    header
                        .get_item(1)?
                        .cast_as::<PyBytes>()?
                        .as_bytes()
                        .to_vec(),
                ))
            }
        })
        .collect()
}

#[pymethods]
impl Sender {
    fn __call__<'a>(
        &'a self,
        py: Python<'a>,
        event: &'a PyDict,
    ) -> PyResult<PyAsync<PyResult<()>>> {
        log::trace!("Sender: {:?}", event);
        // println!("Sender: {:?}", event);
        let stype: &str = event
            .get_item("type")
            .ok_or_else(|| PyErr::new::<PyKeyError, _>("type"))?
            .extract()?;
        let type_: Type = stype.parse().map_err(|err| {
            eprintln!("type `{:?}` is invalid, {:?}", stype, err);
            PyErr::new::<PyValueError, _>(format!("type `{:?}` is invalid, {:?}", stype, err))
        })?;
        log::trace!("type: {:?}", type_);
        // println!("type: {:?}", type_);
        let message = match type_ {
            Type::HttpResponseStart => {
                let status: u16 = event
                    .get_item("status")
                    .ok_or_else(|| PyErr::new::<PyKeyError, _>("status"))?
                    .extract()?;
                log::trace!("status: {:?}", status);
                let headers = if let Some(headers) = event.get_item("headers") {
                    parse_headers(headers)?
                } else {
                    vec![]
                };
                log::trace!("headers: {:?}", headers);
                Send::ResponseStart {
                    type_,
                    status,
                    headers,
                }
            }
            Type::HttpResponseBody => {
                let body: Vec<u8> = if let Some(body) = event.get_item("body") {
                    body.cast_as::<PyBytes>()?.as_bytes().to_vec()
                } else {
                    vec![]
                };
                let more_body: bool = if let Some(more) = event.get_item("more_body") {
                    more.cast_as::<PyBool>()?.is_true()
                } else {
                    false
                };
                Send::ResponseBody {
                    type_,
                    body,
                    more_body,
                }
            }
            _ => unreachable!(),
        };
        log::trace!("message: {:?}", message);
        let sender = self.inner.clone();
        log::trace!("sender");
        // println!("sender");
        Ok(PyAsync::from(async move {
            log::trace!("sender sending");
            // println!("sender sending");
            sender.send(message).map_err(|err| {
                log::error!("{:?}", err);
                PyErr::new::<PyException, _>(err.to_string())
            })
        }))
    }
}

#[pyclass]
#[derive(Debug)]
pub struct RequestReceiver {
    body: Arc<Mutex<HyperBody>>,
}

impl Clone for RequestReceiver {
    fn clone(&self) -> Self {
        RequestReceiver {
            body: self.body.clone(),
        }
    }
}

impl RequestReceiver {
    #[inline]
    pub fn new(body: HyperBody) -> RequestReceiver {
        RequestReceiver {
            body: Arc::new(Mutex::new(body)),
        }
    }
}

#[pymethods]
impl RequestReceiver {
    fn __call__<'py>(&'py self, py: Python<'py>) -> PyResult<PyAsync<PyResult<PyObject>>> {
        let body = self.body.clone();
        let future = async move {
            // let mut recv = this.borrow_mut();
            let mut body = body.lock().await;
            if let Some(buf) = body.data().await {
                let mut buf = buf.map_err(|err| anyhow::anyhow!("{}", err))?;
                unsafe {
                    Python::with_gil_unchecked(|py| {
                        let pybytes = PyBytes::new_with(py, buf.remaining(), |pybytes| {
                            buf.copy_to_slice(pybytes);
                            Ok(())
                        })?;
                        let req = PyDict::new(py);
                        req.set_item("type", Type::HttpRequest)?;
                        req.set_item("more_body", true)?;
                        req.set_item("body", pybytes)?;
                        Ok(req.to_object(py))
                    })
                }
            } else {
                unsafe {
                    Python::with_gil_unchecked(|py| {
                        let req = PyDict::new(py);
                        req.set_item("type", Type::HttpRequest)?;
                        req.set_item("more_body", false)?;
                        req.set_item("body", PyBytes::new(py, &[]))?;
                        Ok(req.to_object(py))
                    })
                }
            }
        };
        // unsafe {
        //     Python::with_gil_unchecked(|py| {
        // pyo3_asyncio::tokio::local_future_into_py_with_locals(py, self.locals.clone(), future)
        // })
        // }
        Ok(future.into())
    }
}
