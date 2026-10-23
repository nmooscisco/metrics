#[macro_use]
extern crate criterion;

use criterion::Criterion;
use std::sync::Arc;

use metrics::{
    counter, Counter, Gauge, Histogram, Key, KeyName, Label, LabelSet, Metadata, Recorder,
    SharedString, Unit,
};

#[derive(Debug)]
struct TestRecorder;

impl Recorder for TestRecorder {
    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn register_counter(&self, _: &Key, _: &Metadata<'_>) -> Counter {
        Counter::noop()
    }
    fn register_gauge(&self, _: &Key, _: &Metadata<'_>) -> Gauge {
        Gauge::noop()
    }
    fn register_histogram(&self, _: &Key, _: &Metadata<'_>) -> Histogram {
        Histogram::noop()
    }
}

fn macro_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("macros");
    group.bench_function("uninitialized/no_labels", |b| {
        b.iter(|| {
            counter!("counter_bench").increment(42);
        })
    });
    group.bench_function("uninitialized/with_static_labels", |b| {
        b.iter(|| {
            counter!("counter_bench", "request" => "http", "svc" => "admin").increment(42);
        })
    });
    group.bench_function("global_initialized/no_labels", |b| {
        let _ = metrics::set_global_recorder(TestRecorder);
        b.iter(|| {
            counter!("counter_bench").increment(42);
        });
    });
    group.bench_function("global_initialized/with_static_labels", |b| {
        let _ = metrics::set_global_recorder(TestRecorder);
        b.iter(|| {
            counter!("counter_bench", "request" => "http", "svc" => "admin").increment(42);
        });
    });
    group.bench_function("global_initialized/with_dynamic_labels", |b| {
        let _ = metrics::set_global_recorder(TestRecorder);

        let label_val = rand::random::<u64>().to_string();
        b.iter(move || {
            counter!("counter_bench", "request" => "http", "uid" => label_val.clone())
                .increment(42);
        });
    });
    group.bench_function("local_initialized/no_labels", |b| {
        metrics::with_local_recorder(&TestRecorder, || {
            b.iter(|| {
                counter!("counter_bench").increment(42);
            });
        });
    });
    group.bench_function("local_initialized/with_static_labels", |b| {
        metrics::with_local_recorder(&TestRecorder, || {
            b.iter(|| {
                counter!("counter_bench", "request" => "http", "svc" => "admin").increment(42);
            });
        });
    });
    group.bench_function("local_initialized/with_dynamic_labels", |b| {
        metrics::with_local_recorder(&TestRecorder, || {
            let label_val = rand::random::<u64>().to_string();
            b.iter(move || {
                counter!("counter_bench", "request" => "http", "uid" => label_val.clone())
                    .increment(42);
            });
        });
    });
    group.finish();
}

fn shared_labels_benchmark(c: &mut Criterion) {
    // Compare seven keys with sixteen labels each. Share the strings in both cases to isolate
    // the cost of copying and retaining the label arrays.
    let label_names = [
        "label_0", "label_1", "label_2", "label_3", "label_4", "label_5", "label_6", "label_7",
        "label_8", "label_9", "label_10", "label_11", "label_12", "label_13", "label_14",
        "label_15",
    ];
    let labels: Vec<Label> = label_names
        .iter()
        .enumerate()
        .map(|(i, name)| Label::new(*name, Arc::<str>::from(i.to_string())))
        .collect();
    let shared: Arc<[Label]> = labels.clone().into();
    let names =
        ["metric_0", "metric_1", "metric_2", "metric_3", "metric_4", "metric_5", "metric_6"];
    let mut group = c.benchmark_group("shared_labels/retain_seven_keys");

    group.bench_function("owned", |b| {
        b.iter(|| {
            names
                .iter()
                .map(|name| Key::from_parts(*name, labels.clone()).to_retained())
                .collect::<Vec<_>>()
        });
    });
    group.bench_function("shared", |b| {
        b.iter(|| {
            names
                .iter()
                .map(|name| Key::from_parts(*name, Arc::clone(&shared)).to_retained())
                .collect::<Vec<_>>()
        });
    });
    group.finish();
}

fn shared_prefix_benchmark(c: &mut Criterion) {
    use std::hint::black_box;

    let parent = LabelSet::from(
        (0..16)
            .map(|i| Label::new(format!("label_{}", i), Arc::<str>::from(i.to_string())))
            .collect::<Vec<_>>(),
    );
    let extra = vec![Label::new("label_c", Arc::<str>::from("value_c"))];
    let names =
        ["metric_0", "metric_1", "metric_2", "metric_3", "metric_4", "metric_5", "metric_6"];
    let mut group = c.benchmark_group("shared_prefix/extend_and_retain_seven_keys");
    group.bench_function("flat", |b| {
        b.iter(|| {
            let labels: Arc<[Label]> =
                black_box(&parent).iter().chain(black_box(&extra).iter()).cloned().collect();
            names
                .iter()
                .map(|name| Key::from_parts(*name, Arc::clone(&labels)).to_retained())
                .collect::<Vec<_>>()
        });
    });
    group.bench_function("prefix", |b| {
        b.iter(|| {
            let labels = black_box(&parent).with_extra_labels(black_box(extra.clone()));
            names
                .iter()
                .map(|name| Key::from_parts(*name, &labels).to_retained())
                .collect::<Vec<_>>()
        });
    });
    group.finish();
}

fn shared_sets_benchmark(c: &mut Criterion) {
    use std::hint::black_box;

    let sets: Vec<_> = (0..2)
        .map(|set| {
            LabelSet::from(
                (0..16)
                    .map(|index| {
                        Label::new(
                            format!("label_{}_{}", set, index),
                            Arc::<str>::from(index.to_string()),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let mut group = c.benchmark_group("shared_sets/combine_and_retain_seven_keys");
    for shared in [false, true] {
        group.bench_function(if shared { "shared" } else { "copied_suffix" }, |b| {
            b.iter(|| {
                let first = black_box(&sets[0]);
                let second = black_box(&sets[1]);
                let labels = if shared {
                    first.with_extra_labels(second)
                } else {
                    first.with_extra_labels(second.iter().cloned().collect::<Vec<_>>())
                };
                ["metric_0", "metric_1", "metric_2", "metric_3", "metric_4", "metric_5", "metric_6"]
                    .iter()
                    .map(|name| Key::from_parts(*name, &labels).to_retained())
                    .collect::<Vec<_>>()
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    macro_benchmark,
    shared_labels_benchmark,
    shared_prefix_benchmark,
    shared_sets_benchmark
);
criterion_main!(benches);
