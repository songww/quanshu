mod access;
mod asgi;

use std::net::SocketAddr;
use std::net::ToSocketAddrs;
#[cfg(unix)]
use std::os::unix::prelude::FromRawFd;
#[cfg(windows)]
use std::os::windows::prelude::FromRawSocket;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;

use hyper::header::HeaderName;
use hyper::header::HeaderValue;
use hyper::{server::conn::Http, service::service_fn};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use stream::{Stream, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;
use tokio_graceful_shutdown::{SubsystemHandle, Toplevel};
use tokio_native_tls::native_tls::{Identity, TlsAcceptor};
use tokio_native_tls::TlsStream;
use tokio_stream as stream;

pub use asgi::RequestTask;

#[cfg(unix)]
type RawFd = std::os::unix::io::RawFd;
#[cfg(windows)]
type RawFd = std::os::windows::io::RawSocket;

static SHUTDOWN: tokio::sync::Notify = tokio::sync::Notify::const_new();

static PYAPPWRAPPING: &str = "
async def wraps(app, scope, receive, send):
    return await app(scope, receive, send)
";

fn to_socket_addrs(addr: &str) -> std::io::Result<Vec<SocketAddr>> {
    addr.to_socket_addrs().map(|iter| iter.collect())
}

fn try_parse_header(header: &str) -> anyhow::Result<(HeaderName, HeaderValue)> {
    if let Some((k, v)) = header.split_once(":") {
        Ok((HeaderName::from_str(k)?, HeaderValue::from_str(v)?))
    } else {
        anyhow::bail!("Invalid header ")
    }
}

fn try_import_app(s: &str) -> anyhow::Result<PyObject> {
    Python::with_gil(|py| -> anyhow::Result<PyObject> {
        let (module, attrs) = s.split_once(":").ok_or_else(|| {
            anyhow::anyhow!(
                "app string '{}' must be in format '<module>:<attribute>'.",
                s
            )
        })?;
        let pymodule = py.import(module)?;
        let mut pyapp = pymodule.as_ref();
        for attr in attrs.split(".") {
            pyapp = pyapp.getattr(attr)?
        }
        if !pyapp.is_callable() {
            anyhow::bail!("app '{}' is not callable!", s);
        }
        let wraps = PyModule::from_code(py, PYAPPWRAPPING, "py_app_wrapper.py", "py_app_wrapper")
            .unwrap()
            .getattr("wraps")
            .unwrap();

        let partial = py.import("functools").unwrap().getattr("partial").unwrap();
        let locals = pyo3::types::PyDict::new(py);
        locals.set_item("app", pyapp).unwrap();
        locals.set_item("wraps", wraps).unwrap();
        locals.set_item("partial", partial).unwrap();
        let pyapp = py.eval("partial(wraps, app)", None, Some(locals)).unwrap();
        println!("py app {}", pyapp);

        Ok(pyapp.to_object(py))
    })
}

type SocketAddrs = Vec<SocketAddr>;

#[pyclass]
#[derive(Clone, Debug, clap::Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Options {
    #[clap(parse(try_from_str = try_import_app))]
    pub app: PyObject,
    /// Bind to this
    #[clap(
        name = "bind",
        short = 'b',
        long = "bind",
        parse(try_from_str = to_socket_addrs),
        default_value = "127.0.0.1:8000",
        value_name = "ADDRESS"
    )]
    addrs: SocketAddrs,
    #[clap(long)]
    uds: Option<PathBuf>,
    #[clap(long)]
    fd: Option<RawFd>,
    #[clap(long)]
    // pkcs12: Option<Identity>,
    pkcs12: Option<PathBuf>,
    #[clap(short = 'H', long, parse(try_from_str = try_parse_header))]
    headers: Vec<(HeaderName, HeaderValue)>,
    #[clap(short, long)]
    workers: Option<usize>,
    #[clap(short, long, default_value = "")]
    root_path: String,
    // #[clap(short, long, default_value = "")]
    // date_header_enabled: bool,
    // server_header_enabled: bool,
    // proxy_headers_enabled: bool,
    #[clap(short, long, parse(from_occurrences))]
    verbose: usize,
    #[clap(skip)]
    access_log: bool,
}

