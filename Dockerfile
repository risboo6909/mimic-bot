FROM rustlang/rust:nightly-slim

WORKDIR /mimic-bot
COPY . .

RUN apt-get update
RUN apt-get --yes install pkg-config libssl-dev

RUN cargo install --path .

CMD ["mimic-bot"]
