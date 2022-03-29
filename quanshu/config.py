import dataclasses
import typing as t
from functools import cached_property

from . import quanshu as qs

@dataclasses.dataclass
class Config:
    app: t.Union[t.Callable[..., None], str]
    host: str
    port: int
    uds: str
    fd: int
    loop: str
    ws_max_size: int
    ws_ping_interval: float
    ws_ping_timeout: float
    ws_per_message_deflate: bool
    lifespan: str
    interface: str
    debug: bool
    workers: int
    env_file: str
    proxy_headers: bool
    server_header: bool
    date_header: bool
    forwarded_allow_ips: str
    root_path: str
    limit_concurrency: int
    backlog: int
    limit_max_requests: int
    timeout_keep_alive: int
    ssl_certfile: str
    ssl_key_password: str
    ssl_ciphers: str
    headers: t.List[str]
    app_dir: str
    log_config: t.Optional[t.Union[t.Dict, str]]
    log_level: t.Optional[t.Union[int, str]]
    access_log: bool
    use_colors: bool = False

    socket: t.Optional[qs.Socket] = None

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
        opts.set_bind_options(self.bind_options)
        print("bind options")
        # opts.set_socket(self.socket.try_clone())
        opts.set_headers(self.headers)
        if self.ssl_certfile:
            opts.set_certfile(self.ssl_certfile, self.ssl_key_password)
        if self.root_path:
            opts.set_root_path(self.root_path)
        return opts
