"""
Some light wrappers around Python's multiprocessing, to deal with cleanly
starting child processes.
"""
import asyncio
import multiprocessing as mp
import os
import sys
from multiprocessing.context import SpawnProcess
import typing as t

mp.allow_connection_pickling()
spawn = mp.get_context("spawn")
from . import quanshu as qs
from . import loggers, config, sock

def get_subprocess(
    config: config.Config,
    target: t.Callable[..., t.Coroutine],
) -> SpawnProcess:
    """
    Called in the parent process, to instantiate a new child process instance.
    The child is not yet started at this point.
    * config - The Uvicorn configuration instance.
    * target - A callable that accepts a list of sockets. In practice this will
               be the `Server.run()` method.
    * sockets - A list of sockets to pass to the server. Sockets are bound once
                by the parent process, and then passed to the child processes.
    """
    # We pass across the stdin fileno, and reopen it in the child process.
    # This is required for some debugging environments.
    stdin_fileno: t.Optional[int]
    try:
        stdin_fileno = sys.stdin.fileno()
    except OSError:
        stdin_fileno = None

    kwargs = {
        "config": config,
        "target": target,
        "stdin_fileno": stdin_fileno,
    }

    return spawn.Process(target=subprocess_started, kwargs=kwargs)


def subprocess_started(
    config: config.Config,
    target: t.Callable[..., t.Coroutine],
    stdin_fileno: t.Optional[int],
) -> None:
    """
    Called when the child process starts.
    * config - The Uvicorn configuration instance.
    * target - A callable that accepts a list of sockets. In practice this will
               be the `Server.run()` method.
    * sockets - A list of sockets to pass to the server. Sockets are bound once
                by the parent process, and then passed to the child processes.
    * stdin_fileno - The file number of sys.stdin, so that it can be reattached
                     to the child process.
    """
    # Re-open stdin.
    if stdin_fileno is not None:
        sys.stdin = os.fdopen(stdin_fileno)

    # Logging needs to be setup again for each child.
    loggers.setup(config.log_config, config.log_level, config.access_log)
    if config.loop == "uvloop":
        import uvloop
        uvloop.install()

    print("running in subprocess");
    asyncio.run(target(config.options(), config.socket),debug=True)
