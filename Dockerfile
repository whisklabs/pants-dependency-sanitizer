# Create a container with statically linked binary (container size ~ 3.7 MB)

FROM ekidd/rust-musl-builder:stable AS builder

COPY . .
RUN sudo chown -R rust:rust .
RUN cargo build --release

FROM scratch

COPY --from=builder /home/rust/src/target/x86_64-unknown-linux-musl/release/pants-cleaner /app
WORKDIR /project
ENTRYPOINT ["/app"]
