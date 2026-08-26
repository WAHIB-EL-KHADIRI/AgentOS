# Pinned by digest; the tag is kept alongside so it stays readable and so
# Dependabot's docker ecosystem can bump both together.
FROM node:26-alpine@sha256:aadf416b2cdce311a8811ba3f0608a61b77dbf997500e2eafe781b51f6a0b019 AS builder

WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM nginxinc/nginx-unprivileged:alpine@sha256:f972e5322b9797dc2a6b830030094426437b1ae7032e4644496395336ac6fdac

# This base image runs as UID 101, which is the point of choosing it. `apk
# upgrade` needs root to write /var/log/apk.log and replace system packages,
# so as written the build failed outright:
#
#   ERROR: Unable to open log: Permission denied
#   process "/bin/sh -c apk upgrade --no-cache" ... exit code: 99
#
# Root only for the patching, then straight back. 101 is numeric rather than
# `nginx` deliberately: it is what the base image's own config declares as
# User, and a numeric id keeps the guarantee readable to anything checking
# that the final image is not root.
USER root
RUN apk upgrade --no-cache
USER 101

COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 5173
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget --no-verbose --tries=1 --spider http://localhost:5173/ || exit 1
CMD ["nginx", "-g", "daemon off;"]