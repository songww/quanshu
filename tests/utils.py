import asyncio
import os
from contextlib import asynccontextmanager, contextmanager
from pathlib import Path

from quanshu import Config, serve
from quanshu import quanshu as qs


@asynccontextmanager
async def run_server(config: Config, sockets=None):
    serving = serve(config.options())
    cancel_handle = asyncio.ensure_future(serving)
    await asyncio.sleep(0.1)
    try:
        yield serving
    finally:
        qs.shutdown()
        await asyncio.sleep(0.5)
        cancel_handle.cancel()


@contextmanager
def as_cwd(path: Path):
    """Changes working directory and returns to previous on exit."""
    prev_cwd = Path.cwd()
    os.chdir(path)
    try:
        yield
    finally:
        os.chdir(prev_cwd)
