FROM rustlang/rust:nightly-bookworm AS build

# Install build dependencies including Skia requirements
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    clang \
    libclang-dev \
    libfontconfig1-dev \
    libfreetype6-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

# Install runtime dependencies for Skia/fontconfig
RUN apt-get update && apt-get install -y \
    libfontconfig1 \
    libfreetype6 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /app/target/release/rosu-pp-service /

ENV PP_SERVICE_ADDR=[::]:50051 \
    RUST_LOG=info

EXPOSE 50051

ENTRYPOINT ["/rosu-pp-service"]
