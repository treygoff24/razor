import Foundation
import os
import RazorNetworkFFI

// Razor's network client for a Swift app: HTTP requests and streams to a paired machine, addressed
// by its key, over the C interface in razor_network.h (docs/design/network.md). An app keeps one
// `NetworkClient` per process, each process with a key of its own, and pairs with `join`.

/// A machine on the network: its 32-byte public key and the relays it was paired through.
public struct NetworkPeer: Codable, Hashable, Sendable {
    public var key: Data
    public var relays: [String]
}

public struct NetworkError: Error, Sendable {
    public enum Kind: Sendable {
        /// Nothing was sent: this key is not enrolled on that machine.
        case refused
        /// No byte moved for the request's idle time.
        case timedOut
        case other
    }
    public var kind: Kind
    public var message: String
    init(_ kind: Kind, _ message: String) { self.kind = kind; self.message = message }

    /// Takes and frees a library error.
    init(_ raw: UnsafeMutablePointer<RazorNetworkError>?) {
        guard let raw else { self.init(.other, "The network operation failed."); return }
        defer { razor_network_error_free(raw) }
        self.init(raw.pointee.code == RAZOR_NETWORK_REFUSED ? .refused : .other, String(cString: raw.pointee.message))
    }
}

public struct NetworkResponse: Sendable {
    public var status: Int
    /// By lowercased name; a repeated header keeps its last value.
    public var headers: [String: String]
}

public enum NetworkBody: Sendable {
    case none
    case data(Data)
    /// Read from disk as it is sent, never held whole.
    case file(URL)
}

/// One endpoint with one identity. Opening blocks while it binds and closing blocks while the
/// endpoint closes (a second or two), so do both off the main thread. A key is used by one live
/// client at a time, across the app and its extensions.
public final class NetworkClient: @unchecked Sendable {
    private let handle: OpaquePointer
    /// Held around starting a request and around marking the client closed, as the C interface
    /// forbids racing the two.
    private let lock = NSLock()
    private var closed = false
    private var outstanding = 0, changed = false
    private let openedAt: TimeInterval
    private var lastHead: TimeInterval?

    /// `key` is the 32-byte secret key; `relays` are the only relays this client uses.
    public init(key: Data, relays: [String]) throws(NetworkError) {
        guard key.count == 32 else { throw NetworkError(.other, "A network key is 32 bytes.") }
        var error: UnsafeMutablePointer<RazorNetworkError>?
        let opened = lend(relays) { relays in
            key.withUnsafeBytes { key in
                relays.withUnsafeBufferPointer { razor_network_open(key.bindMemory(to: UInt8.self).baseAddress, $0.baseAddress, $0.count, &error) }
            }
        }
        guard let opened else { throw NetworkError(error) }
        handle = opened
        openedAt = ProcessInfo.processInfo.systemUptime
    }

    deinit { close() }

    /// Looks for new network paths without blocking; call when the app observes a network change.
    public func networkChanged() {
        lock.withLock {
            if !closed { changed = true; razor_network_network_change(handle) }
        }
    }

    /// Cancels every request, closes the endpoint and returns when it is closed; later requests
    /// fail. Requests in flight keep this client alive, so close it explicitly before the app
    /// suspends or another client opens with the same key.
    public func close() {
        let first = lock.withLock {
            defer { closed = true }
            return !closed
        }
        if first { razor_network_close(handle) }
    }

    /// Starts a request and returns before anything is dialed; the blocking calls on the result do
    /// the work. `fresh` dials a connection of this request's own within that many seconds, for a
    /// request that must not go down a connection that may be dead; without it the machine's open
    /// connection is shared. A WebSocket's upgrade uses this directly.
    public func request(_ peer: NetworkPeer, _ method: String, _ path: String, headers: [(String, String)] = [],
                        length: Int = 0, pairing: Bool = false, fresh: TimeInterval? = nil) throws(NetworkError) -> NetworkRequest {
        guard peer.key.count == 32 else { throw NetworkError(.other, "A machine's key is 32 bytes.") }
        var error: UnsafeMutablePointer<RazorNetworkError>?
        let made: OpaquePointer? = lend([method, path] + headers.flatMap { [$0.0, $0.1] } + peer.relays) { bytes in
            let pairs = headers.indices.map { RazorNetworkHeader(name: bytes[2 + $0 * 2], value: bytes[3 + $0 * 2]) }
            let relays = Array(bytes[(2 + headers.count * 2)...])
            return pairs.withUnsafeBufferPointer { pairs in
                relays.withUnsafeBufferPointer { relays in
                    var to = RazorNetworkPeer()
                    withUnsafeMutableBytes(of: &to.key) { $0.copyBytes(from: peer.key) }
                    to.relays = relays.baseAddress
                    to.relay_count = relays.count
                    var head = RazorNetworkRequestHead(method: bytes[0], path: bytes[1], headers: pairs.baseAddress, header_count: pairs.count,
                                                       body_length: UInt64(length), protocol: pairing ? RAZOR_NETWORK_PAIRING : RAZOR_NETWORK_API,
                                                       connect: fresh == nil ? RAZOR_NETWORK_CACHED : RAZOR_NETWORK_FRESH,
                                                       connect_timeout_ms: UInt64((fresh ?? 0) * 1000))
                    return lock.withLock {
                        guard !closed, let request = razor_network_request(handle, &to, &head, &error) else { return nil }
                        outstanding += 1
                        return request
                    }
                }
            }
        }
        guard let made else { throw error == nil ? NetworkError(.other, "The network client is closed.") : NetworkError(error) }
        return NetworkRequest(made, ended: { self.lock.withLock { self.outstanding -= 1 } },
                              headed: { self.lock.withLock { self.lastHead = ProcessInfo.processInfo.systemUptime } })
    }
}

