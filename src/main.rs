use std::thread;

use clap::Parser;
use pyo3::prelude::*;
use pyo3_asyncio::TaskLocals;

use quanshu as qs;

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
            .call1((0, cwd.as_os_str()))?;

        // store a reference for the assertion
        // let uvloop = PyObject::from(uvloop);

        let asyncio = py.import("asyncio")?;
        let event_loop = asyncio.getattr("get_event_loop")?.call0()?;

        let locals = TaskLocals::new(event_loop);

        println!("before aio run");
        let rt = py.allow_threads(|| {
            thread::Builder::new()
                .name("tokio".to_string())
                .spawn(|| {
                    let mut builder = tokio::runtime::Builder::new_current_thread();
                    builder.enable_all();
                    pyo3_asyncio::tokio::init(builder);
                    pyo3_asyncio::tokio::get_runtime()
                        .block_on(pyo3_asyncio::tokio::scope(locals, _main()))
                })
                .expect("spawn failed")
        });

        println!("before run_forever");
        event_loop
            .call_method0("run_forever")
            .map(|v| v.to_object(py))
    })?;
    Ok(())
}

async fn _main() -> PyResult<()> {
    let locals = unsafe {
        Python::with_gil_unchecked(|py| {
            let locals = pyo3_asyncio::tokio::get_current_locals(py).unwrap();
            println!("event loop: {}", locals.event_loop(py));
            locals
        })
    };

    let opts = qs::Options::parse();
    // println!("opts: {:?}", opts);
    // println!("Started http server: {:?}", &opts.addrs);

    qs::graceful(locals, opts).await?;
    println!("Stopped http server");

    Ok(())
}
