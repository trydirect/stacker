FROM rust:bookworm as builder

RUN apt-get update; \
    #apt-get install --no-install-recommends -y libpq-dev libssl-dev pkg-config; \
    apt-get install --no-install-recommends -y libssl-dev; \
    rm -rf /var/lib/apt/lists/*; \
    USER=root cargo new --bin app;

RUN cargo install sqlx-cli

WORKDIR /app
# copy manifests
COPY ../Cargo.toml .
COPY ../Cargo.lock .
COPY ../rustfmt.toml .
COPY ../Makefile .
COPY ../docker/local/.env .
COPY ../docker/local/configuration.yaml .

# build this project to cache dependencies
#RUN sqlx database create && sqlx migrate run

RUN cargo build --release; \
    rm src/*.rs

# add .env and secret.key for Docker env
#RUN touch .env;
# copy project source and necessary files
COPY ../src ./src

#RUN sqlx migrate run
#RUN cargo sqlx prepare -- --bin stacker


# rebuild app with project source
RUN rm -rf ./target/release/deps/stacker*; \
    cargo build --release

# deploy stage
FROM debian:bookworm as production

# create app directory
WORKDIR /app
RUN mkdir ./files && chmod 0777 ./files

# install libpq
RUN apt-get update; \
    apt-get install --no-install-recommends -y  libssl-dev  \
    && rm -rf /var/lib/apt/lists/*

# copy binary and configuration files
#COPY --from=builder ~/.cargo/bin/sqlx-cli sqlx-cli
COPY --from=builder /app/target/release/stacker .
COPY --from=builder /app/.env .
COPY --from=builder /app/configuration.yaml .
COPY --from=builder /usr/local/cargo/bin/sqlx sqlx

EXPOSE 8000

# run the binary
ENTRYPOINT ["/app/stacker"]
