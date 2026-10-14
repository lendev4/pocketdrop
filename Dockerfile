FROM rust:1.99-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:trixie-slim
WORKDIR /app
COPY --from=build /src/target/release/pocketdrop /usr/local/bin/pocketdrop
COPY public ./public
ENV HOST=0.0.0.0 DATA_DIR=/data
CMD ["pocketdrop"]
