FROM oven/bun:1-alpine AS base
WORKDIR /app

# Stage 1: Install dependencies into temporary directories to cache them
FROM base AS install
RUN mkdir -p /temp/dev /temp/prod

# Install development dependencies
COPY package.json bun.lock /temp/dev/
RUN --mount=type=cache,id=bun,target=/root/.bun/install/cache \
    cd /temp/dev && bun install --frozen-lockfile

# Install production dependencies (exclude devDependencies)
COPY package.json bun.lock /temp/prod/
RUN --mount=type=cache,id=bun,target=/root/.bun/install/cache \
    cd /temp/prod && bun install --frozen-lockfile --production

# Stage 2: Build stage
FROM base AS builder
COPY --from=install /temp/dev/node_modules ./node_modules
COPY tsconfig.json ./
COPY src/ ./src/
RUN bun run build

# Stage 3: Runner stage (Node.js runtime for running the compiled application)
FROM node:22-alpine AS runner
WORKDIR /app

COPY --from=install /temp/prod/node_modules ./node_modules
COPY --from=builder /app/dist ./dist
COPY package.json ./

ARG APP_VERSION=dev
ENV NODE_ENV=production
ENV PORT=47633
ENV DATABASE_PATH=/data/spool.db
ENV APP_VERSION=$APP_VERSION

RUN mkdir -p /data

EXPOSE 47633

CMD ["node", "dist/index.js"]