#[pymethods]
impl Options {
    // #[new]
    // fn new(py: Python, app: &PyAny) -> PyResult<Options> {
    //     if app.is_none() {
    //         return Err(PyErr::new::<PyTypeError, _>(
    //             "Invalid app type, string or callable object expected.",
    //         ));
    //     }
    //     let app = {
    //         if app.is_instance_of::<PyString>()? {
    //             let s = app.cast_as::<PyString>()?.to_str()?;
    //             let (module, attrs) = s.split_once(":").ok_or_else(|| {
    //                 PyErr::new::<PyValueError, _>(format!(
    //                     "app string '{}' must be in format '<module>:<attribute>'.",
    //                     s
    //                 ))
    //             })?;
    //             let mut instance = py.import(&module)?.as_ref();
    //             for attr in attrs.split(".") {
    //                 instance = instance.getattr(attr)?
    //             }
    //             if !instance.is_callable() {
    //                 return Err(PyErr::new::<PyValueError, _>(format!(
    //                     "app '{}' is not callable!",
    //                     s
    //                 )));
    //             }
    //             instance
    //         } else if !app.is_callable() {
    //             return Err(PyErr::new::<PyValueError, _>(format!(
    //                 "app '{}' is not callable!",
    //                 app
    //             )));
    //         } else {
    //             app
    //         }
    //     };
    //     Ok(Options {
    //         app: app.to_object(py),
    //         addrs: "localhost:8000".to_socket_addrs()?.collect(),
    //         uds: None,
    //         fd: None,
    //         // pkcs12: None,
    //         headers: Vec::new(),
    //         workers: None,
    //         root_path: String::new(),
    //         // date_header_enabled: true,
    //         // server_header_enabled: true,
    //         // proxy_headers_enabled: true,
    //         // access_log: true,
    //     })
    // }

    /// default is "127.0.0.1"
    fn set_hostport(&mut self, host: &str, port: u16) -> PyResult<()> {
        self.addrs = (host, port)
            .to_socket_addrs()
            .map_err(|err| anyhow::anyhow!("{}: {}", err, host))?
            // .map(|mut addr| {
            //     if addr.is_ipv4() {
            //         if let IpAddr::V4(v4) = addr.ip() {
            //             addr.set_ip(IpAddr::V6(v4.to_ipv6_mapped()))
            //         }
            //     }
            //     addr
            // })
            .collect();
        Ok(())
    }

    /// default is 8000
    fn set_port(&mut self, port: u16) -> PyResult<()> {
        self.addrs.iter_mut().for_each(|addr| addr.set_port(port));
        Ok(())
    }

    /*
    /// Bind to a UNIX domain socket.
    /// eg: `/tmp/quanshu.sock`
    fn set_uds(&mut self, uds: String) {
        self.uds.replace(uds);
    }

    fn set_fd(&mut self, fd: RawFd) {
        self.fd.replace(fd);
    }

    fn enable_access_log(&mut self, enable: bool) {
        self.access_log = enable;
    }

    fn set_workers(&mut self, workers: usize) {
        self.workers.replace(workers);
    }

    fn enable_date_header(&mut self, enable: bool) {
        self.date_header_enabled = enable;
    }

    fn enable_server_header(&mut self, enable: bool) {
        self.server_header_enabled = enable;
    }

    fn enable_proxy_headers(&mut self, enable: bool) {
        self.proxy_headers_enabled = enable;
    }

    fn workers(&self) -> usize {
        self.workers.map(|w| w + 1).unwrap_or(1)
    }

    /// A DER-formatted PKCS #12 archive, using the specified password to decrypt the key.
    /// PKCS #12 archives typically have the file extension .p12 or .pfx, and can be created with the OpenSSL pkcs12 tool:
    ///
    ///     openssl pkcs12 -export -out identity.pfx -inkey key.pem -in cert.pem -certfile chain_certs.pem
    ///
    /// See: [certificate](https://docs.rs/tokio-native-tls/0.3.0/tokio_native_tls/native_tls/struct.Identity.html#impl)
    fn set_certfile(&mut self, certfile: &str, password: Option<&str>) -> PyResult<()> {
        let cert = std::fs::read(certfile).map_err(|err| anyhow::anyhow!("{}", err))?;
        let identity = Identity::from_pkcs12(&cert, password.unwrap_or(""))
            .map_err(|err| anyhow::anyhow!("{}", err))?;
        self.pkcs12.replace(identity);
        Ok(())
    }

    /// Specify custom default HTTP response headers as a Name:Value pair
    fn set_headers(&mut self, headers: Vec<(String, String)>) -> PyResult<()> {
        self.headers.extend(headers);
        Ok(())
    }

    /// "Set the ASGI 'root_path' for applications submounted below a given URL path.",
    fn set_root_path(&mut self, root_path: String) {
        self.root_path = root_path;
    }
    */
}

