# syntax=docker/dockerfile:1.7
#
# Multi-stage build for mdweb.
#
# Builder stage needs the embedded `site/` directory at compile time
# (build.rs reads it and emits `cargo:rerun-if-changed=site`); it must NOT
# appear in .dockerignore.

# ---------- builder ----------
FROM rust:1.83-slim-bookworm AS builder

WORKDIR /build

# Manifests + build script first so the dependency layer caches.
COPY Cargo.toml Cargo.lock build.rs ./

# build.rs requires `site/` at compile time. Without it the build emits a
# warning and `mdweb create` will produce empty scaffolding.
COPY site ./site
COPY src ./src

RUN cargo build --release \
    && strip target/release/mdweb

# ---------- runtime ----------
FROM debian:bookworm-slim

# ca-certificates: future-proofs HTTPS outbound (no current use, but cheap).
# curl:            required by docker-compose healthcheck.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 1000 --home /app --shell /sbin/nologin mdweb

WORKDIR /app

COPY --from=builder /build/target/release/mdweb /usr/local/bin/mdweb

# Bundle the embedded default site so `mdweb create` and the default theme
# work without a mounted directory. The compose `volumes:` mount hides
# this copy when the user provides their own site/.
COPY --from=builder /build/site /app/site

RUN chown -R mdweb:mdweb /app

USER mdweb

ENV MDWEB_HOST=0.0.0.0 \
    MDWEB_PORT=8080

EXPOSE 8080

# Run the bundled site by default; a volume-mounted site/ at runtime will
# shadow /app/site (the container serves from its cwd, which is /app/site).
WORKDIR /app/site

ENTRYPOINT ["/usr/local/bin/mdweb"]
CMD ["run", "."]