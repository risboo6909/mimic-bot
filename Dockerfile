FROM rustlang/rust:nightly-slim

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

# run project
CMD ["target/release/mimic-bot"]
