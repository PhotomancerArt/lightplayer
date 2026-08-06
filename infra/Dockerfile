# The lightplayer.app service image: one binary, one sidecar, one web bundle.
#
# BUILD CONTEXT IS THE REPO ROOT, not infra/. Two things in the image come
# from outside this directory: the Rust workspace (built here) and the Studio
# web artifact (built BEFORE this Dockerfile runs, see below). Build it as:
#
#   just studio-web-deploy-dir production target/pages/studio lightplayer.app
#   cp infra/dockerignore .dockerignore
#   docker build -f infra/Dockerfile -t lightplayer .
#
# CI does exactly that (.github/workflows/deploy-cloud.yml) and then hands the
# context to fly's remote builder.
#
# Why the web artifact is NOT built in this Dockerfile: `dx` needs the wasm
# toolchain, wasm-bindgen-cli, node, and a warm cargo cache — a ten-minute
# cold build with no layer reuse between deploys. CI already owns those
# caches for the Pages workflow, so the artifact arrives as bytes and the
# image build stays about the server.

# ---------------------------------------------------------------------------
# Stage 1 — the server binary.
# ---------------------------------------------------------------------------
# `rust:1-bookworm` for the C toolchain and rustup; the actual compiler is
# whatever rust-toolchain.toml pins (a nightly — this is a build-std
# workspace), which rustup installs on first cargo invocation. Bookworm is
# also the runtime base below, so the binary's glibc matches by construction.
FROM rust:1-bookworm AS build

# pkg-config + libssl-dev: `openssl-sys` is in the workspace lockfile. The
# cloud crates' TLS goes through rustls/ring, so this is belt-and-braces for
# a transitive dependency rather than a known requirement — cheap insurance
# against a build that fails ten minutes in on the fly builder.
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_PERMIT_COPY_RENAME=1
ENV CARGO_TERM_COLOR=always

WORKDIR /build

# The whole workspace, because cargo needs every member's manifest to resolve
# the lockfile even when only one package is built. No manifest-only
# pre-copy layer: `COPY . .` is invalidated by every commit anyway, and the
# 60-crate manifest dance would need maintenance on every new crate.
# infra/dockerignore keeps `target/` (minus the web artifact) and `.git` out.
COPY . .

# --locked: the deploy compiles what CI tested, or it fails loudly.
RUN cargo build --release --locked -p lp-cloud-server

# ---------------------------------------------------------------------------
# Stage 2 — the litestream binary.
# ---------------------------------------------------------------------------
# Litestream streams the SQLite WAL to Tigris and restores it on boot. It is
# a statically linked Go binary, so it is fetched once here and copied into
# the runtime stage rather than installed there.
#
# No sha256 pin: the checksum could not be verified offline when this file was
# written, and a fabricated one is worse than none. Add it here when someone
# next touches this stage:
#   curl -fsSL .../litestream-v${V}-linux-amd64.tar.gz | sha256sum
FROM debian:bookworm-slim AS litestream

ARG LITESTREAM_VERSION=0.3.13
ARG LITESTREAM_ARCH=amd64

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL -o /tmp/litestream.tar.gz \
        "https://github.com/benbjohnson/litestream/releases/download/v${LITESTREAM_VERSION}/litestream-v${LITESTREAM_VERSION}-linux-${LITESTREAM_ARCH}.tar.gz" \
    && tar -C /usr/local/bin -xzf /tmp/litestream.tar.gz litestream \
    && rm /tmp/litestream.tar.gz \
    && /usr/local/bin/litestream version

# ---------------------------------------------------------------------------
# Stage 3 — the runtime.
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

# ca-certificates: the process talks TLS to Google (OAuth) and Tigris (blobs
# + WAL replication), and a slim image ships no trust store. libssl3 pairs
# with the libssl-dev note in stage 1.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=litestream /usr/local/bin/litestream /usr/local/bin/litestream
COPY --from=build /build/target/release/lp-cloud-server /usr/local/bin/lp-cloud-server

COPY infra/litestream.yml /etc/litestream.yml
COPY infra/entrypoint.sh /usr/local/bin/entrypoint.sh

# The Studio bundle, staged by CI at target/pages/studio. `LP_CLOUD_STATIC_DIR`
# below points the server at it; without it the server serves a placeholder
# page and the deploy would look "fine" while shipping no app.
COPY target/pages/studio /app/web

# Container-shape defaults only. Everything deployment-shaped — base URL,
# bucket, endpoint — lives in infra/fly.toml's [env] so it can be read and
# changed without rebuilding the image. Secrets are never here.
# The git sha this image was built from — CI passes it so /healthz can
# answer "what version is deployed" (and the cutover smoke can assert
# deployed == pushed). Defaults to "dev" for hand builds.
ARG GIT_SHA=dev
ENV LP_CLOUD_BUILD_SHA=${GIT_SHA}
ENV LP_CLOUD_BIND=0.0.0.0 \
    LP_CLOUD_PORT=2812 \
    LP_CLOUD_STORE=sqlite \
    LP_CLOUD_DATA_DIR=/data \
    LP_CLOUD_STATIC_DIR=/app/web \
    RUST_LOG=info

EXPOSE 2812

# Not a plain `CMD lp-cloud-server`: litestream must restore before the server
# opens the database and must stay PID 1's child so it can checkpoint on
# shutdown. See infra/entrypoint.sh.
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
