FROM node:22-alpine

WORKDIR /app

COPY package*.json ./
RUN npm ci --production=false && npm run build && npm prune --production

FROM node:22-alpine

WORKDIR /app

COPY --from=0 /app/node_modules ./node_modules
COPY --from=0 /app/dist ./dist

ENV NODE_ENV=production
ENV PORT=3000

EXPOSE 3000

CMD ["node", "dist/index.js"]