/// One request's blocking calls, as the C interface has them: one `write`, one `response` and one
/// `read` may run at once on different threads, and `cancel` beside any of them wakes them all.
/// Freed, it is cancelled.
public final class NetworkRequest: @unchecked Sendable {
    private let handle: OpaquePointer
    private let ended: @Sendable () -> Void, headed: @Sendable () -> Void
    fileprivate init(_ handle: OpaquePointer, ended: @escaping @Sendable () -> Void, headed: @escaping @Sendable () -> Void) {
        self.handle = handle; self.ended = ended; self.headed = headed
    }
    deinit { razor_network_request_free(handle); ended() }

    public func cancel() { razor_network_cancel(handle) }

    /// Blocks until the bytes are accepted for sending. After a 101 response these are the upgraded stream's raw bytes.
    public func write(_ bytes: Data) throws(NetworkError) {
        guard !bytes.isEmpty else { return }
        let error = bytes.withUnsafeBytes { razor_network_write(handle, RazorNetworkBytes(data: $0.bindMemory(to: UInt8.self).baseAddress, len: $0.count)) }
        if let error { throw NetworkError(error) }
    }

    /// Blocks for the response's head. Call once.
    public func response() throws(NetworkError) -> NetworkResponse {
        var error: UnsafeMutablePointer<RazorNetworkError>?
        guard let raw = razor_network_response(handle, &error) else { throw NetworkError(error) }
        headed()
        defer { razor_network_response_free(raw) }
        var headers: [String: String] = [:]
        for h in UnsafeBufferPointer(start: raw.pointee.headers, count: raw.pointee.header_count) {
            let (name, value) = (UnsafeBufferPointer(start: h.name.data, count: h.name.len), UnsafeBufferPointer(start: h.value.data, count: h.value.len))
            headers[String(decoding: name, as: UTF8.self).lowercased()] = String(decoding: value, as: UTF8.self)
        }
        return NetworkResponse(status: Int(raw.pointee.status), headers: headers)
    }

    /// Blocks for the next bytes of the body, at most 16 KiB; empty at its end.
    public func read() throws(NetworkError) -> Data {
        var data = Data(count: 16_384), count = 0
        let error = data.withUnsafeMutableBytes { razor_network_read(handle, $0.bindMemory(to: UInt8.self).baseAddress, $0.count, &count) }
        if let error { throw NetworkError(error) }
        return data.prefix(count)
    }
}

extension NetworkClient {
    /// Sends a request and returns when its response's head arrives. The body is read as it is asked
    /// for, each read on a thread of its own, so nothing piles up in memory while nobody reads.
    /// `idle` seconds waiting on the machine with no byte moving ends the request as timed out.
    /// Cancelling the reading task, or dropping the body, cancels the request. `sent` hears how much
    /// of the body has gone, of how much.
    public func open(_ peer: NetworkPeer, _ method: String, _ path: String, headers: [(String, String)] = [], body: NetworkBody = .none,
                     pairing: Bool = false, fresh: TimeInterval? = nil, idle: TimeInterval,
                     sent: (@Sendable (Int64, Int64) -> Void)? = nil) async throws(NetworkError) -> (NetworkResponse, AsyncThrowingStream<Data, any Error>) {
        let length: Int
        switch body {
        case .none: length = 0
        case .data(let data): length = data.count
        case .file(let url):
            guard let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize else { throw NetworkError(.other, "The file to send could not be read.") }
            length = size
        }
        let request = try request(peer, method, path, headers: headers, length: length, pairing: pairing, fresh: fresh)
        let watch = Watchdog(idle) { phase, bytes in
            let state = self.lock.withLock {
                let now = ProcessInfo.processInfo.systemUptime
                return (self.outstanding, Int(now - self.openedAt), self.lastHead.map { Int(now - $0) } ?? -1, self.changed)
            }
            Logger(subsystem: "com.treygoff.razor", category: "network").error("Request timeout phase=\(phase, privacy: .public) received=\(bytes) outstanding=\(state.0) client_age_s=\(state.1) since_head_s=\(state.2) network_changed=\(state.3)")
            request.cancel()
        }
        let head = try await blocking(request, watch) { () throws(NetworkError) -> NetworkResponse in
            // A machine may answer before the whole body has gone; its answer is then the result.
            var stopped: NetworkError?
            do throws(NetworkError) { try request.upload(body, of: length, watch, sent) } catch { stopped = error }
            do throws(NetworkError) { return try request.response() } catch { throw stopped ?? error }
        }
        watch.body()
        let chunks = AsyncThrowingStream<Data, any Error> {
            let bytes = try await blocking(request, watch) { () throws(NetworkError) -> Data in
                let bytes = try request.read()
                watch.received(bytes.count)
                return bytes
            }
            return bytes.isEmpty ? nil : bytes
        }
        return (head, chunks)
    }

