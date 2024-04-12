FROM rust:bookworm as builder

#RUN apt-get update; \
#    apt-get install --no-install-recommends -y libssl-dev; \
#    rm -rf /var/lib/apt/lists/*; \
#    USER=root cargo new --bin app;

RUN cargo install sqlx-cli

WORKDIR /app
# copy manifests
COPY ./Cargo.toml .
COPY ./Cargo.lock .
COPY ./rustfmt.toml .
COPY ./Makefile .
COPY ./docker/local/.env .
COPY ./docker/local/configuration.yaml .

# build this project to cache dependencies
#RUN sqlx database create && sqlx migrate run

# build skeleton and remove src after
#RUN cargo build --release; \
#    rm src/*.rs


COPY ./src ./src

# for ls output use BUILDKIT_PROGRESS=plain docker build .
#RUN ls -la /app/ >&2
#RUN sqlx migrate run
#RUN cargo sqlx prepare -- --bin stacker

RUN apt-get update && apt-get install --no-install-recommends -y libssl-dev; \
    cargo build --bin=console && cargo build --release

#RUN ls -la /app/target/release/ >&2

# deploy production
FROM debian:bookworm-slim as production

RUN apt-get update && apt-get install --no-install-recommends -y libssl-dev ca-certificates;
# create app directory
WORKDIR /app
RUN mkdir ./files && chmod 0777 ./files

# copy binary and configuration files
COPY --from=builder /app/target/release/server .
COPY --from=builder /app/target/release/console .
COPY --from=builder /app/.env .
COPY --from=builder /app/configuration.yaml .
COPY --from=builder /usr/local/cargo/bin/sqlx sqlx
COPY ./access_control.conf.dist /app

EXPOSE 8000

# run the binary
ENTRYPOINT ["/app/server"]
