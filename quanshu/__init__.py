import os
import sys
import platform
import typing as t
import asyncio
import logging
import logging.config

import click

from .quanshu import *

__doc__ = quanshu.__doc__

TRACE_LOG_LEVEL = 5

LOG_LEVELS = {
    "critical": logging.CRITICAL,
    "error": logging.ERROR,
    "warning": logging.WARNING,
    "info": logging.INFO,
    "debug": logging.DEBUG,
    "trace": TRACE_LOG_LEVEL,
}

LOOP_CHOICES = click.Choice(["uvloop", "asyncio"])
INTERFACE_CHOICES = click.Choice(["asgi2", "asgi3"])
LEVEL_CHOICES = click.Choice([])

LOGGING_CONFIG: dict = {
    "version": 1,
    "disable_existing_loggers": False,
    "formatters": {
        "default": {
            "()": "quanshu.loggers.DefaultFormatter",
            "fmt": "%(levelprefix)s %(message)s",
            "use_colors": None,
        },
        # "fmt": '%(levelprefix)s %(client_addr)s - "%(request_line)s" %(status_code)s',  # noqa: E501
    },
    "handlers": {
        "default": {
            "formatter": "default",
            "class": "logging.StreamHandler",
            "stream": "ext://sys.stderr",
        },
    },
    "loggers": {
        "quanshu": {"handlers": ["default"], "level": "INFO"},
        "quanshu.error": {"level": "INFO"},
    },
}


def print_version(ctx: click.Context, param: click.Parameter, value: bool) -> None:
    if not value or ctx.resilient_parsing:
        return
    click.echo(
        "Running quanshu %s with %s %s on %s"
        % (
            quanshu.__version__,
            platform.python_implementation(),
            platform.python_version(),
            platform.system(),
        )
    )
    ctx.exit()

@click.command(context_settings={"auto_envvar_prefix": "QUANSHU"})
@click.argument("app")
@click.option(
    "--host",
    type=str,
    default="127.0.0.1",
    help="Bind socket to this host.",
    show_default=True,
)
@click.option(
    "--port",
    type=int,
    default=8000,
    help="Bind socket to this port.",
    show_default=True,
)
@click.option("--uds", type=str, default=None, help="Bind to a UNIX domain socket.")
@click.option(
    "--fd", type=int, default=None, help="Bind to socket from this file descriptor."
)
@click.option(
    "--debug", is_flag=True, default=False, help="Enable debug mode.", hidden=True
)
@click.option(
    "--workers",
    default=None,
    type=int,
    help="Number of worker processes. Defaults to the $WEB_CONCURRENCY environment"
    " variable if available, or 1. Not valid with --reload.",
)
@click.option(
    "--loop",
    type=LOOP_CHOICES,
    default="uvloop",
    help="Event loop implementation.",
    show_default=True,
)
@click.option(
    "--ws-max-size",
    type=int,
    default=16777216,
    help="WebSocket max size message in bytes",
    show_default=True,
)
@click.option(
    "--ws-ping-interval",
    type=float,
    default=20.0,
    help="WebSocket ping interval",
    show_default=True,
)
@click.option(
    "--ws-ping-timeout",
    type=float,
    default=20.0,
    help="WebSocket ping timeout",
    show_default=True,
)
@click.option(
    "--ws-per-message-deflate",
    type=bool,
    default=True,
    help="WebSocket per-message-deflate compression",
    show_default=True,
)
@click.option(
    "--lifespan",
    type=bool,
    default=True,
    help="Lifespan implementation.",
    show_default=True,
)
@click.option(
    "--interface",
    type=INTERFACE_CHOICES,
    default="asgi3",
    help="Select ASGI3, ASGI2, or WSGI as the application interface.",
    show_default=True,
)
@click.option(
    "--env-file",
    type=click.Path(exists=True),
    default=None,
    help="Environment configuration file.",
    show_default=True,
)
@click.option(
    "--log-config",
    type=click.Path(exists=True),
    default=None,
    help="Logging configuration file. Supported formats: .ini, .json, .yaml.",
    show_default=True,
)
@click.option(
    "--log-level",
    type=LEVEL_CHOICES,
    default=None,
    help="Log level. [default: info]",
    show_default=True,
)
@click.option(
    "--access-log/--no-access-log",
    is_flag=True,
    default=True,
    help="Enable/Disable access log.",
)
@click.option(
    "--use-colors/--no-use-colors",
    is_flag=True,
    default=None,
    help="Enable/Disable colorized logging.",
)
@click.option(
    "--proxy-headers/--no-proxy-headers",
    is_flag=True,
    default=True,
    help="Enable/Disable X-Forwarded-Proto, X-Forwarded-For, X-Forwarded-Port to "
    "populate remote address info.",
)
@click.option(
    "--server-header/--no-server-header",
    is_flag=True,
    default=True,
    help="Enable/Disable default Server header.",
)
@click.option(
    "--date-header/--no-date-header",
    is_flag=True,
    default=True,
    help="Enable/Disable default Date header.",
)
@click.option(
    "--forwarded-allow-ips",
    type=str,
    default=None,
    help="Comma seperated list of IPs to trust with proxy headers. Defaults to"
    " the $FORWARDED_ALLOW_IPS environment variable if available, or '127.0.0.1'.",
)
@click.option(
    "--root-path",
    type=str,
    default="",
    help="Set the ASGI 'root_path' for applications submounted below a given URL path.",
)
@click.option(
    "--limit-concurrency",
    type=int,
    default=None,
    help="Maximum number of concurrent connections or tasks to allow, before issuing"
    " HTTP 503 responses.",
)
@click.option(
    "--backlog",
    type=int,
    default=2048,
    help="Maximum number of connections to hold in backlog",
)
@click.option(
    "--limit-max-requests",
    type=int,
    default=None,
    help="Maximum number of requests to service before terminating the process.",
)
@click.option(
    "--timeout-keep-alive",
    type=int,
    default=5,
    help="Close Keep-Alive connections if no new data is received within this timeout.",
    show_default=True,
)
@click.option(
    "--ssl-certfile",
    type=str,
    default=None,
    help="A DER-formatted PKCS #12 archive, typically have the file extension .p12 or .pfx",
    show_default=True,
)
@click.option(
    "--ssl-key-password",
    type=str,
    default=None,
    help="SSL key's password",
    show_default=True,
)
@click.option(
    "--ssl-ciphers",
    type=str,
    default="TLSv1",
    help="Ciphers to use (see https://docs.rs/tokio-native-tls/latest/tokio_native_tls/native_tls/enum.Protocol.html)",
    show_default=True,
)
@click.option(
    "--header",
    "headers",
    multiple=True,
    help="Specify custom default HTTP response headers as a Name:Value pair",
)
@click.option(
    "--version",
    is_flag=True,
    callback=print_version,
    expose_value=False,
    is_eager=True,
    help="Display the quanshu version and exit.",
)
@click.option(
    "--app-dir",
    default=".",
    show_default=True,
    help="Look for APP in the specified directory, by adding this to the PYTHONPATH."
    " Defaults to the current working directory.",
)
def main(
    app: str,
    host: str,
    port: int,
    uds: str,
    fd: int,
    loop: str,
    ws_max_size: int,
    ws_ping_interval: float,
    ws_ping_timeout: float,
    ws_per_message_deflate: bool,
    lifespan: str,
    interface: str,
    debug: bool,
    workers: int,
    env_file: str,
    log_config: str,
    log_level: str,
    access_log: bool,
    proxy_headers: bool,
    server_header: bool,
    date_header: bool,
    forwarded_allow_ips: str,
    root_path: str,
    limit_concurrency: int,
    backlog: int,
    limit_max_requests: int,
    timeout_keep_alive: int,
    ssl_certfile: str,
    ssl_key_password: str,
    ssl_ciphers: str,
    headers: t.List[str],
    use_colors: bool,
    app_dir: str,
    ):
    if app_dir is not None:
        sys.path.insert(0, app_dir)
    setup_logging(log_config or LOGGING_CONFIG, log_level, access_log, use_colors)
    if loop == "uvloop":
        import uvloop
        uvloop.install()
    opts = quanshu.Options(app)
    opts.set_port(port)
    opts.set_host(host)
    opts.set_headers(headers)
    if ssl_certfile:
        opts.set_certfile(ssl_certfile, ssl_key_password)
    if root_path:
        opts.set_root_path(root_path)
    asyncio.run(run(opts))
    if uds:
        os.remove(uds)  # pragma: py-win32

