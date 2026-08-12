# Build stage
FROM rust:1-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY apps/terminal/ apps/terminal/
RUN cargo build --release --bin terminal-server

# Runtime stage — solo el binario, imagen chica
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends bash ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/terminal-server /usr/local/bin/terminal-server
EXPOSE 7681
CMD ["terminal-server"]
