FROM rust:slim as build

RUN apt-get update; \
    apt-get install --no-install-recommends -y libpq-dev libssl-dev pkg-config; \
    rm -rf /var/lib/apt/lists/*; \
    USER=root cargo new --bin app;

WORKDIR /app

# copy manifests
COPY ./Cargo.toml .
COPY ./rustfmt.toml .
COPY ./Makefile .
COPY .env .

RUN cargo install sqlx-cli
# build this project to cache dependencies
#ENV DATABASE_URL=postgres://postgres:postgres@172.0.0.1:5432/stacker
RUN sqlx database create && sqlx migrate run

RUN cargo build --release; \
    rm src/*.rs

# copy project source and necessary files
COPY ./src ./src

# add .env and secret.key for Docker env
#RUN touch .env;

# rebuild app with project source
RUN rm -rf ./target/release/deps/stacker*; \
    cargo build --release


# deploy stage
FROM debian:bullseye-slim as production

# create app directory
WORKDIR /app
RUN mkdir ./files && chmod 0777 ./files

# install libpq
#RUN apt-get update; \
#    apt-get install --no-install-recommends -y libpq-dev libssl-dev; \
#    rm -rf /var/lib/apt/lists/*

# copy binary and configuration files
#COPY --from=builder ~/.cargo/bin/sqlx-cli sqlx-cli
COPY --from=build /app/target/release/stacker .
COPY --from=build /app/.env .

EXPOSE 8080

# run the binary
ENTRYPOINT ["/app/stacker"]
