# Networking on iroh

Status: design approved with changes on 9 November 2026, after a cross-family review. Built and on main: the server endpoint, admission, pairing by code, paired servers and the remote terminal. To come: the phone and Mac apps, then the switch.

Razor connects its machines (paired servers, a remote terminal, the phone and Mac apps) by speaking plain HTTP with a password-like key on every HTTP request, and relies on Tailscale to encrypt that traffic and to keep strangers away from the port. This page replaces that with [iroh](https://iroh.computer), a Rust library that dials a device by its public key, encrypts every connection end to end, connects directly through home routers when it can, and falls back to a relay that cannot read the traffic when it cannot. Tailscale leaves Razor entirely.

## Why

Three problems, each of which Tailscale causes or cannot solve.

1. **The phone loses every server at once.** Tailscale on iPhone runs as a system VPN, and it has an open bug in which the tunnel freezes for every connection while the app still says Connected. A burst of packets toward a machine that cannot answer, such as a Mac asleep in a backpack, is enough to trigger it. On 7 November it took down the phone's connection to every machine more than once. Razor now avoids dialing a sleeping Mac, and the phone offers a button that restarts Tailscale, but those work around the bug; they do not remove it. An app that carries its own connections has no VPN to freeze.
2. **Razor cannot be used where Tailscale cannot be installed.** A company laptop usually forbids installing a VPN, and a company's security team will not accept unencrypted traffic between machines. Today Razor's traffic between machines is unencrypted unless Tailscale wraps it. A collaborator who uses Razor for his work on a company laptop raised exactly this: plain HTTP between servers is a non-starter, everything must be encrypted, and Razor must be able to run on one machine with no server-to-server traffic at all, or with connections only to places the company approves.
3. **Every new user needs Tailscale before Razor can connect anything.** Pairing two machines, using the phone app and running a remote terminal all require both ends to be on the same Tailscale network, or on two networks joined by Tailscale's device sharing. For an open-source user that is an account, an app on every device and a page of setup before the first connection.

Trey, 7 November 2026: "I definitely do want to create a long-running feature branch to completely refactor Razor's networking around that iroh library. That sounds fucking awesome." And on 9 November, after weighing the alternatives at the end of this page: "Iroh it is."

## Goals

In the order they matter:

1. **No VPN anywhere.** No Razor feature needs Tailscale or any other VPN on any device. The phone's family of VPN freezes cannot happen.
2. **Encrypted and authenticated by Razor itself.** Every byte between two machines is encrypted end to end, and each side proves who it is with its own key. No secret crosses the network in an HTTP request.
3. **Pairing in one step per side.** Pairing a machine or a phone is showing a code on one device and scanning or pasting it on the other. No account, no address, no port.
4. **Works on a locked-down company network,** and with relays named in the settings contacts no relay but those.
5. **At least as fast and reliable as today.** Direct connections where Tailscale had them; a relay near Trey where Tailscale relayed through Dallas; reconnection after sleep, network changes and cellular handoffs no worse than now. One exception: the lock screen's first wake after iOS has discarded the extension's process opens a fresh endpoint and connection, which no endpoint can do as fast as an already-open tunnel. It happens once per quiet stretch, and a push already arrives with seconds of jitter.
6. **Inside the budgets.** Idle memory, the local terminal's start and the line budget stay within SPEC section 6, or a raise is asked for with measurements.

### Desired outcomes, as they would be checked

- A new user installs Razor on two machines and the phone app and pairs all three with no other software, by scanning or pasting codes.
- A person's phone, Mac and Linux server stay connected through a day of sleep, travel and network changes with no restart of anything.
- A server on a network that blocks everything but outgoing web traffic on port 443, through the company's proxy and its TLS inspection, pairs with and reaches a server elsewhere through a relay. The check is the first install on a real company laptop.
- With relays named in the settings, a packet capture shows no relay but those, including when a pairing code or a pair names another.
- With the work-machine setting that contacts only the model provider, Razor opens no network endpoint at all.
- `docs/machines.md` has no Tailscale section, and the phone app has no exception for plain HTTP to `ts.net`.

## Not goals

- Changing what Razor's machines say to each other. Every HTTP request and event stream stays as it is; only the connection underneath changes.
- Building a relay. iroh ships one as a ready-made program (`iroh-relay`); Razor documents how to run it.
- A lookup service of our own for finding devices.
- iroh's other protocols (content-addressed files, broadcast, shared documents). HTTP already carries the files and streams that must survive this change.
- Connecting through more than one hop. Pairing stays direct, as today.

## Decisions

Trey, 9 November 2026, unless noted:

- **iroh**, over the alternatives at the end of this page.
- **Relays.** Left alone, Razor uses iroh's free public relays: "or just uses the IRO public relays kind of by default". A setting points a server at other relays instead, and then only those are used. Trey's machines use a relay on a rented server he provides ("Yes, I'll get a VPS for it"), and only that relay: iroh chooses among listed relays by measured latency, not by order, so listing the public relays as a backup could put his traffic on them while his relay is healthy. If his relay is down, direct connections still work.
- **The phone and Mac apps may include iroh,** an exception to the app's rule that no third-party code ships in the app (the app's SPEC, section 5), for iroh and what it needs only.
- **Recordings after Stop upload later, not in the background.** iOS's background uploads run in a system process that speaks only HTTP and HTTPS, so it cannot use iroh. A recording's pieces stay on the phone until the server acknowledges them, upload while the app runs (which it does while recording), and the rest finishes the next time the app gets time to run. Nothing is lost; it can arrive later. This replaces the background-upload promise in the app's SPEC 3.11.
- **Pairing consent is holding the code.** Whoever presents a fresh, unused code pairs, as linking a computer to WhatsApp works. The code is single-use and short-lived, and the server that showed it says which device joined, so a wrong one can be removed at once.
- **Existing pairs re-pair once,** with codes. No automatic migration. Pairing by code under the name of an entry made the old way, one that holds no network key, upgrades that entry where it is: its settings, notification token and follows stay, and its old key and address are dropped. Until then each use of such a server says it is not paired, and the release notes say how to pair it again. A name whose entry holds network keys is refused to any other key, as before.
- **Servers first, the apps at the switch** (Trey, 11 November 2026). Main has paired servers on iroh alone: a pair made the old way reaches nothing until it is paired again by code. The phone and Mac apps stay on the old path, the plain HTTP listener and the bearer key, until the switch (below).

