
import os
import signal
import logging
import threading
from types import FrameType
import typing as t
import dataclasses
from multiprocessing.context import SpawnProcess

import click

from quanshu.subprocess import get_subprocess
from . import quanshu as qs
from .config import Config

HANDLED_SIGNALS = (
    signal.SIGINT,  # Unix signal 2. Sent by Ctrl+C.
    signal.SIGTERM,  # Unix signal 15. Sent by `kill <pid>`.
)

logger = logging.getLogger("quanshu.error")


class Supervisor:
    def __init__(
        self,
        config: Config,
        target: t.Callable[..., t.Coroutine],
    ) -> None:
        self.config = config
        self.target = target
        self.processes: t.List[SpawnProcess] = []
        self.should_exit = threading.Event()
        self.pid = os.getpid()

    def signal_handler(self, sig: signal.Signals, frame: FrameType) -> None:
        """
        A signal handler that is registered with the parent process.
        """
        self.should_exit.set()

    def run(self) -> None:
        self.startup()
        self.should_exit.wait()
        self.shutdown()

    def startup(self) -> None:
        message = "Started parent process [{}]".format(str(self.pid))
        color_message = "Started parent process [{}]".format(
            click.style(str(self.pid), fg="cyan", bold=True)
        )
        logger.info(message, extra={"color_message": color_message})

        for sig in HANDLED_SIGNALS:
            signal.signal(sig, self.signal_handler)

        for idx in range(self.config.workers):
            process = get_subprocess(
                config=self.config, target=self.target
            )
            process.start()
            self.processes.append(process)

    def shutdown(self) -> None:
        for process in self.processes:
            process.terminate()
            process.join()

        message = "Stopping parent process [{}]".format(str(self.pid))
        color_message = "Stopping parent process [{}]".format(
            click.style(str(self.pid), fg="cyan", bold=True)
        )
        logger.info(message, extra={"color_message": color_message})