async def run(opts: quanshu.Options):
    """workaround for https://pyo3.rs/v0.16.2/ecosystem/async-await.html#a-note-about-asynciorun
    """
    await quanshu.run(opts)

def setup_logging(log_config: t.Optional[t.Union[t.Dict, str, None]],
                  log_level: t.Optional[t.Union[int, str, None]],
                  access_log: bool,
                  use_colors: bool = False):
    logging.addLevelName(TRACE_LOG_LEVEL, "TRACE")

    if log_config is not None:
        if isinstance(log_config, dict):
            if use_colors in (True, False):
                log_config["formatters"]["default"][
                    "use_colors"
                ] = use_colors
                log_config["formatters"]["access"][
                    "use_colors"
                ] = use_colors
            logging.config.dictConfig(log_config)
        elif log_config.endswith(".json"):
            import json
            with open(log_config) as file:
                loaded_config = json.load(file)
                logging.config.dictConfig(loaded_config)
        elif log_config.endswith((".yaml", ".yml")):
            import yaml
            with open(log_config) as file:
                loaded_config = yaml.safe_load(file)
                logging.config.dictConfig(loaded_config)
        else:
            # See the note about fileConfig() here:
            # https://docs.python.org/3/library/logging.config.html#configuration-file-format
            logging.config.fileConfig(
                log_config, disable_existing_loggers=False
            )

    if log_level is not None:
        if isinstance(log_level, str):
            log_level = LOG_LEVELS[log_level]
        else:
            log_level = log_level
        logging.getLogger("quanshu.error").setLevel(log_level)
        logging.getLogger("quanshu.access").setLevel(log_level)
        logging.getLogger("quanshu.asgi").setLevel(log_level)
    if access_log is False:
        logging.getLogger("quanshu.access").handlers = []
        logging.getLogger("quanshu.access").propagate = False
