# Create a container with statically linked binary (container size ~ 2 MB)

FROM ekidd/rust-musl-builder:stable AS builder

COPY . .
RUN sudo chown -R rust:rust .
RUN cargo build --release

FROM scratch

COPY --from=builder /home/rust/src/target/x86_64-unknown-linux-musl/release/dep-sanitizer /app
WORKDIR /project
ENTRYPOINT ["/app"]
