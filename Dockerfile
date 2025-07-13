# --- Build Stage ---
FROM messense/rust-musl-cross:x86_64-musl as builder

WORKDIR /app
COPY . .

RUN cargo build --release

# --- Runtime Stage ---
FROM alpine:latest
RUN apk --no-cache add ca-certificates

WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/rust_rapi_chat .

ENV RUST_LOG=info
EXPOSE 8080

CMD ["./rust_rapi_chat"]