    /// One whole exchange: the response's head and its body, of at most `limit` bytes.
    public func send(_ peer: NetworkPeer, _ method: String, _ path: String, headers: [(String, String)] = [], body: NetworkBody = .none,
                     pairing: Bool = false, fresh: TimeInterval? = nil, idle: TimeInterval, limit: Int = 64 << 20,
                     sent: (@Sendable (Int64, Int64) -> Void)? = nil) async throws(NetworkError) -> (NetworkResponse, Data) {
        let (head, chunks) = try await open(peer, method, path, headers: headers, body: body, pairing: pairing, fresh: fresh, idle: idle, sent: sent)
        var all = Data()
        do {
            for try await bytes in chunks {
                all += bytes
                if all.count > limit { throw NetworkError(.other, "The machine's answer was too long.") }
            }
        } catch { throw error as? NetworkError ?? NetworkError(.other, "The network operation failed.") }
        // A cancelled task ends the stream quietly; what was read is not the whole answer.
        if Task.isCancelled { throw NetworkError(.other, "The request was cancelled.") }
        return (head, all)
    }

    /// Spends a pairing code: the machine enrolls `keys` (public keys, this client's own among them)
    /// as one device and answers with its server id. On any failure this device is not paired, and
    /// pairing again needs a fresh code.
    public func join(_ code: PairingCode, keys: [Data]) async throws(NetworkError) -> String {
        let secret = code.secret.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
        let offer: [String: Any] = ["secret": secret, "keys": keys.map { $0.map { String(format: "%02x", $0) }.joined() }, "relays": code.peer.relays]
        guard let body = try? JSONSerialization.data(withJSONObject: offer) else { throw NetworkError(.other, "The pairing request could not be made.") }
        let (head, data) = try await send(code.peer, "POST", "/join", headers: [("content-type", "application/json")], body: .data(body),
                                          pairing: true, fresh: 10, idle: 10, limit: 4096)
        let reply = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        guard head.status == 200, let id = reply?["id"] as? String else {
            throw NetworkError(.other, reply?["error"] as? String ?? "Pairing was not acknowledged. Make a fresh code and try again.")
        }
        return id
    }
}

/// A pairing code as `razor devices add` shows it: `razor1-` and the unpadded base64url of a version
/// byte (1), the machine's key, a secret, then each relay as a two-byte length and its URL.
public struct PairingCode: Sendable {
    public let peer: NetworkPeer
    fileprivate let secret: Data

    public init?(_ text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.hasPrefix("razor1-"), text.utf8.count <= 2825 else { return nil }
        var base64 = text.dropFirst(7).replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        base64 += String(repeating: "=", count: (4 - base64.count % 4) % 4)
        guard let bytes = Data(base64Encoded: base64), bytes.count >= 49, bytes[0] == 1 else { return nil }
        var relays: [String] = [], rest = bytes.dropFirst(49)
        while let high = rest.first {
            guard rest.count >= 2, relays.count < 8 else { return nil }
            let count = Int(high) << 8 | Int(rest.dropFirst().first!)
            rest = rest.dropFirst(2)
            guard count <= 256, rest.count >= count, let relay = String(data: rest.prefix(count), encoding: .utf8) else { return nil }
            relays.append(relay)
            rest = rest.dropFirst(count)
        }
        peer = NetworkPeer(key: Data(bytes[1..<33]), relays: relays)
        secret = Data(bytes[33..<49])
    }
}

/// A streamed body's lines, each without its newline: Razor's event streams. A line longer than
/// Razor's cap on any answer or stream line, 64 MiB, ends the stream with an error.
public struct NetworkLines: AsyncSequence, Sendable {
    public typealias Element = Data
    let chunks: AsyncThrowingStream<Data, any Error>
    public init(_ chunks: AsyncThrowingStream<Data, any Error>) { self.chunks = chunks }
    public func makeAsyncIterator() -> Iterator { Iterator(chunks: chunks.makeAsyncIterator()) }

