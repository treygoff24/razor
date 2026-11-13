# Running your own relay

Razor's devices connect to each other directly when they can. When a network blocks that, traffic goes through a relay, a server that forwards encrypted packets it cannot read. Without any setting, Razor uses the public relays run by iroh's developers. To use your own, run `iroh-relay` on a small Linux server with a public address and list it in each Razor's config.

## The server

Any Linux server with a public IPv4 address, ports 80 and 443 (TCP) and 7842 (UDP) open, and about 1 GB of memory is enough. A relay carries only traffic that cannot go direct, so the smallest plan from most hosts has plenty of transfer.

Build `iroh-relay` at the version of iroh that Razor uses. Razor's `Cargo.lock` names it as the `iroh-relay` package:

```sh
cargo install iroh-relay --version 1.4.1 --locked --features server
```

The relay gets its own TLS certificate from Let's Encrypt, so it needs a DNS name pointing at the server. Many hosts give each server one already (Akamai's look like `203-0-113-7.ip.linodeusercontent.com`); a name on your own domain works too. Save as `/etc/iroh-relay.toml`:

```toml
http_bind_addr = "[::]:80"
enable_quic_addr_discovery = true
metrics_bind_addr = "127.0.0.1:9090"

[tls]
cert_mode = "LetsEncrypt"
hostname = "relay.example.com"
contact = "you@example.com"
cert_dir = "/var/lib/iroh-relay/certs"
```

As written, anyone who knows the address can relay through it. To keep strangers off, give each person who uses the relay a password of their own, list every password, and restart the relay whenever the list changes:

```toml
[access]
shared_token = ["<one person's password>", "<another person's>"]
```

`openssl rand -hex 16` makes a good password. To shut one person out, remove theirs and restart the relay; nobody else is touched. A restart drops every connection for a moment, and devices reconnect by themselves.

Run it as a system service under its own user, with `AmbientCapabilities=CAP_NET_BIND_SERVICE` so it can bind ports 80 and 443, `WorkingDirectory` and a writable `/var/lib/iroh-relay`, and `ExecStart=/usr/local/bin/iroh-relay --config-path /etc/iroh-relay.toml`. It requests the certificate on first start and renews it itself. `curl https://relay.example.com/` answers with a page titled "Iroh Relay" once it is up.

## Razor's setting

In the `config.toml` of each Razor a person runs, the relay's address with that person's password:

```toml
[network]
relays = ["https://relay.example.com/?token=<their password>"]
```

With `relays` set, Razor uses only the relays listed and refuses any other, including one a paired device suggests. Pairing compares addresses whole, password included, so a server that pairs with someone else's lists both people's addresses.

Set `relays` on every machine before pairing anything. A pairing code carries the relays of the machine that made it, so a phone or server paired before the change keeps the old address. It may keep reaching that machine, over a direct connection already open or while the old relay still admits it, but it can stop once that relay refuses it or cannot be reached, since connecting again needs a relay both can reach; pair it again under the same name to give it the new one.

The password travels in every pairing code to each device and server the person pairs with. It keeps strangers off the relay's bandwidth; it is not a secret from the people you pair with. Razor leaves it out of what it prints: `razor doctor` and pairing's refusals show the relay's address without it.
