FROM rust:alpine as builder

WORKDIR /app/src
RUN USER=root

RUN apk add pkgconfig openssl-dev libc-dev
COPY ./ ./
RUN cargo build -p ws-server --release 

FROM alpine:latest
WORKDIR /app

COPY --from=builder /app/src/target/release/ws-server /app/ws-server 

CMD ["/app/ws-server "]