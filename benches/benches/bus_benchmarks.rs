use agentos_bus::AgentBusTrait;
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};

// Benchmarks for the in-memory bus.
fn bench_in_memory_bus_publish(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("in_memory_bus_publish", |b| {
        b.iter_batched(
            agentos_bus::InMemoryBus::new,
            |bus| {
                let envelope = agentos_bus::AgentEnvelope::new(
                    black_box("bench-agent"),
                    black_box("broadcast"),
                    black_box("bench.topic"),
                    black_box(vec![0u8; 256]),
                );
                rt.block_on(bus.publish(envelope))
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_in_memory_bus_drain(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("in_memory_bus_drain", |b| {
        b.iter_batched(
            || {
                let bus = agentos_bus::InMemoryBus::new();
                let envelope = agentos_bus::AgentEnvelope::new(
                    "bench-agent",
                    "bench-agent",
                    "bench.topic",
                    vec![0u8; 256],
                );
                rt.block_on(bus.publish(envelope)).unwrap();
                bus
            },
            |bus| rt.block_on(bus.drain_for(black_box("bench-agent"))),
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    benches,
    bench_in_memory_bus_publish,
    bench_in_memory_bus_drain
);
criterion_main!(benches);
