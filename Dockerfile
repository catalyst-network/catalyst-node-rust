FROM rust:1.80 as build
WORKDIR /app
COPY . .
RUN cargo build --release --workspace

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /app/target/release/catalyst-node /app/target/release/catalyst-node
ENTRYPOINT ["/app/target/release/catalyst-node"]
