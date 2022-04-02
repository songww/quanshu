use std::{sync::Arc, thread};

use clap::Parser;
use pyo3::prelude::*;
use pyo3_futures::PyAsync;
use tokio::sync::{
    mpsc::{unbounded_channel as unbounded, UnboundedReceiver, UnboundedSender},
    Mutex,
};

use quanshu as qs;

static PYSERVING: &'static str = "
import asyncio
async def serving(rx, app):
    tasks = []
    while True:
        task = await rx.recv()
        task = asyncio.create_task(app(task.scope(), task.receive, task.send))
        # tasks.append(task)
        # task.add_done_callback(lambda task: tasks.remove(task))
";

#[pyclass]
pub struct TaskReceiver {
    rx: Arc<Mutex<UnboundedReceiver<qs::RequestTask>>>,
}

impl TaskReceiver {
    fn new(rx: UnboundedReceiver<qs::RequestTask>) -> Self {
        TaskReceiver {
            rx: Arc::new(Mutex::new(rx)),
        }
    }
}

#[pymethods]
impl TaskReceiver {
    fn recv(&self) -> PyAsync<qs::RequestTask> {
        let rx = self.rx.clone();
        async move {
            let mut rx = rx.lock().await;
            rx.recv().await.unwrap()
        }
        .into()
    }
}

fn main() -> anyhow::Result<()> {
    pyo3::prepare_freethreaded_python();

    println!("tokio rt initialized");
    Python::with_gil(|py| -> PyResult<PyObject> {
        let uvloop = py.import("uvloop")?;
        uvloop.call_method0("install")?;

        let cwd = std::env::current_dir()?;
        py.import("sys")?
            .getattr("path")?
            .getattr("insert")?
            .call1((0usize, cwd.as_os_str()))?;

        // store a reference for the assertion
        // let uvloop = PyObject::from(uvloop);

        let asyncio = py.import("asyncio")?;
        let event_loop = asyncio.getattr("get_event_loop")?.call0()?;

        let opts = qs::Options::parse();
        let pyapp = opts.app.clone();
        // println!("opts: {:?}", opts);

        let (tx, rx) = unbounded();

        println!("before aio run");
        let _rt = py.allow_threads(|| {
            thread::Builder::new()
                .name("tokio".to_string())
                .spawn(|| {
                    let mut builder = tokio::runtime::Builder::new_current_thread();
                    let rt = builder.enable_all().build()?;
                    rt.block_on(_main(tx, opts))
                })
                .expect("spawn failed")
        });

        let serving_mod = PyModule::from_code(py, PYSERVING, "serving.py", "serving").unwrap();
        let serving = serving_mod.getattr("serving").unwrap();
        // let partial = py.import("functools").unwrap().getattr("partial").unwrap();
        // let serving = partial.call1((serving, rx, pyapp)).unwrap().call0();
        let serving = serving.call1((TaskReceiver::new(rx), pyapp)).unwrap();

        println!("before run_forever");
        event_loop
            .call_method1("run_until_complete", (serving,))
            .map(|v| v.to_object(py))
    })?;
    Ok(())
}

async fn _main(tx: UnboundedSender<qs::RequestTask>, opts: qs::Options) -> PyResult<()> {
    // let locals = unsafe {
    //     Python::with_gil_unchecked(|py| {
    //         let locals = pyo3_asyncio::tokio::get_current_locals(py).unwrap();
    //         println!("event loop: {}", locals.event_loop(py));
    //         locals
    //     })
    // };

    // println!("Started http server: {:?}", &opts.addrs);

    qs::graceful(tx, opts).await?;
    println!("Stopped http server");

    Ok(())
}
