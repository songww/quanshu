import http
import logging
import logging.config
import sys
from copy import copy
import typing as t

import click

TRACE_LOG_LEVEL = 5

LOG_LEVELS = {
    "critical": logging.CRITICAL,
    "error": logging.ERROR,
    "warning": logging.WARNING,
    "info": logging.INFO,
    "debug": logging.DEBUG,
    "trace": TRACE_LOG_LEVEL,
}


class ColourizedFormatter(logging.Formatter):
    """
    A custom log formatter class that:
    * Outputs the LOG_LEVEL with an appropriate color.
    * If a log call includes an `extras={"color_message": ...}` it will be used
      for formatting the output, instead of the plain text message.
    """

    level_name_colors = {
        TRACE_LOG_LEVEL: lambda level_name: click.style(str(level_name), fg="blue"),
        logging.DEBUG: lambda level_name: click.style(str(level_name), fg="cyan"),
        logging.INFO: lambda level_name: click.style(str(level_name), fg="green"),
        logging.WARNING: lambda level_name: click.style(str(level_name), fg="yellow"),
        logging.ERROR: lambda level_name: click.style(str(level_name), fg="red"),
        logging.CRITICAL: lambda level_name: click.style(
            str(level_name), fg="bright_red"
        ),
    }

    def __init__(
        self,
        fmt: t.Optional[str] = None,
        datefmt: t.Optional[str] = None,
        style: str = "%",
        use_colors: t.Optional[bool] = None,
    ):
        if use_colors in (True, False):
            self.use_colors = use_colors
        else:
            self.use_colors = sys.stdout.isatty()
        super().__init__(fmt=fmt, datefmt=datefmt, style=style)

    def color_level_name(self, level_name: str, level_no: int) -> str:
        def default(level_name: str) -> str:
            return str(level_name)  # pragma: no cover

        func = self.level_name_colors.get(level_no, default)
        return func(level_name)

    def should_use_colors(self) -> bool:
        return True  # pragma: no cover

    def formatMessage(self, record: logging.LogRecord) -> str:
        recordcopy = copy(record)
        levelname = recordcopy.levelname
        seperator = " " * (8 - len(recordcopy.levelname))
        if self.use_colors:
            levelname = self.color_level_name(levelname, recordcopy.levelno)
            if "color_message" in recordcopy.__dict__:
                recordcopy.msg = recordcopy.__dict__["color_message"]
                recordcopy.__dict__["message"] = recordcopy.getMessage()
        recordcopy.__dict__["levelprefix"] = levelname + ":" + seperator
        return super().formatMessage(recordcopy)


class DefaultFormatter(ColourizedFormatter):
    def should_use_colors(self) -> bool:
        return sys.stderr.isatty()  # pragma: no cover


def setup(log_config: t.Optional[t.Union[t.Dict, str, None]],
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
