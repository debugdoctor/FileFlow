#!/bin/sh
set -eu
lan_ip=${1:?Usage: sh deploy/create-lan-cert.sh IP_ADDRESS}
case "$lan_ip" in *[!0-9.]*|'') printf '%s\n' 'Expected an IPv4 address' >&2; exit 1 ;; esac
tls_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/tls
test ! -e "$tls_dir" || { printf '%s\n' 'TLS directory already exists; preserving certificates.' >&2; exit 1; }
umask 077
mkdir "$tls_dir"
openssl req -x509 -newkey rsa:3072 -nodes -sha256 -days 3650 \
  -keyout "$tls_dir/ca.key" -out "$tls_dir/ca.crt" -subj '/CN=FileFlow LAN Root CA' \
  -addext 'basicConstraints=critical,CA:TRUE' -addext 'keyUsage=critical,keyCertSign,cRLSign'
openssl req -newkey rsa:2048 -nodes -sha256 -keyout "$tls_dir/server.key" \
  -out "$tls_dir/server.csr" -subj "/CN=$lan_ip"
cat > "$tls_dir/server.ext" <<EOF
subjectAltName=IP:$lan_ip,IP:127.0.0.1,DNS:localhost
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
EOF
openssl x509 -req -sha256 -days 365 -in "$tls_dir/server.csr" \
  -CA "$tls_dir/ca.crt" -CAkey "$tls_dir/ca.key" -CAcreateserial \
  -extfile "$tls_dir/server.ext" -out "$tls_dir/server.crt"
chmod 755 "$tls_dir"
chmod 644 "$tls_dir/ca.crt" "$tls_dir/server.crt"
# Only nginx (UID 101) and the host owner/root can read the server key.
# Run this command through Docker because the deploy user may not have sudo.
printf '%s\n' 'Certificates created. Assign server.key to nginx UID 101 with mode 0400 before starting HTTPS.'
openssl x509 -in "$tls_dir/ca.crt" -noout -fingerprint -sha256
