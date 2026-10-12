# syntax=docker/dockerfile:1

FROM node:22-alpine3.22 AS web-build
WORKDIR /app/web
RUN npm install --global pnpm@9.15.9 && pnpm config set store-dir /pnpm/store
COPY web/package.json web/pnpm-lock.yaml ./
RUN --mount=type=cache,target=/pnpm/store pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm run build

FROM rust:1.90-alpine3.22 AS server-build
ARG TARGETARCH
RUN apk add --no-cache musl-dev
WORKDIR /app/server
COPY server/Cargo.toml server/Cargo.lock ./
COPY server/src/ ./src/
COPY --from=web-build /app/web/dist /app/web/dist
ENV CARGO_PROFILE_RELEASE_OPT_LEVEL=z \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_PROFILE_RELEASE_PANIC=abort
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,id=fileflow-target-${TARGETARCH},target=/app/server/target \
    cargo build --release --locked && mkdir /out && cp target/release/FileFlow /out/FileFlow

FROM alpine:3.22 AS runtime
RUN addgroup -S fileflow && adduser -S -G fileflow -H fileflow
COPY --from=server-build /out/FileFlow /usr/local/bin/fileflow
USER fileflow:fileflow
WORKDIR /app
ENV FILEFLOW_HOST=0.0.0.0 FILEFLOW_PORT=5000 RUST_LOG=warn
EXPOSE 5000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget -q -O /dev/null "http://127.0.0.1:${FILEFLOW_PORT}/api/fileflow/hello" || exit 1
ENTRYPOINT ["/usr/local/bin/fileflow"]
