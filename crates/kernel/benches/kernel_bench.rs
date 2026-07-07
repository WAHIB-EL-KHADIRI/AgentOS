use criterion::{black_box, criterion_group, criterion_main, Criterion};

use agentos_kernel::circuit_breaker::CircuitBreaker;
use agentos_kernel::system::AgentOSSystem;

fn bench_system_creation(c: &mut Criterion) {
    c.bench_function("system::new", |b| {
        b.iter(|| {
            let _sys = AgentOSSystem::new();
            black_box(())
        })
    });
}

fn bench_circuit_breaker_creation(c: &mut Criterion) {
    c.bench_function("circuit_breaker::new", |b| {
        b.iter(|| {
            let cb = CircuitBreaker::new("bench");
            black_box(cb)
        })
    });
}

criterion_group!(
    benches,
    bench_system_creation,
    bench_circuit_breaker_creation,
);
criterion_main!(benches);
