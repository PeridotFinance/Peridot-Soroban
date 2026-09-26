FROM node:22-alpine

WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev

COPY src ./src
RUN mkdir -p /app/data && chown -R node:node /app

USER node
ENV STATE_FILE=/app/data/state.json
CMD ["node", "src/main.mjs"]
