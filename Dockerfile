FROM rust:1.70.0 AS builder

WORKDIR /app
RUN apt-get update && apt-get install lld clang -y
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim AS runtime

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    && apt-get autoremove -y \
    && apt-get clean -y  \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/demo demo
ENTRYPOINT ["./demo"]
