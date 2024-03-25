# syntax=docker/dockerfile:1.4
FROM rust:bookworm AS builder

WORKDIR /code

COPY . /code

RUN --mount=type=cache,target=/code/target cd crates/forged; cargo prisma generate
RUN --mount=type=cache,target=/code/target cargo build --release -p forged --bin forged; cp /code/target/release/forged /code/forged

FROM debian:buster-slim

EXPOSE 3100

COPY --from=builder /code/forged /forged

CMD [ "/forged" ]
