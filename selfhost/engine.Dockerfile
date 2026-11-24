FROM rust:1-slim-trixie AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p aroh-server

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 aroh
COPY --from=build /src/target/release/aroh-server /usr/local/bin/aroh-server
USER aroh
ENV AROH_BIND=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["aroh-server"]
CMD ["worker"]
