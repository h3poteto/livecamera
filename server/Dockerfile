FROM rust:1.77.1 as builder
WORKDIR /build
COPY . .
RUN rustup component add rustfmt clippy
RUN apt-get update && apt-get install -y python3-pip
RUN cargo build --release

FROM rust:1.77.1
WORKDIR /build
COPY --from=builder /build/target/release/livecamera .
CMD ["./livecamera"]
