use agentos_memory::embedder::HashingEmbedder;
use agentos_memory::Embedder;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_embedding(c: &mut Criterion) {
    let embedder = HashingEmbedder::new(128);

    c.bench_function("embed_text_short", |b| {
        b.iter(|| embedder.embed(black_box("Hello, world!")))
    });

    c.bench_function("embed_text_long", |b| {
        b.iter(|| embedder.embed(black_box(
            "The quick brown fox jumps over the lazy dog. This is a longer text to test embedding performance with more tokens and characters."
        )))
    });
}

criterion_group!(benches, bench_embedding);
criterion_main!(benches);
