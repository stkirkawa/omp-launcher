#!/bin/sh
set -eu

public_key_file=/tmp/kirkawa-release-panel/panel-tunnel.pub
authorized_keys=/root/.ssh/authorized_keys
test -s "$public_key_file"
install -d -o root -g root -m 0700 /root/.ssh

IFS=' ' read -r key_type key_data _ < "$public_key_file"
if [ "$key_type" != "ssh-ed25519" ] || [ -z "$key_data" ]; then
    echo "The panel tunnel public key is invalid." >&2
    exit 1
fi

if ! grep -Fq -- "$key_data" "$authorized_keys" 2>/dev/null; then
    options='command="/usr/bin/false",restrict,port-forwarding,permitopen="127.0.0.1:8787",permitlisten="127.0.0.1:1"'
    printf '%s %s %s Kirkawa-release-panel-tunnel\n' "$options" "$key_type" "$key_data" >> "$authorized_keys"
fi

chown root:root "$authorized_keys"
chmod 0600 "$authorized_keys"
rm -f "$public_key_file"
