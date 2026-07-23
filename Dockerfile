# syntax=docker/dockerfile:1.7

ARG NODE_VERSION=24
ARG BUN_VERSION=1.3.14
ARG DIOXUS_VERSION=0.7.9
ARG PI_VERSION=0.80.10

FROM docker.io/oven/bun:${BUN_VERSION} AS bun

FROM docker.io/library/node:${NODE_VERSION}-trixie AS development

ARG DIOXUS_VERSION

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup
ENV PATH="${CARGO_HOME}/bin:${PATH}"

COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    bash \
    build-essential \
    ca-certificates \
    curl \
    git \
    gnupg \
    openssh-client \
    pkg-config \
    ripgrep \
    tini

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable \
    && rustup target add wasm32-unknown-unknown \
    && cargo install --locked --version "${DIOXUS_VERSION}" dioxus-cli \
    && cargo install --locked just

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends clang

ARG PI_VERSION

RUN npm install --global --prefix /opt/syntaxis-pi \
    "@earendil-works/pi-coding-agent@${PI_VERSION}" \
    && npm cache clean --force \
    && for binary in cargo dx just rustc rustdoc rustfmt rustup; do \
    ln -s "/usr/local/cargo/bin/${binary}" "/usr/local/bin/${binary}"; \
    done

RUN usermod --login dev --home /home/dev --move-home node \
    && groupmod --new-name dev node \
    && mkdir -p /Projects /home/dev/.cargo/git /home/dev/.cargo/registry \
    && chown -R dev:dev \
    /Projects \
    /home/dev/.cargo \
    /usr/local/cargo \
    /usr/local/rustup

ENV HOME=/home/dev \
    SHELL=/bin/bash \
    CARGO_HOME=/home/dev/.cargo \
    NPM_CONFIG_PREFIX=/home/dev/.local
ENV PATH="${CARGO_HOME}/bin:/usr/local/cargo/bin:${PATH}"
ENV PATH="${HOME}/.local/bin:${PATH}"

USER dev
WORKDIR /Projects/syntaxis

EXPOSE 8080 5173

COPY --chmod=755 docker-entrypoint.sh /usr/local/bin/docker-entrypoint

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/docker-entrypoint"]
CMD ["just", "serve", "web", "0.0.0.0", "8080"]

FROM development AS build

USER root
WORKDIR /build

RUN mkdir /build-output && chown dev:dev /build /build-output

COPY --chown=dev:dev . .

USER dev

RUN --mount=type=cache,id=syntaxis-bun,target=/home/dev/.bun/install/cache,uid=1000,gid=1000 \
    bun install --frozen-lockfile \
    && bun run build:terminal \
    && bun run generate:completions \
    && bun run generate:pi-settings \
    && touch assets/tailwind.css

RUN --mount=type=cache,id=syntaxis-cargo-registry,target=/home/dev/.cargo/registry,uid=1000,gid=1000 \
    --mount=type=cache,id=syntaxis-cargo-git,target=/home/dev/.cargo/git,uid=1000,gid=1000 \
    --mount=type=cache,id=syntaxis-target,target=/build/target,uid=1000,gid=1000 \
    rm -rf target/dx/syntaxis/release/web/public \
    && dx build --platform web --release --locked --debug-symbols false \
    && cp -a target/dx/syntaxis/release/web/. /build-output/

FROM docker.io/library/node:${NODE_VERSION}-trixie-slim AS production

ARG PI_VERSION

ENV DEBIAN_FRONTEND=noninteractive

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    bash \
    ca-certificates \
    curl \
    git \
    gnupg \
    openssh-client \
    ripgrep \
    tini \
    && npm install --global --prefix /opt/syntaxis-pi \
    "@earendil-works/pi-coding-agent@${PI_VERSION}" \
    && npm cache clean --force

RUN usermod --login dev --home /home/dev --move-home node \
    && groupmod --new-name dev node \
    && mkdir -p /app /Projects \
    && chown dev:dev /app /Projects

COPY --from=build --chown=dev:dev /build-output /app
COPY --chmod=755 docker-entrypoint.sh /usr/local/bin/docker-entrypoint

ENV HOME=/home/dev \
    SHELL=/bin/bash \
    IP=0.0.0.0 \
    PORT=8080 \
    NPM_CONFIG_PREFIX=/home/dev/.local \
    SYNTAXIS_PROJECTS_ROOT=/Projects
ENV PATH="${HOME}/.local/bin:${PATH}"

USER dev
WORKDIR /Projects

EXPOSE 8080

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/docker-entrypoint"]
CMD ["/app/server"]
