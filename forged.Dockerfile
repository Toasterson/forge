# syntax=docker/dockerfile:1.4
FROM rust:bookworm AS base

WORKDIR /code

FROM base AS development

EXPOSE 3100

CMD [ "cargo", "run", "--offline" ]

FROM base AS dev-envs

EXPOSE 3100
RUN <<EOF
apt-get update
apt-get install -y --no-install-recommends git
EOF

RUN <<EOF
useradd -s /bin/bash -m vscode
groupadd docker
usermod -aG docker vscode
EOF

# install Docker tools (cli, buildx, compose)
COPY --from=gloursdocker/docker / /
COPY . /code
CMD [ "cargo", "run" ]

FROM base AS builder

RUN --mount=type=cache,target=/code/target cargo build --release

FROM debian:buster-slim

EXPOSE 3100

COPY --from=builder /code/target/release/forged /forged

CMD [ "/forged" ]
