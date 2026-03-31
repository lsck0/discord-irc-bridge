FROM rust:1-bookworm AS base
RUN cargo install cargo-chef

FROM base AS planner
WORKDIR /app
COPY .. .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --tests --features integration-test --recipe-path recipe.json

FROM base AS runtime
WORKDIR /app
COPY .. .
COPY --from=builder /app/target target

CMD ["cargo", "test", "--workspace", "--", "--test-threads=1"]
