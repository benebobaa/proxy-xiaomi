# 1. Planning Stage
FROM rust:1-slim AS planner
WORKDIR /app
RUN cargo install cargo-chef --locked
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# 2. Caching Stage
FROM rust:1-slim AS cacher
WORKDIR /app
RUN cargo install cargo-chef --locked
COPY --from=planner /app/recipe.json recipe.json
# Install build dependencies for compiling libsql-ffi (SQLite C code)
RUN apt-get update && apt-get install -y build-essential && rm -rf /var/lib/apt/lists/*
RUN cargo chef cook --release --recipe-path recipe.json

# 3. Builder Stage
FROM rust:1-slim AS builder
WORKDIR /app
COPY . .
# Install build dependencies again for compile stage
RUN apt-get update && apt-get install -y build-essential && rm -rf /var/lib/apt/lists/*
# Copy chef target output to avoid rebuilding dependencies
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
RUN cargo build --release

# 4. Runtime Stage
FROM gcr.io/distroless/cc-debian12 AS runtime
WORKDIR /app

# Copy release binary and configuration file
COPY --from=builder /app/target/release/xiaomi-proxy /app/xiaomi-proxy
COPY --from=builder /app/config.toml /app/config.toml

# Expose port
EXPOSE 8080

# Set configuration environment variable
ENV XIAOMI_PROXY_CONFIG=/app/config.toml

# Execute binary
ENTRYPOINT ["/app/xiaomi-proxy"]
