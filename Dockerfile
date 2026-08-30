# Stage 1: Build
# Base images are pinned by digest, not tag: a tag is mutable and can be
# repointed at a different image. Dependabot's docker ecosystem keeps these
# digests current, so pinning does not mean sitting on unpatched CVEs.
FROM rust:1-slim-bookworm@sha256:2775a09d208ff0d7c1f50490c45b62db929e87ba1dcbc3f2132ac71a704bcdd3 AS builder

WORKDIR /app
COPY . .

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
RUN cargo build --workspace --release
RUN cp target/release/agentOS /agentOS
RUN mkdir -p /templates && cp -r templates/* /templates/

# Stage 2: Supervisor monitor image
FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171 AS supervisor

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 agentos \
    && useradd --system --uid 10001 --gid agentos --home-dir /app --shell /usr/sbin/nologin agentos \
    && mkdir -p /app /data \
    && chown -R agentos:agentos /app /data

COPY --from=builder /agentOS /usr/local/bin/agentOS

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
  CMD ["agentOS", "supervisor", "--health"] || exit 1
USER agentos:agentos
WORKDIR /app
EXPOSE 9876
ENV RUST_LOG=info \
    AGENTOS_DATA_DIR=/data
CMD ["agentOS", "supervisor"]

# Stage 3: Full runtime
FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171 AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 agentos \
    && useradd --system --uid 10001 --gid agentos --home-dir /app --shell /usr/sbin/nologin agentos \
    && mkdir -p /app /data \
    && chown -R agentos:agentos /app /data

COPY --from=builder /agentOS /usr/local/bin/agentOS
COPY --from=builder /templates /templates

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
  CMD ["agentOS", "health"] || exit 1
USER agentos:agentos
WORKDIR /app
ENV RUST_LOG=info \
    AGENTOS_DATA_DIR=/data
ENTRYPOINT ["agentOS"]
CMD ["--help"]