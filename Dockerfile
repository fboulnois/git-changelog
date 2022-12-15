FROM rust:1-alpine AS env-build

WORKDIR /app
COPY . /app

RUN cargo build --release
