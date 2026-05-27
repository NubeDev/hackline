# Deploying hackline to EC2 with Caddy TLS

End-to-end guide for deploying hackline-gateway on an EC2 instance
with Caddy for HTTPS termination and DigitalOcean DNS.

## Prerequisites

- EC2 instance (Ubuntu 24.04, x86_64, t3.small or larger)
- DigitalOcean DNS managing your domain (e.g. `nube-iiot.com`)
- DigitalOcean API token with read+write scope
- SSH key for the EC2 instance

## 1. DNS entries (DigitalOcean)

Add these A records pointing to the EC2 Elastic IP:

```
hackline.nube-iiot.com           →  A  →  <EC2-PUBLIC-IP>
*.hackline.nube-iiot.com         →  A  →  <EC2-PUBLIC-IP>
hackline.zenoh.nube-iiot.com     →  A  →  <EC2-PUBLIC-IP>
```

The first two are for Caddy (HTTP/HTTPS). The third is for Zenoh
(raw TLS on port 7447 — not proxied through Caddy).

## 2. EC2 security group

Open these inbound ports:

| Port | Source    | Purpose                         |
|------|----------|----------------------------------|
| 22   | Your IP  | SSH                              |
| 80   | 0.0.0.0/0| Caddy HTTP (ACME + redirect)     |
| 443  | 0.0.0.0/0| Caddy HTTPS                      |
| 7447 | 0.0.0.0/0| Zenoh (agents connect here)      |

## 3. Sync source code to EC2

From your Mac (excludes build artifacts and heavy folders):

```sh
rsync -avz --exclude 'target/' --exclude 'node_modules/' --exclude '.git/' \
  -e "ssh -i ~/Downloads/Brabeem-hackline.pem" \
  ~/Documents/codeduo/hackline/ ubuntu@<EC2-PUBLIC-IP>:~/hackline/
```

## 4. Install dependencies on EC2

```sh
ssh -i ~/Downloads/Brabeem-hackline.pem ubuntu@<EC2-PUBLIC-IP>
```

```sh
# System packages
sudo apt update && sudo apt install -y \
  build-essential pkg-config libssl-dev sqlite3 git golang-go tmux

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

# Caddy with DigitalOcean DNS plugin
~/go/bin/xcaddy build --with github.com/caddy-dns/digitalocean || {
  go install github.com/caddyserver/xcaddy/cmd/xcaddy@latest
  ~/go/bin/xcaddy build --with github.com/caddy-dns/digitalocean
}
sudo mv caddy /usr/local/bin/
```

## 5. Build hackline

```sh
cd ~/hackline
cargo build --release -p hackline-gateway -p hackline-agent
```

Binaries will be at:
- `~/hackline/target/release/serve` (gateway)
- `~/hackline/target/release/hackline-agent` (agent)

## 6. Create directories and config files

```sh
sudo mkdir -p /etc/hackline /var/lib/hackline
sudo chown ubuntu:ubuntu /var/lib/hackline
```

### Gateway config

```sh
sudo tee /etc/hackline/gateway.toml > /dev/null << 'EOF'
listen = "127.0.0.1:8080"
http_listen = "127.0.0.1:9000"
database = "/var/lib/hackline/gateway.db"

[zenoh]
mode = "router"
listen = ["tcp/0.0.0.0:7447"]

[log]
level = "info,hackline_core=info,hackline_gateway=info"
format = "json"
EOF
```

### Caddyfile

```sh
sudo tee /etc/hackline/Caddyfile > /dev/null << 'CADDYEOF'
hackline.nube-iiot.com {
    reverse_proxy localhost:8080
}

*.hackline.nube-iiot.com {
    tls {
        dns digitalocean {env.DO_API_TOKEN}
    }
    reverse_proxy localhost:9000 {
        header_up Host {host}
    }
}
CADDYEOF
```

## 7. Start the gateway

```sh
tmux new -s hackline
~/hackline/target/release/serve /etc/hackline/gateway.toml
# Copy the claim token from the output!
# Ctrl+B, D to detach
```

## 8. Start Caddy

```sh
tmux new -s caddy
sudo DO_API_TOKEN=<token_dadbb7f44129ae28> caddy run --config /etc/hackline/Caddyfile
# Wait for cert acquisition (30-60s for DNS challenge)
# Ctrl+B, D to detach
```

## 9. Verify HTTPS

From your local machine:

```sh
curl https://hackline.nube-iiot.com/v1/claim/status
# Expected: {"claimed":false,"can_claim":true}
```

## 10. Claim the gateway

```sh
curl -X POST https://hackline.nube-iiot.com/v1/claim \
  -H 'Content-Type: application/json' \
  -d '{"token": "<claim-token>", "label": "cloud-gateway"}'
# Save the bearer token from the response!
```

## 11. Register a device and create a tunnel

```sh
TOKEN="rDbBCwssyVCk3Kp_JCWwdaPAIhwizlhSWY7IT22762Y"

# Register device
curl -X POST https://hackline.nube-iiot.com/v1/devices \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"zid": "aa01", "label": "test-device"}'

# Create HTTP tunnel
curl -X POST https://hackline.nube-iiot.com/v1/tunnels \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"device_id": 1, "kind": "http", "local_port": 9998, "public_hostname": "test-device.hackline.nube-iiot.com"}'
```

