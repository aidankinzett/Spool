FROM mcr.microsoft.com/devcontainers/base:noble

RUN apt-get update && apt-get install -y \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    unzip \
    && rm -rf /var/lib/apt/lists/*
