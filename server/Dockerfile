FROM rust:1.81.0 as builder
WORKDIR /build
COPY . .
RUN rustup component add rustfmt clippy
RUN apt-get update && apt-get install -y python3-pip
RUN cargo build --release

FROM rust:1.81.0
WORKDIR /build
COPY --from=builder /build/target/release/livecamera .
CMD ["./livecamera"]
