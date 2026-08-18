# Build stage
FROM rust:1-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
# El manifiesto de la raíz declara los dos sidecars (terminal-server y
# complexity-engine) en un solo Cargo.toml — cargo necesita que ambos paths
# existan para poder resolverlo, aunque acá solo se compile uno.
COPY services/terminal/ services/terminal/
COPY services/complexity/ services/complexity/
RUN cargo build --release --bin terminal-server

# Runtime stage — solo el binario, imagen chica
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends bash ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/terminal-server /usr/local/bin/terminal-server
EXPOSE 7681
CMD ["terminal-server"]