## How iroh works, briefly

Each device has a key pair; its public key is its address. A device keeps one open connection to a relay it calls home, which is how others reach it from behind a router. When device A dials device B, A's first packets go through B's home relay, so the conversation starts at once; meanwhile the two exchange network addresses through the relay and try to open a direct path through their routers. iroh's documentation says that works in about nine of ten network conditions and keeps working while both networks stay the same; then the relay steps out. Otherwise traffic keeps flowing through the relay. Either way it is QUIC, encrypted end to end between the two devices' keys; the relay forwards bytes it cannot read.

A relay connection is an HTTPS request on port 443 upgraded to a WebSocket, so it passes firewalls that block UDP. A filter that allows HTTPS but blocks WebSocket upgrades stops it, and the relay's host must then be allowed as a WebSocket destination. A company that inspects TLS needs the relay connection to trust the system's certificate authorities, which iroh supports, and the company's proxy.

iroh's public relays are rate-limited with unpublished, adjustable limits, carry no uptime promise, are meant for development and testing, and officially support only the latest stable iroh, so using them obliges Razor to stay current. A relay sees which keys connect, from what addresses, when and how much; its operator can drop or delay traffic but not read it.

Sources: [relays](https://docs.iroh.computer/concepts/relays), [public relays](https://docs.iroh.computer/iroh-services/relays/public), [rate limiting](https://docs.iroh.computer/relays/rate-limiting), [configuring networks](https://docs.iroh.computer/configuring-networks).

## The design

### Identity and who may connect

- Each Razor home creates an iroh secret key once (`~/.razor/node.key`, mode 0600). Its public key is the server's identity on the network. `server.id` stays as it is and stays the server's id everywhere it is used today (the app's machine cache, pushes, the shared-settings owner); the network key does not replace it.
- **One process owns a key at a time.** iroh requires an identity to be unique among endpoints running at once; a second endpoint with the same key takes over its relay connection from the first. A home's key is therefore used only by that home's server.
- **The remote terminal becomes a setting on a paired server.** Under iroh a laptop runs its own server anyway, so `[remote] server` names which paired server new sessions, `--continue`, `--resume` and `--print` go to, with optional `[remote] folder` for new sessions and `--continue` (otherwise the launch directory's path), over the forwarding and remote-start routes paired servers already have (SPEC 3). The second client path (`Connection.remote`, its own key file, never starting a local server) is deleted. Any operation the remote terminal uses today that forwarding does not yet carry (files, drafts and voice are to be checked) is added to forwarding, not to a second path.
- **The phone has one enrollment and several keys.** The app and each of its extensions (notification service, notification content, widget, share) can run at once, so each has its own key, made together at pairing, kept in the shared Keychain group with `AfterFirstUnlockThisDeviceOnly` as the device key is today. The server records them under the phone's one device entry: one row, one set of notification settings and follows, and `razor devices remove phone` revokes every key at once. The Mac app does the same.
- `devices.json` records each device's public keys in place of the shared key a paired device carries today.
- **Admission.** iroh's endpoint hooks see each connection's key and protocol name after the handshake and before any data. A connection for Razor's API protocol (`razor/1`) from a key that is not enrolled is closed there, so an unknown device never reaches Razor's HTTP code. The only exception is the pairing protocol below, served separately and never given the API router.
- Each HTTP request served over iroh carries the connecting device's name, taken from the connection, in place of today's `Authorization: Bearer` check. The local terminal, through the Unix socket, keeps the local key exactly as now.
- **Revocation** closes every open connection of the removed device's keys, including one accepted while the removal was being saved, as revocation ends its streams today.

No secret crosses the network after pairing, and Razor's own security code is the enrollment list, the admission check and pairing. The cryptography is iroh's.

### What travels on the connection

Razor's API stays HTTP, with its newline-delimited event streams and WebSocket routes. Each HTTP request runs on its own QUIC stream within the one connection between two devices: the client opens a stream, writes one HTTP/1.1 request, and reads the reply; an event stream keeps its stream open, and a WebSocket upgrade keeps its stream. Opening a stream on an open connection costs no network round trip, and hyper's HTTP/1.1 handshake is local state, so an HTTP request on a warm connection costs an ordinary HTTP request and HTTP response. HTTP keep-alive is off (one HTTP request per stream), and cancelling an HTTP request cancels its stream.

Streams do not block each other's ordered delivery, but they share the connection's congestion control and flow control, and a relayed connection also shares the relay's rate limit. A large upload can still slow an event stream; that is measured, not assumed away.

HTTP/2 inside one stream would put every HTTP request back in one ordered byte stream. HTTP/3 fits in principle but brings a second HTTP stack and its integration with axum and upgrades. iroh's own RPC library would replace the API for no gain. Neither earns the change.

- **Server.** One iroh endpoint per server process, created at start unless networking is off (below). For each accepted stream on an admitted `razor/1` connection, hyper's HTTP/1.1 server runs the same axum router the Unix socket serves.
- **Rust client.** A hyper HTTP/1.1 client over a QUIC stream on the cached connection to that device, replacing reqwest for peer traffic. Its callers change where they hold reqwest types today (`Devices::request` and peer forwarding); a reqwest look-alike to avoid touching them would be code for its own sake. It keeps today's deadlines, body bounds, cancellation and streaming.
- **No replay of something that may have run.** Child operations and mail dial fresh, skipping the cache, with their existing connect deadlines (2 seconds for children, 5 for mail) wrapping iroh's own connect timeout; a failed handshake within that deadline means not sent, while after the handshake the HTTP request may have run and is never resent. Ordinary HTTP requests and streams keep the cached connection; cancelling a read leaves its shared dial running to its own five-second bound, so another read can reuse it. An admission rejection before any API stream is served also proves not sent, with its own close code; revocation after admission does not. A server closes its endpoint on shutdown so its peers' cached connections end at once.

### Pairing

1. On the server being joined, A: `razor devices add laptop` prints a pairing code as a QR code and as one line of text, and waits until a device joins or the code expires. The code holds a version tag, A's public key, A's relays (at most a few) and a random secret of at least 128 bits; nothing else. It expires in ten minutes. A home has one outstanding code; a new one replaces the last.
2. On the joining device, B: `razor devices join desk`, which asks for the code (pasted, so it stays out of shell history and process lists), or the phone scans it. B dials A by the key in the code on the pairing protocol (`razor-pair/1`); iroh's handshake is to that key or fails, so B makes no check of its own. B sends the secret, its keys' names and its own relays.
3. A, holding one lock: if the secret matches the outstanding code, A records B's key from the authenticated connection (not from anything B claims) with B's relays, saves the enrollment, marks the code used, and replies with its `server.id`. A wrong or expired secret is refused without spending a valid code.
4. B records A only when that reply arrives. If it is lost, B is not paired and says so; the person runs it again with a fresh code, and A's enrollment of B, being by key, is replaced, so nothing is left to repair.
5. The waiting `add` prints which device joined and exits; Ctrl-C or the expiry withdraws the code, and a second `add` replaces the code and ends the first with a line saying so. The terminal that showed the code is the only notice, and `razor devices` lists it, so a wrong one is removed at once. A's name on B is the name typed in step 2; the phone takes a server's label from its first status line, as now.

The text code is `razor1-` followed by unpadded base64url: one version byte (`1`), the 32-byte public key, the 16-byte random secret, then each relay as a two-byte big-endian UTF-8 length and its URL. There are at most eight relays (including all four of iroh's defaults), each at most 256 bytes. The code is at most 2,825 characters; one local relay makes it 105 characters. Base64url keeps it on one copyable line without JSON quoting or padding, and the same bytes fit a QR code. The terminal draws the QR in black on white so either terminal theme can be scanned.

With `relays` set, each side refuses the other's relays at this exchange if they are outside its list; that is the one place the rule is enforced.

A server joining by code sends its server id with its authenticated keys; the receiver validates and stores it, while a phone omits it, so only an enrollment with both keys and a server id has paired-server authority.

The pairing protocol accepts connections from unknown keys only while a code is outstanding, with a short deadline, a small byte limit and a limit on connections at once. The secret never appears in a URL, a log or an error. A screenshot, the clipboard or scrollback holding an unused code is a copy of the credential until the code expires or is used; that is the consent rule Trey accepted.

`razor devices find`, setup's Tailscale step, the port range 7433 to 7442, and today's `/v1/pair` with its sender-IP check are deleted.

### Finding a paired device later

There is no lookup service: Razor uses iroh's Minimal preset and installs no address lookup service, so it neither publishes nor looks up addresses through DNS, pkarr or local-network discovery. iroh 1.4 compiles its lookup code and dependencies unconditionally; these are included in the measurements below. Razor stores, for each pair, only the relays exchanged at pairing, and dials through them; iroh learns the pair's current direct addresses through the relay each time it connects, so addresses are never stored. An iroh peer address carries several relay URLs; the measurement below reaches a live relay when another in that address is down. A device that moves to a relay its pairs do not know is re-paired, the rule Trey accepted for the switch (Trey, 9 November, approving this cut).

Two devices on one network still connect directly, because they learn each other's local addresses through the relay.

### Settings

```toml
[network]
# The relays this server uses and advertises. Unset: iroh's public relays.
relays = ["https://relay.example.net"]
```

- **A new user** sets nothing and uses iroh's public relays.
- **Trey** lists his relay.
- **When `relays` is set, it is the only list of relays**, for this server and for dialing its pairs: pairing refuses a device whose relays are outside it (Pairing, above). What iroh itself learns about a peer's relay during a connection is iroh's behavior, seen at the first real company install, not policed by Razor. That is what keeps Trey's traffic off the public relays, and what a company needs.
- The relay connection's TLS always trusts the system's certificate authorities as well, so a company that inspects TLS works with no setting; on Android the relay connection trusts the Mozilla roots compiled into the library instead, since Android apps ignore user-installed authorities by default.
- **A proxy** comes from the environment's proxy variables: the endpoint enables iroh's `proxy_from_env()`, so no setting is needed.
- **Relay-only transport** (no direct UDP paths, no port mapping, no network probes beyond the relays) is the expected shape of a further setting, `relay_only`, written only if the first install on a real company laptop shows the direct-path attempts are what gets blocked or flagged.
- **The work-machine setting** (`provider_only`) opens no network endpoint at all.
- `razor doctor` reports the home relay connection, whether each pair is direct or relayed, and the round trip to each.
- A short page says how to run `iroh-relay` on a rented server, with its TLS certificate, and how to keep it current.

### The phone and Mac apps

- **One Rust library for every client.** The [network crate](network.md) supplies it; iroh's own Swift bindings do not expose what the relay rules and admission need (proxy, certificate authorities, hooks, transport controls), and Swift would also need its own HTTP client over QUIC streams. So the apps use a small Rust library from this repository, built for iOS and macOS: it holds the endpoint, pairing, and the hyper client above, and offers the apps one call that sends an HTTP request to a machine and returns its HTTP response or stream. The terminal's client and the apps' client are then one implementation. It lives in this repository and counts in Razor's line budget; the Swift that calls it counts in the app's; It is bound to Swift by a hand-written C interface of a few functions (open, request, close to begin with): no crate, nothing generated.
- The App Transport Security exceptions for `ts.net` are deleted; nothing in the app speaks plain HTTP over a network.
- Pairing the phone is scanning a server's code in the app.
- **The lock screen's programs.** The notification service extension has about 30 seconds and about 24 MB, libraries included. It opens a short-lived endpoint with its own key, fetches what the notification needs, and closes. Whether that fits, through a relay on cellular, with the real payload and the creature drawn, is the first thing measured. If it does not fit, the fallback is a push that carries the notification's words encrypted to the phone's key, which needs Trey's word because no agent's words travel through Apple today.
- **Recordings after Stop** upload when the app next runs, under the decision above.
- **The app's networking follows its life.** It closes its endpoint when iOS suspends it and reopens on return; a live call or a recording keeps its own background rights and is checked separately.
- **A Mac that sleeps.** With no VPN on the phone, an HTTP request to a sleeping Mac costs that HTTP request a timeout, not a frozen phone. So the phone's machinery for never dialing a sleeping machine, built only because dialing one froze Tailscale, is deleted: the shared list of asleep machines that every HTTP request checks, waiting for another machine to say it is back, the rare randomized retries, and the Restart Tailscale button. The server's "going to sleep" line stays, so lists show a sleeping Mac at once instead of as stale (Trey, 9 November, approving this cut).
- **Reconnecting that stops.** Trey, 9 November: the phone "should not just be like permanently forever trying to reconnect to a sleeping server". After a machine drops or fails, the phone makes a short burst of retries with exponential backoff and jitter (about 1, 2, 4, 8, 16 and 30 seconds, roughly a minute), for a flaky network; then it shows the machine offline and stops trying on its own. It tries once more when the app comes to the front, the network changes, the person taps that machine, a push arrives from it, or another connected machine's status lists it answering again. A machine whose own server line said it is going to sleep skips the burst and shows asleep at once. In the background iOS suspends the app, so nothing retries there.
- Apple's push service is unchanged.

### What is deleted

- The plain HTTP listener on `[server] address`, and the bearer-key check for anything but the Unix socket.
- `razor devices find`, the tailnet scan, the port range and setup's Tailscale step.
- The remote terminal's separate client path and its key file.
- `/v1/pair` as it is today, with its sender-IP check and held keys.
- The `ts.net` exceptions in the phone app's five Info.plist files, and the SPEC sentences about Tailscale in both repositories.
- The phone's background upload of recordings after Stop.
- The phone's asleep list, its HTTP request gate and rare retries, and the "Restart Tailscale" button with its detection of a frozen tunnel.
- Tailscale in `docs/machines.md`, `docs/security.md` and the quickstart.
- A paired server's old address and key in `devices.json`, with the listener they were for. The upgrade of an old entry by pairing under its name goes in the release after the switch, when no entry without a network key can exist.

## Order of work

On one long-running branch per repository. The measurements that can stop the project come first, and their numbers go on this page. Before anything merges to main, one size request with base and candidate commits and measured counts per row; nothing on the branch needs a size grant until then.

1. **The server's idle memory,** with only the endpoint opened (before admission, pairing or revocation exist) and then with two connected pairs, under SPEC's memory workload, against the 24 MiB limit; CPU and wakeups after a network loss; buffers bounded under a slow reader. Binary size and release build time.

   **Measured, 9 November 2026,** on a Linux server with a local development relay: the server's idle memory rose from 9.5 to 10.4–10.9 MiB with the endpoint open and 10.5 MiB with two peers connected, against 24 MiB; the other four SPEC section 6 budgets were unchanged. Idle CPU was 0.05% of one core with the relay up and 0.12% in the minute after it went away. The release binary grew by 16.7 MiB, to 75 MiB, the clean release build from 165 to 208 seconds, and Cargo.lock by 132 crates. iroh accepts an `http://` relay address, and a peer address holds several relays as a set: a dial reached the live one with another in the address down.

2. **The lock screen.** The Rust library's endpoint, hyper client and first C interface, in a real notification service extension with its key enrolled by hand, on Trey's phone: cold start through a relay on cellular, the real payload and creature, peak memory, success rate and timing, with the app and another extension running at once. The Mac app's build too.

   **Measured, 10 November 2026,** on Trey's Duo with real pushes through Apple's development service, iroh's public relays, and the app in the foreground or the phone locked (no difference): 24 of 24 notifications filled. On home Wi-Fi the extension had the content 0.45–0.51 s after waking, on cellular 0.83–1.23 s; peak memory 6.7 and 8.5 MB against about 24 MB. Most of that is the first request on a fresh connection (0.34–0.40 s on Wi-Fi, 0.61–0.97 s on cellular); a second request on the same connection took 13–25 ms and 104–247 ms. Today's HTTP over Tailscale filled in 0.15 s on Wi-Fi and 0.35 s on cellular, one wake each. Closing the endpoint after delivery took 1–2 s, outside the wait. iOS kept one extension process alive across every wake for 24 minutes, so the cold start is measured once (Wi-Fi: 68 ms to wake, filled at 0.51 s).
3. **No separate company-network test.** The company case is designed from iroh's documented behavior: on a network that blocks UDP, direct attempts fail and traffic stays on the relay, which is ordinary HTTPS on port 443. The live test is the collaborator installing the release on his company laptop; `relay_only` is written only if that shows the direct attempts are blocked or flagged (Trey, 9 November: design it well from first principles so that the live test is the real install, with no simulated company network built beforehand).
4. **Server to server:** endpoint, admission, pairing by code, HTTP over streams, settings, `razor doctor`; proven between a Linux server and a Mac at home, on the phone's hotspot and through Trey's relay.
5. **The remote terminal** as a setting naming a paired server, with any missing forwarding added.
6. **The phone and Mac apps:** pairing by scanning, HTTP requests and streams, the lock screen's programs, recordings that finish later.
7. **The switch: one release that switches and deletes.** It deletes everything listed above and ships Razor, TestFlight and the Mac app together. Its notes say to re-pair every device by code; an old app meeting a new server shows the signed-out state the app's SPEC 3.1 already has.

The project stops, or this page changes, if the notification service extension cannot fetch reliably within its limits and the encrypted-push fallback is refused; if idle memory cannot stay within budget without a raise Trey will not give; or if the phone on cellular to a server is measurably worse than through Tailscale today. The acceptable difference, set on 10 November 2026 after the first phone measurement (so after its numbers were seen): a warm wake of the notification service extension has the content ready within 0.35 s on cellular and 0.15 s on Wi-Fi on the Duo, at least as fast as Tailscale, measured before the switch release deletes Tailscale; a cold wake, the first after iOS discards the process, within 1.5 s on cellular and 0.75 s on Wi-Fi; and a wake on a connection left stale while iOS suspended the process costs no more than a cold wake. If the warm numbers land just above today's, today's are measured again with ten wakes on each network before concluding.

The local terminal's 15 ms start (launch to first frame with a server already running) is unaffected by design and still measured; the first HTTP response from a remote machine is measured separately.

## Dependencies (RULES 20)

- **`iroh`** (Rust, MIT or Apache-2.0; 1.4.1 current), with default features disabled and only `tls-aws-lc-rs`, `portmapper` and `platform-verifier` enabled: QUIC, hole punching, relay fallback and key-based connections. Writing our own would mean owning QUIC, NAT traversal and a relay protocol; iroh is the reason this is possible at all. The Minimal preset installs no address lookup service; iroh 1.4 compiles its lookup code unconditionally. Its binary size, build time and crates, including those lookup dependencies, are measured in step 1.
- **`hyper`** declared directly for its HTTP/1.1 client and server connections, already compiled for axum and reqwest, so little new code; its features are reviewed.
- **`qrcode`**, with default features disabled, encodes pairing codes as terminal QR symbols; it replaces a handwritten QR encoder, whose error correction and symbol layout would add security-sensitive code for no benefit.
- **No crate for the Swift binding:** a hand-written C interface of a few functions. Memory ownership across it and how a cancelled stream ends are in the review's security scope.
- **In the apps,** the Rust library above with iroh inside it, under Trey's exception.

## Risks

- **Razor owns its networking.** A bug in admission, pairing or revocation would be a hole into every user's machine. The cryptography stays iroh's; Razor's part is small, and the review of the Rust work by another model family centres on it.
- **iroh is young** next to Tailscale (1.0 in June 2026; a relay security fix in July). The public relays oblige us to follow its releases.
- **Company networks** that block WebSocket upgrades block iroh until their IT allows the relay: a sentence in the docs and a clear `razor doctor` line.
- **The lock screen** may not fit; step 2 finds out before the security-critical code is built.

## Alternatives considered

- **TLS with pinned keys on today's TCP listener.** Encrypts and authenticates, like SSH, with little code; does nothing to get through home routers, so Tailscale or an open port is still needed. iroh does everything this does.
- **Another VPN** (WireGuard, ZeroTier, NetBird, a self-hosted Tailscale control server). A system VPN on every device: forbidden on company laptops, and the phone keeps a VPN that can freeze.
- **Tailscale built into the app** (libtailscale, with Tailscale's Swift package TailscaleKit). Removes the phone's system VPN and keeps Tailscale's mature networking. The runner-up. Against it: a Go library linked into a Rust server and a Swift app; every user still needs a Tailscale account; a company still sees and must allow Tailscale; and the freeze may live in networking code it shares with the VPN.
- **A hub every machine dials out to.** The friendliest to company firewalls; all traffic passes a server we must run forever, and two machines on one desk talk through it.
- **libp2p or WebRTC.** Both traverse routers; both are larger and more general than Razor needs, and neither fits a Rust server and a Swift app as directly.
