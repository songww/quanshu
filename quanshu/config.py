import dataclasses
import typing as t

from click.types import Path

from . import quanshu as qs

@dataclasses.dataclass
class Config:
    app: t.Union[t.Callable[..., None], str]
    host: str = "127.0.0.1"
    port: int = 8000
    uds: t.Optional[str] = None
    fd: t.Optional[str] = None
    loop: str = "uvloop"
    ws_max_size: int = 16777216
    ws_ping_interval: float = 20.0
    ws_ping_timeout: float = 20.0
    ws_per_message_deflate: bool = True
    lifespan: bool = True
    interface: str = "asgi3"
    debug: bool = False
    workers: t.Optional[int] = None
    env_file: t.Optional[str] = None
    proxy_headers: bool = True
    server_header: bool = True
    date_header: bool = True
    forwarded_allow_ips: t.Optional[str] = None
    root_path: str = ""
    limit_concurrency: t.Optional[int] = None
    backlog: int = 2048
    limit_max_requests: t.Optional[int] = None
    timeout_keep_alive: int = 5
    ssl_certfile: t.Optional[str] = None
    ssl_key_password: str = ""
    ssl_ciphers: str = "TLSv1"
    headers: t.Optional[t.List[str]] = None
    app_dir: str = "."
    log_config: t.Optional[t.Union[t.Dict, Path, str]] = None
    log_level: t.Optional[t.Union[int, str]] = "info"
    access_log: bool = True
    use_colors: bool = False

    @property
    def bind_options(self):
        opts = qs.BindOptions()
        opts.set_port(self.port)
        opts.set_host(self.host)
        if self.uds:
            opts.set_uds(self.uds)
        if self.fd:
            opts.set_fd(self.fd)
        return opts

    def options(self):
        opts = qs.Options(self.app)
        # opts.set_bind_options(self.bind_options)
        opts.set_port(self.port)
        opts.set_host(self.host)
        if self.uds:
            opts.set_uds(self.uds)
        if self.fd:
            opts.set_fd(self.fd)
        if self.headers:
            opts.set_headers(self.headers)
        if self.ssl_certfile:
            opts.set_certfile(self.ssl_certfile, self.ssl_key_password)
        if self.root_path:
            opts.set_root_path(self.root_path)
        opts.enable_date_header(self.date_header)
        opts.enable_server_header(self.server_header)
        opts.enable_proxy_headers(self.proxy_headers)
        return opts