enum Connection {
    TlsStream(TlsStream<TcpStream>),
    TcpStream(TcpStream),
}

impl AsyncRead for Connection {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Connection::TlsStream(ref mut s) => Pin::new(s).poll_read(cx, buf),
            Connection::TcpStream(ref mut s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Connection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            Connection::TlsStream(ref mut s) => Pin::new(s).poll_write(cx, buf),
            Connection::TcpStream(ref mut s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            Connection::TlsStream(ref mut s) => Pin::new(s).poll_flush(cx),
            Connection::TcpStream(ref mut s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            Connection::TlsStream(ref mut s) => Pin::new(s).poll_shutdown(cx),
            Connection::TcpStream(ref mut s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Acceptor(Option<tokio_native_tls::TlsAcceptor>);

impl Acceptor {
    #[inline]
    async fn accept(&self, stream: TcpStream) -> anyhow::Result<Connection> {
        Ok(if let Some(ref acceptor) = self.0 {
            Connection::TlsStream(acceptor.accept(stream).await?)
        } else {
            Connection::TcpStream(stream)
        })
    }
}

type PinnedDynSocketStream =
    Pin<Box<dyn Stream<Item = std::io::Result<(TcpStream, SocketAddr)>> + Send>>;

#[inline]
async fn bind(
    opts: &Options,
) -> anyhow::Result<stream::StreamMap<SocketAddr, PinnedDynSocketStream>> {
    let mut listening = vec![];
    let defautl_addr = opts.addrs[0];
    let mut map = stream::StreamMap::with_capacity(opts.addrs.len());
    if let Some(fd) = opts.fd {
        #[cfg(unix)]
        let socket = unsafe { socket2::Socket::from_raw_fd(fd) };
        #[cfg(windows)]
        let socket = unsafe { socket2::Socket::from_raw_socket(fd) };
        socket.set_only_v6(false).ok();
        socket.set_reuse_port(true)?;
        socket.set_reuse_address(true)?;
        socket.set_nonblocking(true)?;
        socket.listen(4096)?;

        let listener = TcpListener::from_std(socket.into())?;
        listening.push(listener.local_addr().unwrap());
        let stream = Box::pin(async_stream::try_stream! {
            loop {
                yield listener.accept().await?;
            }
        }) as PinnedDynSocketStream;
        map.insert(defautl_addr, stream);
    }
    if let Some(ref uds) = opts.uds {
        unimplemented!()
    } else {
        for addr in opts.addrs.iter() {
            let domain = socket2::Domain::for_address(*addr);
            let typ = socket2::Type::STREAM.nonblocking();
            let socket = socket2::Socket::new(domain, typ, Some(socket2::Protocol::TCP))?;
            socket.set_only_v6(false).ok();
            socket.set_reuse_port(true)?;
            socket.set_reuse_address(true)?;
            socket.bind(&(*addr).into())?;
            socket.set_nonblocking(true)?;
            socket.listen(4096)?;

            let listener = TcpListener::from_std(socket.into())?;
            listening.push(listener.local_addr().unwrap());
            let stream = Box::pin(async_stream::try_stream! {
                loop {
                    yield listener.accept().await?;
                }
            }) as PinnedDynSocketStream;
            map.insert(*addr, stream);
            log::info!("quanshu listening on {}", addr);
        }
    }
    println!("Started http server: {:?}", &listening);
    Ok(map)
}

/*
async fn serve(
    subsys: SubsystemHandle,
    locals: pyo3_asyncio::TaskLocals,
    opts: Options,
) -> anyhow::Result<()> {
    unimplemented!()
}
*/

#[inline]
async fn serve(
    subsys: SubsystemHandle,
    tx: UnboundedSender<asgi::RequestTask>,
    mut opts: Options,
) -> anyhow::Result<()> {
    // TODO: lifespan here

    let mut listeners = bind(&opts).await?;

    let acceptor = if let Some(pkcs12) = opts.pkcs12.take() {
        let der = tokio::fs::read(&pkcs12).await?;
        let identity = Identity::from_pkcs12(&der, "")?;
        Acceptor(Some(tokio_native_tls::TlsAcceptor::from(
            TlsAcceptor::builder(identity).build().map_err(|err| {
                PyErr::new::<PyValueError, _>(format!("Unsupported certificate {}", err))
            })?,
        )))
    } else {
        Acceptor(None)
    };

    // if opts.server_header_enabled
    //     && opts
    //         .headers
    //         .iter()
    //         .find(|(k, _)| k.to_lowercase() == "server")
    //         .is_none()
    // {
    //     opts.headers
    //         .push(("Server".to_string(), "quanshu".to_string()));
    // }

    let mut http = Http::new();
    http.http1_preserve_header_case(true);
    // http.http1_keep_alive(false);
    http.pipeline_flush(true);

    // let mut futures = vec![];

    let nested = subsys.start("shutingdown", shutingdown);

    loop {
        // Asynchronously wait for an inbound socket.
        let (stream, remote_addr) = tokio::select! {
            Some((_, next)) = listeners.next() => {
                next?
            }
            _ = subsys.on_shutdown_requested() => {
                break
            }
            else => break
        };

        let conn_accepted = ConnAccepted {
            local_addr: stream.local_addr()?,
            remote_addr,
            stream,
            acceptor: acceptor.clone(),
            http: http.clone(),
            opts: opts.clone(),
            tx: tx.clone(),
        };
        log::trace!("accept connection from {}", remote_addr);
        subsys.start("accept-connection", |subsys| conn_accepted.handle(subsys));
    }
    subsys.perform_partial_shutdown(nested).await.ok();
    // check
    // lifespan here
    Ok(())
}

struct ConnAccepted {
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    http: Http,
    opts: Options,
    stream: TcpStream,
    acceptor: Acceptor,
    tx: UnboundedSender<asgi::RequestTask>,
}

impl ConnAccepted {
    #[inline]
    async fn handle(self, _subsys: SubsystemHandle) -> anyhow::Result<()> {
        // Accept the TLS connection.
        let conn = self
            .acceptor
            .accept(self.stream)
            .await
            .expect("Accept failed");

        let begin = Instant::now();

        let service = service_fn({
            move |req| {
                let tx = self.tx.clone();
                let opts = self.opts.clone();
                async move {
                    let ctx = asgi::Context::new(tx, self.local_addr, self.remote_addr);
                    let asgi = asgi::Asgi::new(ctx, opts);
                    asgi.serve(req).await
                }
            }
        });
        // let service = service_fn(|_| async {
        //     Ok::<hyper::Response<hyper::Body>, anyhow::Error>(hyper::Response::new(
        //         hyper::Body::from("Hello World"),
        //     ))
        // });
        // In a loop, read data from the socket and write the data back.
        if let Err(err) = self.http.serve_connection(conn, service).await {
            log::warn!("Error while serving HTTP connection: {}", err);
        }
        Ok(())
    }
}

#[inline]
async fn shutingdown(subsys: SubsystemHandle) -> anyhow::Result<()> {
    SHUTDOWN.notified().await;
    subsys.request_shutdown();
    Ok(())
}

#[inline]
pub async fn graceful(tx: UnboundedSender<asgi::RequestTask>, opts: Options) -> PyResult<()> {
    let handle = Toplevel::new()
        .start("serving", |subsys| serve(subsys.clone(), tx, opts))
        .catch_signals()
        .handle_shutdown_requests(std::time::Duration::from_millis(500));

    // to map anyhow::Error -> pyo3::Error
    handle.await?;
    Ok(())
}

#[pyfunction]
pub fn shutdown() {
    SHUTDOWN.notify_one()
}

/// A Python module implemented in Rust.
#[pymodule]
fn quanshu(py: Python, m: &PyModule) -> PyResult<()> {
    // let logger = pyo3_log::Logger::new(py, pyo3_log::Caching::LoggersAndLevels)?;
    // logger
    //     .install()
    //     .map_err(|err| PyErr::new::<PyException, _>(format!("Cannot set Logger {}", err)))?;

    m.add_class::<Options>()?;
    m.add_function(wrap_pyfunction!(shutdown, m)?)?;

    Ok(())
}
