# Stage 1: build the static site (tsc + asset copy -> dist/)
FROM node:16-alpine AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

# Stage 2: minimal runtime that serves dist/ on 3456
FROM node:16-alpine
WORKDIR /app
RUN npm install -g serve
COPY --from=builder /app/dist ./dist
EXPOSE 3456
CMD ["serve", "-s", "dist", "-l", "3456"]
