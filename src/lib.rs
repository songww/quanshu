mod asgi;

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str::FromStr;

use hyper::{server::conn::Http, service::service_fn};
use pyo3::exceptions::{PyException, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyString;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpSocket, TcpStream};
use tokio_native_tls::native_tls::{Identity, TlsAcceptor};
use tokio_native_tls::TlsStream;

#[cfg(unix)]
type RawFd = std::os::unix::io::RawFd;
#[cfg(windows)]
type RawFd = std::os::windows::io::RawSocket;

#[pyclass]
#[derive(Clone)]
struct Options {
    app: PyObject,
    host: IpAddr,
    port: u16,
    uds: Option<String>,
    fd: Option<RawFd>,
    pkcs12: Option<Identity>,
    headers: Vec<(String, String)>,
    workers: Option<usize>,
    root_path: String,
    date_header_enabled: bool,
    server_header_enabled: bool,
    proxy_headers_enabled: bool,
}

#[pymethods]
impl Options {
    #[new]
    fn new(py: Python, app: &PyAny) -> PyResult<Options> {
        if app.is_none() {
            return Err(PyErr::new::<PyTypeError, _>(
                "Invalid app type, string or callable object expected.",
            ));
        }
        let app = {
            if app.is_instance_of::<PyString>()? {
                let s = app.cast_as::<PyString>()?.to_str()?;
                let (module, attrs) = s.split_once(":").ok_or_else(|| {
                    PyErr::new::<PyValueError, _>(format!(
                        "app string '{}' must be in format '<module>:<attribute>'.",
                        s
                    ))
                })?;
                let mut instance = py.import(&module)?.as_ref();
                for attr in attrs.split(".") {
                    instance = instance.getattr(attr)?
                }
                if !instance.is_callable() {
                    return Err(PyErr::new::<PyValueError, _>(format!(
                        "app '{}' is not callable!",
                        s
                    )));
                }
                instance
            } else if !app.is_callable() {
                return Err(PyErr::new::<PyValueError, _>(format!(
                    "app '{}' is not callable!",
                    app
                )));
            } else {
                app
            }
        };
        Ok(Options {
            app: app.to_object(py),
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8000,
            uds: None,
            fd: None,
            pkcs12: None,
            headers: Vec::new(),
            workers: None,
            root_path: String::new(),
            date_header_enabled: true,
            server_header_enabled: true,
            proxy_headers_enabled: true,
        })
    }

    /// default is "127.0.0.1"
    fn set_host(&mut self, host: &str) -> PyResult<()> {
        self.host = IpAddr::from_str(host).map_err(|err| anyhow::anyhow!(err))?;
        Ok(())
    }

    /// default is 8000
    fn set_port(&mut self, port: u16) -> PyResult<()> {
        self.port = port;
        Ok(())
    }

    /// Bind to a UNIX domain socket.
    /// eg: `/tmp/quanshu.sock`
    fn set_uds(&mut self, uds: String) {
        self.uds.replace(uds);
    }

    fn set_fd(&mut self, fd: RawFd) {
        self.fd.replace(fd);
    }
    fn set_workers(&mut self, workers: usize) {
        self.workers.replace(workers);
    }

    fn enable_date_header(&mut self, enable: bool) {
        self.date_header_enabled = enable;
    }