## 12. Start agent on the "device" (your Mac or another EC2)

Update agent config to connect to the cloud gateway:

```toml
# examples/agent-cloud.toml
zid = "aa01"
allowed_ports = [9998]
label = "test-agent"

[zenoh]
mode = "client"
connect = ["tcp/3.106.242.141:7447"]

[log]
level = "info,hackline_core=debug"
format = "pretty"
```

Start a dummy HTTP server and the agent:

```sh
python3 -m http.server 9998 &
cargo run -p hackline-agent -- examples/agent.toml
```

## 13. Test the tunnel

```sh
curl https://test-device.hackline.nube-iiot.com/
# Should return the Python file server's directory listing
```

## Upgrading to Zenoh mTLS

This section walks through securing the Zenoh transport with mutual
TLS using certificates signed by the private CA server at
`hackline.ca.nube-iiot.com`.

### Prerequisites

- CA server running at `hackline.ca.nube-iiot.com`
- DNS record: `hackline.zenoh.nube-iiot.com` → EC2 public IP
- Both the gateway device and agent device registered in the CA
  server's database with their `global_uuid` and `preshared_secret`

### 14. Register devices in the CA database

SSH into the CA server and insert device records. Two devices are
needed: one for the gateway (Zenoh router) and one for the agent.

```sh
# SSH into the CA server host
# Enter the database container
docker exec -it <ca-db-container> psql -U <user> -d <db>

-- Insert gateway device
INSERT INTO zc_devices (global_uuid, preshared_secret)
VALUES ('hackline-gateway', '<gateway-preshared-secret>');

-- Insert agent device
INSERT INTO zc_devices (global_uuid, preshared_secret)
VALUES ('hackline-agent-01', '<agent-preshared-secret>');
```

### 15. Fetch the CA certificate

Both the gateway and agent need the CA root certificate to verify
each other's certs during mTLS.

```sh
# On the gateway server
sudo mkdir -p /etc/hackline/certs
sudo curl -o /etc/hackline/certs/ca.pem \
  https://hackline.ca.nube-iiot.com/ca/certificate
```

```sh
# On the agent device (your Mac)
mkdir -p ~/.hackline/certs
sudo curl -o ~/.hackline/certs/ca.pem \
  https://hackline.ca.nube-iiot.com/ca/certificate
```

### 16. Generate key pair and CSR on the gateway server

The gateway's Zenoh peer needs a certificate for the domain
`hackline.zenoh.nube-iiot.com`. In peer mode both sides present
certs (mTLS), so the gateway needs a listen certificate and the
CA root to validate incoming peer certs.

```sh
ssh -i ~/Downloads/Brabeem-hackline.pem ubuntu@<EC2-PUBLIC-IP>

# Generate private key
openssl genrsa -out /etc/hackline/certs/zenoh-server-key.pem 4096

# Create CSR for the Zenoh domain
openssl req -new \
  -key /etc/hackline/certs/zenoh-server-key.pem \
  -out /tmp/zenoh-server.csr \
  -subj "/CN=hackline.zenoh.nube-iiot.com"
```

### 17. Sign the gateway CSR with the CA

Send the CSR to the CA server with HMAC authentication. The HMAC is
computed over `global_uuid + timestamp + preshared_secret` using the
preshared secret as the HMAC key, output as base64.

```sh
GLOBAL_UUID="hackline-gateway"
PRESHARED_SECRET="<gateway-preshared-secret>"
TIMESTAMP=$(date +%s)
HMAC=$(echo -n "${GLOBAL_UUID}${TIMESTAMP}${PRESHARED_SECRET}" \
  | openssl dgst -sha256 -hmac "$PRESHARED_SECRET" -binary | base64)

# Build JSON payload with properly escaped CSR
jq -n --arg csr "$(cat /tmp/zenoh-server.csr)" '{csr: $csr}' > /tmp/csr_request.json

curl -X POST https://hackline.ca.nube-iiot.com/ca/sign \
  -H "Content-Type: application/json" \
  -H "X-GlobalUUID: $GLOBAL_UUID" \
  -H "X-Timestamp: $TIMESTAMP" \
  -H "X-HMAC: $HMAC" \
  -d @/tmp/csr_request.json \
  | jq -r '.certificate' | sudo tee /etc/hackline/certs/zenoh-server.pem > /dev/null

rm -f /tmp/csr_request.json
```

### 18. Generate key pair and CSR on the agent device

The agent (your Mac or edge device) needs a client certificate.
The CN is an identity label — no domain needed.

```sh
# On your Mac
openssl genrsa -out ~/.hackline/certs/device-01-key.pem 4096

openssl req -new \
  -key ~/.hackline/certs/device-01-key.pem \
  -out /tmp/device-01.csr \
  -subj "/CN=hackline-agent-01"
```

### 19. Sign the agent CSR with the CA

