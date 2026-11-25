FROM node:22-alpine

WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev

COPY src ./src
COPY config ./config
RUN mkdir -p /app/data && chown -R node:node /app

USER node
ENV STATE_FILE=/app/data/state.json JOURNAL_FILE=/app/data/journal.jsonl
CMD ["node", "src/main.mjs"]
