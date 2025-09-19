FROM rust:1.90.0 as builder
WORKDIR /build
COPY . .
RUN rustup component add rustfmt clippy
RUN cargo build --release

FROM rust:1.90.0
WORKDIR /build
COPY --from=builder /build/target/release/livecamera .
CMD ["./livecamera"]