```sh
GLOBAL_UUID="hackline-agent-01"
PRESHARED_SECRET="<agent-preshared-secret>"
TIMESTAMP=$(date +%s)
HMAC=$(echo -n "${GLOBAL_UUID}${TIMESTAMP}${PRESHARED_SECRET}" \
  | openssl dgst -sha256 -hmac "$PRESHARED_SECRET" -binary | base64)

# Build JSON payload with properly escaped CSR
jq -n --arg csr "$(cat /tmp/device-01.csr)" '{csr: $csr}' > /tmp/csr_request.json

curl -X POST https://hackline.ca.nube-iiot.com/ca/sign \
  -H "Content-Type: application/json" \
  -H "X-GlobalUUID: $GLOBAL_UUID" \
  -H "X-Timestamp: $TIMESTAMP" \
  -H "X-HMAC: $HMAC" \
  -d @/tmp/csr_request.json \
  | jq -r '.certificate' > ~/.hackline/certs/device-01.pem

rm -f /tmp/csr_request.json
```

### 20. Update gateway config for mTLS (peer mode)

Replace the gateway Zenoh config to use peer mode with TLS.
Peer mode lets the gateway discover local peers via multicast
while also accepting TLS connections from remote agents.

```sh
sudo tee /etc/hackline/gateway.toml > /dev/null << 'EOF'
listen = "127.0.0.1:8080"
http_listen = "127.0.0.1:9000"
database = "/var/lib/hackline/gateway.db"

[zenoh]
mode = "peer"
listen = ["tls/0.0.0.0:7447"]

[zenoh.tls]
root_ca_certificate = "/etc/hackline/certs/ca.pem"
server_certificate = "/etc/hackline/certs/zenoh-server.pem"
server_private_key = "/etc/hackline/certs/zenoh-server-key.pem"
client_auth = true
verify_name_on_connect = false

[log]
level = "info,hackline_core=info,hackline_gateway=info"
format = "json"
EOF
```

Restart the gateway:

```sh
tmux attach -t hackline
# Ctrl+C to stop, then:
~/hackline/target/release/serve /etc/hackline/gateway.toml
```

### 21. Update agent config for mTLS (peer mode)

On your Mac, create the TLS agent config. Peer mode enables
local multicast discovery for LAN peers while also connecting
to the cloud gateway over TLS.

```toml
# examples/agent-cloud-tls.toml
zid = "aa01"
allowed_ports = [9998]
label = "test-agent"

[zenoh]
mode = "peer"
connect = ["tls/hackline.zenoh.nube-iiot.com:7447"]

[zenoh.tls]
root_ca_certificate = "/Users/brabeem/.hackline/certs/ca.pem"
client_certificate = "/Users/brabeem/.hackline/certs/device-01.pem"
client_private_key = "/Users/brabeem/.hackline/certs/device-01-key.pem"
verify_name_on_connect = false

[log]
level = "info,hackline_core=debug"
format = "pretty"
```

In peer mode, the agent presents its client certificate when
connecting to the gateway. LAN peers discovered via multicast
connect over plain TCP (no TLS on LAN).

Start the agent:

```sh
python3 -m http.server 9998 &
cargo run -p hackline-agent -- examples/agent-cloud-tls.toml
```

### 22. Verify mTLS tunnel

From anywhere on the internet:

```sh
curl https://test-device.hackline.nube-iiot.com/
# Should return the Python file server's directory listing
```

The Zenoh transport is now encrypted and mutually authenticated.
Only devices with certificates signed by the private CA can connect.
The agent behind NAT is reachable via the HTTPS tunnel hostname.

### Certificate file summary

| Location | File | Purpose |
|----------|------|---------|
| Gateway | `/etc/hackline/certs/ca.pem` | CA root cert (validates client certs) |
| Gateway | `/etc/hackline/certs/zenoh-server.pem` | Zenoh router server cert |
| Gateway | `/etc/hackline/certs/zenoh-server-key.pem` | Zenoh router private key |
| Agent | `~/.hackline/certs/ca.pem` | CA root cert (validates server cert) |
| Agent | `~/.hackline/certs/device-01.pem` | Agent client cert |
| Agent | `~/.hackline/certs/device-01-key.pem` | Agent private key |

See `examples/gateway-tls.toml` and `examples/agent-tls.toml` for
complete examples.

## Useful commands

```sh
# Re-attach to tmux sessions
tmux attach -t hackline
tmux attach -t caddy

# Check gateway logs
tmux attach -t hackline

# Wipe and restart
sudo rm /var/lib/hackline/gateway.db
# Restart gateway (new claim token will be printed)

# Re-sync code from Mac after changes
rsync -avz --exclude 'target/' --exclude 'node_modules/' --exclude '.git/' \
  -e "ssh -i ~/Downloads/Brabeem-hackline.pem" \
  ~/Documents/codeduo/hackline/ ubuntu@<EC2-PUBLIC-IP>:~/hackline/
# Then rebuild on EC2:
cd ~/hackline && cargo build --release -p hackline-gateway -p hackline-agent
```
