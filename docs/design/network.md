# The network client

`network/` is the `razor-network` workspace crate used by the server, terminal,
phone and Mac apps. Its Rust contracts live in `network/src/lib.rs`; its C
contracts live in `network/include/razor_network.h`. The endpoint constructor,
HTTP client and C interface are implemented at this surface revision.
An HTTP request that may have run is never replayed.

## Pull calls across C

Pull reads and writes keep scheduling in Swift's worker queues and make
backpressure explicit. Callbacks avoid blocked caller threads but require Swift
context lifetimes, callback serialization and a completion rule during close.
Pull calls also support uploads and WebSocket upgrades without collecting the
whole HTTP message; Swift supplies the WebSocket framing after a 101 HTTP response.

## Dependencies and building

iroh supplies the encrypted connections described in [Networking on iroh](iroh.md#dependencies).
`hyper` supplies the existing HTTP/1.1 parser and driver; `http`, `bytes` and
`http-body-util` supply its already-resolved HTTP types, byte buffers and body
adapters. They enter the crate with the client implementation, together with
Tokio. No binding generator or binding crate is used.

`cargo build --workspace` builds the server and the rlib. `scripts/apple-network.sh [OUT]`
builds the static library for the phone, the Simulator, Mac Catalyst and the Mac
(`cargo rustc -p razor-network --release --target <Apple target> --crate-type staticlib`),
bundles the four with the C header as `RazorNetworkFFI.xcframework`, and assembles
the Swift package below at OUT. It needs Xcode's SDK and C toolchain for aws-lc-rs,
so only a Mac builds it.

The phone's build compiles only against libc 0.2.189: libc 0.2.190 dropped the iOS
items (`in6_ifreq`, `net/route.h`'s constants) that iroh 1.4's netwatch and netdev
use, and no iroh release yet avoids them. The workspace therefore pins
`libc = "=0.2.189"` in the root `Cargo.toml`, which every dependency accepts; the
pin goes when an iroh release builds for iOS against a current libc.

The iroh features remain `tls-aws-lc-rs`, `portmapper` and `platform-verifier`,
with defaults disabled. iroh also offers `tls-ring`, but a
crypto-provider change needs its own measurements. Cold extension startup,
dependency threads, peak memory and completion through a relay must be measured
on the device against its approximately 24 MB and 30-second limits.

## In a Swift app

An app adds the directory `scripts/apple-network.sh` assembled as a local Swift package and
imports `RazorNetwork`; the package carries the xcframework and links the
system frameworks the library needs, so the app's project adds nothing else.
Each process (the app and each extension) opens one `NetworkClient` with a key
of its own, off the main thread, and closes it before it suspends or before
another client opens with that key.

```swift
let client = try NetworkClient(key: secretKey, relays: relays)
// When the app observes a network change, look for new paths without blocking.
client.networkChanged()
// A whole exchange.
let (head, body) = try await client.send(machine, "GET", "/v1/status", idle: 30)
// A stream, line by line.
let (_, chunks) = try await client.open(machine, "GET", streamPath, idle: 60)
for try await line in NetworkLines(chunks) { handle(line) }
// Pairing: the code a server's `razor devices add` shows; enrolls these public keys.
guard let code = PairingCode(pasted) else { return }
let serverID = try await client.join(code, keys: publicKeys)
client.close()
```

A request timeout logs its phase, bytes received, outstanding requests, client age,
time since the last response head and whether a network change was reported, under
subsystem `com.treygoff.razor`, category `network`, in the system log. It then cancels
the request. The app supplies network-change observations with `networkChanged()`;
the package does not install a path monitor.
