FROM rust:alpine3.21 AS builder

WORKDIR /app

COPY . .

RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static gcc

RUN cargo build --release

FROM alpine:3.21

WORKDIR /app

COPY --from=builder /app/target/release/reddit_discord_webhook /app/reddit_discord_webhook

RUN chmod +x /app/reddit_discord_webhook

STOPSIGNAL SIGINT

CMD /app/reddit_discord_webhook

