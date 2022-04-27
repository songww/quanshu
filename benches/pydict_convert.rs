use std::any::Any;
use std::collections::HashMap;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pyo3::prelude::*;
use pyo3::types::*;

type Elements<'a> = (
    (&'a str, usize),
    (&'a str, &'a str),
    (&'a str, &'a str),
    (&'a str, Vec<u8>),
    (&'a str, usize),
    (&'a str, &'a str),
    (&'a str, &'a str),
    (&'a str, Vec<u8>),
    (&'a str, f64),
    (&'a str, &'a str),
    (&'a str, &'a str),
    (&'a str, ((&'a str, &'a str), (&'a str, &'a str))),
);

pub fn criterion_benchmark(c: &mut Criterion) {
    let elements = (
        ("k1", 1usize),
        ("k2", "v2"),
        ("k3", "longstring"),
        ("k4", vec![1u8, 0, 1, 0, 1, 0]),
        ("k5", 5usize),
        ("k6", "v6"),
        ("k7", "longstring7"),
        ("k8", vec![1u8, 0, 1, 0, 1, 0]),
        ("k9", 5.0f64),
        ("k10", "v6"),
        ("k11", "longstring7"),
        ("k12", (("nested1", "value1"), ("nested2", "value2"))),
    );
    let hashmap = Python::with_gil(|py| {
        let elements = elements.clone();
        let mut hashmap = HashMap::new();
        hashmap.insert(elements.0 .0.to_string(), elements.0 .1.into_py(py));
        hashmap.insert(elements.1 .0.to_string(), elements.1 .1.into_py(py));
        hashmap.insert(elements.2 .0.to_string(), elements.2 .1.into_py(py));
        hashmap.insert(elements.3 .0.to_string(), elements.3 .1.into_py(py));
        hashmap.insert(elements.4 .0.to_string(), elements.4 .1.into_py(py));
        hashmap.insert(elements.5 .0.to_string(), elements.5 .1.into_py(py));
        hashmap.insert(elements.6 .0.to_string(), elements.6 .1.into_py(py));
        hashmap.insert(elements.7 .0.to_string(), elements.7 .1.into_py(py));
        hashmap.insert(elements.8 .0.to_string(), elements.8 .1.into_py(py));
        hashmap.insert(elements.9 .0.to_string(), elements.9 .1.into_py(py));
        hashmap.insert(elements.10 .0.to_string(), elements.10 .1.into_py(py));
        let mut elem = HashMap::new();
        let v = elements.11 .1;
        elem.insert(v.0 .0.to_string(), v.0 .1.into_py(py));
        elem.insert(v.1 .0.to_string(), v.1 .1.into_py(py));
        hashmap
            .insert(
                elements.11 .0.to_string(),
                elem.into_py_dict(py).into_py(py),
            );
        hashmap
    });
    let mut group = c.benchmark_group("pydict");
    group.bench_function("from hashmap", |b| {
        b.iter(|| from_hashmap(black_box(hashmap.clone())))
    });
    group.bench_function("from hashmap -py", |b| {
        Python::with_gil(|py| b.iter(|| from_hashmap_py(py, black_box(hashmap.clone()))));
    });
    group.bench_function("from tuple object", |b| {
        Python::with_gil(|py| {
            let elements = elements.clone().to_object(py);
            b.iter(move || from_tuple_py_object(py, black_box(elements.clone())));
        })
    });
    group.bench_function("from tuple", |b| {
        b.iter(|| from_tuple(black_box(elements.clone())))
    });
    group.bench_function("from set item", |b| {
        b.iter(|| from_set_item(black_box(elements.clone())))
    });
    group.bench_function("from tuple -py", |b| {
        Python::with_gil(|py| b.iter(|| from_tuple_py(py, black_box(elements.clone()))))
    });
    group.bench_function("from set item -py", |b| {
        Python::with_gil(|py| b.iter(|| from_set_item_py(py, black_box(elements.clone()))))
    });
    group.finish();
}

fn from_hashmap(hashmap: HashMap<String, PyObject>) {
    Python::with_gil(|py| {
        hashmap.into_py_dict(py);
    });
}

fn from_hashmap_py(py: Python<'_>, hashmap: HashMap<String, PyObject>) {
    hashmap.into_py_dict(py);
}

fn from_tuple(tuple: Elements) {
    Python::with_gil(|py| {
        PyDict::from_sequence(py, tuple.into_py(py)).unwrap();
    });
}
fn from_set_item(tuple: Elements) {
    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item(tuple.0 .0, tuple.0 .1).unwrap();
        dict.set_item(tuple.1 .0, tuple.1 .1).unwrap();
        dict.set_item(tuple.2 .0, tuple.2 .1).unwrap();
        dict.set_item(tuple.3 .0, tuple.3 .1).unwrap();
        dict.set_item(tuple.4 .0, tuple.4 .1).unwrap();
        dict.set_item(tuple.5 .0, tuple.5 .1).unwrap();
        dict.set_item(tuple.6 .0, tuple.6 .1).unwrap();
        dict.set_item(tuple.7 .0, tuple.7 .1).unwrap();
        dict.set_item(tuple.8 .0, tuple.8 .1).unwrap();
        dict.set_item(tuple.9 .0, tuple.9 .1).unwrap();
        dict.set_item(tuple.10 .0, tuple.10 .1).unwrap();
        dict.set_item(tuple.11 .0, tuple.11 .1).unwrap();
    })
}

fn from_tuple_py(py: Python<'_>, tuple: Elements) {
    PyDict::from_sequence(py, tuple.into_py(py)).unwrap();
}
fn from_tuple_py_object(py: Python<'_>, tuple: PyObject) {
    PyDict::from_sequence(py, tuple).unwrap();
}
fn from_set_item_py(py: Python<'_>, tuple: Elements) {
    let dict = PyDict::new(py);
    dict.set_item(tuple.0 .0, tuple.0 .1).unwrap();
    dict.set_item(tuple.1 .0, tuple.1 .1).unwrap();
    dict.set_item(tuple.2 .0, tuple.2 .1).unwrap();
    dict.set_item(tuple.3 .0, tuple.3 .1).unwrap();
    dict.set_item(tuple.4 .0, tuple.4 .1).unwrap();
    dict.set_item(tuple.5 .0, tuple.5 .1).unwrap();
    dict.set_item(tuple.6 .0, tuple.6 .1).unwrap();
    dict.set_item(tuple.7 .0, tuple.7 .1).unwrap();
    dict.set_item(tuple.8 .0, tuple.8 .1).unwrap();
    dict.set_item(tuple.9 .0, tuple.9 .1).unwrap();
    dict.set_item(tuple.10 .0, tuple.10 .1).unwrap();
    dict.set_item(tuple.11 .0, tuple.11 .1).unwrap();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