    public struct Iterator: AsyncIteratorProtocol {
        var chunks: AsyncThrowingStream<Data, any Error>.AsyncIterator
        /// What has come and is not yet a line, and how much of it holds no newline.
        var rest = Data(), scanned = 0, ended = false

        public mutating func next() async throws -> Data? {
            while true {
                if let end = rest[scanned...].firstIndex(of: 0x0A), end <= 64 << 20 {
                    defer { rest = Data(rest[(end + 1)...]); scanned = 0 }
                    return Data(rest[..<end])
                }
                scanned = rest.count
                guard rest.count <= 64 << 20 else { throw NetworkError(.other, "A line of the machine's answer was too long.") }
                if ended {
                    defer { rest = Data() }
                    return rest.isEmpty ? nil : rest
                }
                if let more = try await chunks.next() { rest.append(more) } else { ended = true }
            }
        }
    }
}

private extension NetworkRequest {
    /// Writes the body in pieces, so the watchdog and `sent` hear its progress. A file that cannot
    /// be read whole cancels the request, which would otherwise wait for bytes that never come.
    func upload(_ body: NetworkBody, of length: Int, _ watch: Watchdog, _ sent: (@Sendable (Int64, Int64) -> Void)?) throws(NetworkError) {
        var gone = 0
        func put(_ piece: Data) throws(NetworkError) {
            try write(piece)
            gone += piece.count
            watch.touch()
            sent?(Int64(gone), Int64(length))
        }
        switch body {
        case .none: break
        case .data(let data):
            for start in stride(from: data.startIndex, to: data.endIndex, by: 65_536) { try put(data[start..<min(start + 65_536, data.endIndex)]) }
        case .file(let url):
            let file = try? FileHandle(forReadingFrom: url)
            defer { try? file?.close() }
            while gone < length {
                guard let piece = try? file?.read(upToCount: min(65_536, length - gone)), !piece.isEmpty else {
                    cancel()
                    throw NetworkError(.other, "The file to send could not be read whole.")
                }
                try put(piece)
            }
        }
    }
}

/// Runs `work`, a request's blocking calls, on a thread of its own while the watchdog counts, and
/// cancels the request if the waiting task is cancelled.
private func blocking<T: Sendable>(_ request: NetworkRequest, _ watch: Watchdog, _ work: @escaping @Sendable () throws(NetworkError) -> T) async throws(NetworkError) -> T {
    do {
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (waiting: CheckedContinuation<T, any Error>) in
                Thread.detachNewThread {
                    watch.touch()
                    let result = Result { try work() }.mapError { watch.what($0) }
                    watch.rest()
                    waiting.resume(with: result)
                }
            }
        } onCancel: { request.cancel() }
    } catch { throw error as? NetworkError ?? NetworkError(.other, "The network operation failed.") }
}

/// Cancels a request through `expire` when `touch` is not called for `idle` seconds while the app
/// waits on the machine; `rest` stops the count while nobody waits.
private final class Watchdog: @unchecked Sendable {
    private let timer = DispatchSource.makeTimerSource(queue: .global())
    private let idle: TimeInterval
    private let lock = NSLock()
    private var expired = false
    // Before head includes opening the stream and uploading; the C interface has no separate open event.
    private var phase = "before head", bytes = 0

    init(_ idle: TimeInterval, _ expire: @escaping @Sendable (String, Int) -> Void) {
        self.idle = idle
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            let state = lock.withLock { expired = true; return (phase, bytes) }
            expire(state.0, state.1)
        }
        timer.resume()
    }

    deinit { timer.cancel() }

    func body() { lock.withLock { phase = "body" } }
    func received(_ count: Int) { lock.withLock { bytes += count } }
    func touch() { timer.schedule(deadline: .now() + idle) }
    func rest() { timer.schedule(deadline: .distantFuture) }

    /// The error to report: a request this watchdog cancelled timed out.
    func what(_ error: any Error) -> any Error {
        lock.withLock { expired } ? NetworkError(.timedOut, "The machine took too long to answer.") : error
    }
}

/// Lends each string's UTF-8 to `body`, as the C interface borrows bytes, for the length of the call.
private func lend<T>(_ strings: [String], _ body: ([RazorNetworkBytes]) -> T) -> T {
    let counts = strings.map(\.utf8.count)
    return Array(strings.joined().utf8).withUnsafeBufferPointer { all in
        var at = 0
        return body(counts.map { count in
            defer { at += count }
            return RazorNetworkBytes(data: all.baseAddress.map { $0 + at }, len: count)
        })
    }
}
