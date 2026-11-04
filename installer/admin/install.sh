#!/bin/sh
set -eu

if ! command -v 7z >/dev/null 2>&1; then
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y p7zip-full
fi
test -s /etc/kirkawa-updates/manifest-signing.key
mkdir -p /opt/kirkawa-release-panel /srv/kirkawa/public/.staging
chmod 0700 /srv/kirkawa/public/.staging
install -o root -g root -m 0644 /tmp/kirkawa-release-panel/admin_server.py /opt/kirkawa-release-panel/admin_server.py
install -o root -g root -m 0644 /tmp/kirkawa-release-panel/index.html /opt/kirkawa-release-panel/index.html
install -o root -g root -m 0644 /tmp/kirkawa-release-panel/admin.css /opt/kirkawa-release-panel/admin.css
install -o root -g root -m 0644 /tmp/kirkawa-release-panel/admin.js /opt/kirkawa-release-panel/admin.js
install -o root -g root -m 0644 /tmp/kirkawa-release-panel/kirkawa-release-panel.service /etc/systemd/system/kirkawa-release-panel.service
systemctl daemon-reload
systemctl enable --now kirkawa-release-panel.service
systemctl restart kirkawa-release-panel.service
systemctl is-active --quiet kirkawa-release-panel.service
attempt=0
while [ "$attempt" -lt 15 ]; do
    if ss -lnt | grep -Fq '127.0.0.1:8787'; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 1
done
if ! ss -lnt | grep -Fq '127.0.0.1:8787'; then
    echo "Release panel did not start listening on 127.0.0.1:8787." >&2
    exit 1
fi
bash /tmp/kirkawa-release-panel/provision-tunnel.sh
rm -rf /tmp/kirkawa-release-panel
