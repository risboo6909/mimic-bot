FROM rustlang/rust:nightly-slim as builder

# update and install required packages
RUN apt-get update
RUN apt-get --yes install pkg-config libssl-dev

# simple project to build dependencies
RUN cargo new mimic-bot

# copy dependencies file from project
WORKDIR /mimic-bot
COPY Cargo.toml Cargo.lock ./

# build dependencies 
RUN cargo build --release

# replace simple project with actual sources
COPY src src
# force update file so cargo will rebuild it
RUN touch src/main.rs

# build project
RUN cargo build --release

# multistage container
# use debian slim image for runtime

FROM debian:buster-slim as runtime
RUN apt-get update \
    && apt-get install -y libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR app
COPY --from=builder /mimic-bot/target/release/mimic-bot .

# run project
CMD ["./mimic-bot"]
