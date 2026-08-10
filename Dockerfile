# sqlx 0.9 (locked in Cargo.lock) requires Rust 1.94 or newer.
FROM rust:1.94-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN useradd --system --create-home --uid 10001 app
COPY --from=build /app/target/release/project-co /usr/local/bin/project-co
USER app
ENV HTTP_ADDR=0.0.0.0:8080
EXPOSE 8080
CMD ["project-co"]
