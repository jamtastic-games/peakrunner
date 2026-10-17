FROM caddy@sha256:22e1d921a7dd98ea722ebd6819de785fd71abdab7f7fed8a2378e96d29bb923a
# Port 8080 needs no privileged bind capability; retain cap_drop: ALL at runtime.
RUN setcap -r /usr/bin/caddy
COPY site/Caddyfile /etc/caddy/Caddyfile
COPY site/public /srv
COPY assets/package-docs/THIRD-PARTY-NOTICES.txt /srv/legal/third-party-notices.txt
USER 10001:10001
EXPOSE 8080
