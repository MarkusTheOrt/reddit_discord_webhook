FROM rust:alpine3.21 AS builder

WORKDIR /app

COPY . .

RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static gcc

RUN cargo build --release --all

FROM alpine:latest

WORKDIR /app

COPY --from=builder /app/target/release/reddit_discord_webhook /app/reddit_discord_webhook
COPY --from=builder /app/target/release/migrate /app/migrate

RUN mkdir /app/migrations

COPY --from=builder /app/migrations/* /app/migrations/

RUN chmod +x /app/reddit_discord_webhook

STOPSIGNAL SIGINT

CMD ["/app/reddit_discord_webhook"]

