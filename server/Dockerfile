FROM node:22-alpine AS builder

WORKDIR /app

COPY package*.json ./
RUN npm ci --production=false

COPY tsconfig.json ./
COPY src/ ./src/
RUN npm run build && npm prune --production

FROM node:22-alpine

WORKDIR /app

COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/dist ./dist

ARG APP_VERSION=dev
ENV NODE_ENV=production
ENV PORT=47633
ENV DATABASE_PATH=/data/ludusavi.db
ENV APP_VERSION=$APP_VERSION

RUN mkdir -p /data

EXPOSE 47633

CMD ["node", "dist/index.js"]