    fn enable_server_header(&mut self, enable: bool) {
        println!("server header enabled: {}", enable);
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
}

enum Stream {
    TlsStream(TlsStream<TcpStream>),
    TcpStream(TcpStream),
}

impl AsyncRead for Stream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Stream::TlsStream(ref mut s) => Pin::new(s).poll_read(cx, buf),
            Stream::TcpStream(ref mut s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            Stream::TlsStream(ref mut s) => Pin::new(s).poll_write(cx, buf),
            Stream::TcpStream(ref mut s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            Stream::TlsStream(ref mut s) => Pin::new(s).poll_flush(cx),
            Stream::TcpStream(ref mut s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            Stream::TlsStream(ref mut s) => Pin::new(s).poll_shutdown(cx),
            Stream::TcpStream(ref mut s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Acceptor(Option<tokio_native_tls::TlsAcceptor>);

impl Acceptor {
    async fn accept(&self, stream: TcpStream) -> anyhow::Result<Stream> {
        Ok(if let Some(ref acceptor) = self.0 {
            Stream::TlsStream(acceptor.accept(stream).await?)
        } else {
            Stream::TcpStream(stream)
        })
    }
}

async fn serve(locals: pyo3_asyncio::TaskLocals, mut opts: Options) -> PyResult<()> {
    let addr = SocketAddr::new(opts.host, opts.port);

    let sock = if addr.is_ipv4() {
        TcpSocket::new_v4()?
    } else {
        TcpSocket::new_v6()?
    };
    sock.set_reuseaddr(true)?;
    sock.set_reuseport(true)?;
    sock.bind(addr)?;

    // let listener: TcpListener = TcpListener::bind(&addr).await?;
    let listener = sock.listen(1024)?;

    let acceptor = if let Some(pkcs12) = opts.pkcs12.take() {
        Acceptor(Some(tokio_native_tls::TlsAcceptor::from(
            TlsAcceptor::builder(pkcs12).build().map_err(|err| {
                PyErr::new::<PyValueError, _>(format!("Unsupported certificate {}", err))
            })?,
        )))
    } else {
        Acceptor(None)
    };

    if opts.server_header_enabled
        && opts
            .headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == "server")
            .is_none()
    {
        opts.headers
            .push(("Server".to_string(), "quanshu".to_string()));
    }

    let mut http = Http::new();
    // http.http1_keep_alive(true);
    http.http1_preserve_header_case(true);

    loop {
        // Asynchronously wait for an inbound socket.
        let (socket, remote_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let http = http.clone();
        let opts = opts.clone();
        let app = opts.app.clone();
        let locals = locals.clone();
        println!("accept connection from {}", remote_addr);
        tokio::spawn(async move {
            // Accept the TLS connection.
            // let mut conn = tls_acceptor.accept(socket).await.expect("accept error");
            let conn = acceptor.accept(socket).await.expect("accept error");

            let service = service_fn({
                move |req| {
                    let app = app.clone();
                    let opts = opts.clone();
                    let locals = locals.clone();
                    pyo3_asyncio::tokio::scope(locals.clone(), async move {
                        let ctx = asgi::Context::new(locals, addr, remote_addr);
                        let asgi = asgi::Asgi::new(app, ctx, opts);
                        asgi.serve(req).await
                    })
                }
            });
            // In a loop, read data from the socket and write the data back.
            if let Err(err) = http.serve_connection(conn, service).await {
                eprintln!("Error while serving HTTP connection: {:?}", err);
            }
        });
    }
}

#[pyfunction]
fn run<'p>(py: Python<'p>, opts: PyRef<Options>) -> PyResult<&'p PyAny> {
    let opts: Options = opts.clone();

    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder
        .worker_threads(1)
        .thread_name("quanshu-worker")
        .enable_io()
        .enable_time();

    pyo3_asyncio::tokio::init(builder);

    let locals = Python::with_gil(|py| pyo3_asyncio::tokio::get_current_locals(py))?;

    pyo3_asyncio::tokio::future_into_py(py, serve(locals, opts)).into()
}

/// A Python module implemented in Rust.
#[pymodule]
fn quanshu(py: Python, m: &PyModule) -> PyResult<()> {
    let logger = pyo3_log::Logger::new(py, pyo3_log::Caching::LoggersAndLevels)?;
    logger
        .install()
        .map_err(|err| PyErr::new::<PyException, _>(format!("Cannot set Logger {}", err)))?;

    m.add_class::<Options>()?;
    m.add_function(wrap_pyfunction!(run, m)?)?;

    Ok(())
}
