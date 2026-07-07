use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_vault_put_get(c: &mut Criterion) {
    let mut vault = agentos_vault::Vault::new();
    let agent_id = "bench-agent";
    let key = "bench-key";
    let value = "secret-value-123";

    c.bench_function("vault_put", |b| {
        b.iter(|| vault.put(black_box(agent_id), black_box(key), black_box(value)))
    });

    vault.put(agent_id, key, value);
    c.bench_function("vault_get", |b| {
        b.iter(|| {
            vault
                .get(black_box(agent_id), black_box(key))
                .map(|secret| black_box(secret.expose().len()))
        })
    });
}

fn bench_vault_encrypt_decrypt(c: &mut Criterion) {
    use agentos_vault::encryption::VaultEncryption;

    let enc = VaultEncryption::new();
    let data = vec![0u8; 1024];

    c.bench_function("vault_encrypt_1kb", |b| {
        b.iter(|| enc.encrypt(black_box(&data)))
    });

    let encrypted = enc.encrypt(&data).unwrap();
    c.bench_function("vault_decrypt_1kb", |b| {
        b.iter(|| enc.decrypt(black_box(&encrypted)))
    });
}

criterion_group!(benches, bench_vault_put_get, bench_vault_encrypt_decrypt);
criterion_main!(benches);
