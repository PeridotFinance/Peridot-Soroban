FROM node:22-alpine

WORKDIR /app
COPY package.json ./
RUN npm install --omit=dev

COPY src ./src

USER node
CMD ["node", "src/main.mjs"]
