#ifndef RAZOR_NETWORK_H
#define RAZOR_NETWORK_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Pull interface to the encrypted network client.
 * Every input pointer is borrowed only until its call returns and is copied if retained.
 * A byte pointer may be NULL only with length zero. All out pointers are required.
 * All functions may run on any caller thread; blocking calls belong on a worker queue.
 * No callback enters Swift. The library owns its async runtime.
 * NULL arrays require zero counts, and nonzero counts require non-NULL arrays.
 */
typedef struct RazorNetworkClient RazorNetworkClient;
typedef struct RazorNetworkRequest RazorNetworkRequest;

/* Caller-owned bytes, without a terminating NUL; may contain binary data. */
typedef struct {
    const uint8_t *data;
    size_t len;
} RazorNetworkBytes;

/* Header names are ASCII; values are HTTP bytes. Repeated headers stay repeated. */
typedef struct {
    RazorNetworkBytes name;
    RazorNetworkBytes value;
} RazorNetworkHeader;

/* The raw Ed25519 public key, and UTF-8 relay URLs (not NUL-terminated).
 * NULL relays with count zero means no supplied peer relays, never public defaults.
 */
typedef struct {
    uint8_t key[32];
    const RazorNetworkBytes *relays;
    size_t relay_count;
} RazorNetworkPeer;

/* The caller chooses the protocol for every HTTP request. Pairing always dials fresh. */
typedef enum {
    RAZOR_NETWORK_API = 0,       /* razor/1 */
    RAZOR_NETWORK_PAIRING = 1    /* razor-pair/1 */
} RazorNetworkProtocol;

/* FRESH skips the connection cache and never inserts into it. CACHED is API-only;
 * selecting CACHED for PAIRING returns OTHER. */
typedef enum {
    RAZOR_NETWORK_CACHED = 0,
    RAZOR_NETWORK_FRESH = 1
} RazorNetworkConnect;

/* Origin-form path, including query; method is an HTTP token. No URL authority.
 * body_length is the exact HTTP request body length; zero means no body. The body
 * ends automatically when that many bytes have been written; excess bytes fail.
 * connect_timeout_ms must be positive for FRESH and zero for CACHED; it wraps
 * iroh's own connect timeout. A fresh handshake failure or deadline is NOT_SENT;
 * after the handshake, HTTP exchange failures are UNCERTAIN.
 * The library supplies Host and framing; conflicting Host, Connection, Content-Length
 * or Transfer-Encoding headers are rejected before sending. Upgrade headers are allowed.
 */
typedef struct {
    RazorNetworkBytes method;
    RazorNetworkBytes path;
    const RazorNetworkHeader *headers;
    size_t header_count;
    uint64_t body_length;
    RazorNetworkProtocol protocol;
    RazorNetworkConnect connect;
    uint64_t connect_timeout_ms;
} RazorNetworkRequestHead;

typedef enum {
    RAZOR_NETWORK_NOT_SENT = 2,
    RAZOR_NETWORK_UNCERTAIN = 3,
    RAZOR_NETWORK_CANCELLED = 4,
    RAZOR_NETWORK_OTHER = 5,
    RAZOR_NETWORK_REFUSED = 6 /* Nothing was sent; this key is not enrolled on that machine. */
} RazorNetworkErrorCode;

/* Library allocation, read-only through C; error_free releases it and its UTF-8,
 * NUL-terminated message. NOT_SENT and REFUSED prove the HTTP request was not sent.
 * UNCERTAIN must never be resent automatically. CANCELLED makes no retry promise.
 * OTHER covers invalid input and all other failures without a retry guarantee.
 * Messages never contain secret keys or HTTP request/response bodies.
 */
typedef struct {
    RazorNetworkErrorCode code;
    const char *message;
} RazorNetworkError;

/* Library allocation. response_free releases this and all header bytes together.
 * Body reads use the request handle independently of this allocation's lifetime.
 */
typedef struct {
    uint16_t status;
    const RazorNetworkHeader *headers;
    size_t header_count;
} RazorNetworkResponse;

/* Blocks while binding. key points to exactly 32 raw secret-key bytes, copied here.
 * relays is the caller's list from pairing; an empty list means no relays, including
 * NULL with count zero. Uses only these UTF-8 URLs; no public default or lookup.
 * Returns a library-owned client, or NULL and an allocated *error. On success *error
 * is NULL. The identity must be unique among live endpoints, including extensions.
 */
RazorNetworkClient *razor_network_open(const uint8_t key[32], const RazorNetworkBytes *relays,
                                       size_t relay_count, RazorNetworkError **error);

/* Copies inputs and returns before dialing or waiting on the network. The library
 * owns the handle; request_free releases it. Failure returns NULL and *error.
 * Success sets *error to NULL. The HTTP request is never replayed.
 * Metadata (method, path, headers and peer relays) is capped at 64 KiB per request;
 * HTTP response headers have the same cap. At most 64 live requests are admitted;
 * exhaustion returns OTHER without queuing. At most 16 cached connections plus
 * 16 active uncached/evicted connections retain at most 32 MiB of QUIC receive
 * payload in total (1 MiB per connection, 256 KiB per stream); transport metadata
 * is additional. In-progress dials reserve a slot in these bounds; an HTTP
 * request needing another connection returns OTHER before dialing.
 * The notification extension's single connection accounts for 1 MiB of that bound.
 * Explicit C upload and download pull queues each hold at most 16 KiB per request:
 * at most 2 MiB of queues and 8 MiB of HTTP metadata for all live requests.
 * Hyper's HTTP working buffer adds at most 64 KiB per request (4 MiB for 64 requests).
 * Caller deadlines cancel the handle; callers bound total HTTP response bodies.
 * These are retained payload bounds, not a total process-memory guarantee.
 */
RazorNetworkRequest *razor_network_request(RazorNetworkClient *client, const RazorNetworkPeer *peer,
                                         const RazorNetworkRequestHead *head, RazorNetworkError **error);

/* Blocks under backpressure until all bytes are accepted or a terminal error occurs.
 * Success is NULL; failure is an allocated error and may follow partial acceptance.
 * An invalid or excess HTTP body write returns OTHER before accepting its bytes.
 * A successful write accepts bytes for sending; it does not prove delivery.
 * After a 101 response this writes raw upgraded bytes, without HTTP framing.
 */
RazorNetworkError *razor_network_write(RazorNetworkRequest *request, RazorNetworkBytes bytes);

/* After a 101 HTTP response, half-closes the upgraded stream's write side.
 * Call once after upgrading; before a 101 this is invalid input (OTHER).
 * Returns NULL or an allocated error. Ordinary HTTP bodies end at body_length.
 */
RazorNetworkError *razor_network_finish(RazorNetworkRequest *request);

/* Blocks for final response headers; may run while another thread uploads so an
 * early rejection can be received. Call once. Returns a library-owned response or
 * NULL and *error (NULL on success). For 101, read/write become the raw byte stream;
 * the caller supplies WebSocket framing. An early HTTP response may end an upload
 * before body_length; its headers and body remain readable after the upload ends.
 */
RazorNetworkResponse *razor_network_response(RazorNetworkRequest *request, RazorNetworkError **error);

/* After response, blocks for at most capacity bytes into caller-owned output.
 * capacity must be positive. Sets *read to the count (zero on EOF or error).
 * NULL with *read == 0 is clean EOF; a failure returns an allocated error and
 * never masquerades as EOF. After EOF, later reads return the same EOF; after
 * a stream failure or cancellation, waiting and later I/O returns that terminal error.
 */
RazorNetworkError *razor_network_read(RazorNetworkRequest *request, uint8_t *output,
                                      size_t capacity, size_t *read);

/* Idempotent, thread-safe cancellation; stops only this QUIC stream and wakes
 * blocked write/finish/response/read calls with CANCELLED. An earlier terminal
 * error or clean EOF is preserved. NULL is allowed.
 * One read, one write (including finish), and one response call may overlap;
 * never two of the same operation. cancel may overlap any of them.
 */
void razor_network_cancel(RazorNetworkRequest *request);

/* Cancels, then frees the library-owned request. NULL is allowed. The caller must
 * first join all calls using this handle; free must never race those calls/cancel.
 */
void razor_network_request_free(RazorNetworkRequest *request);

/* Cancels all requests, closes the endpoint, joins the runtime and frees the client.
 * NULL is allowed. Do not race request creation or another close. Existing request
 * handles remain valid and cancelled; join their I/O calls, then request_free them.
 * Responses/errors remain valid until their own frees, including after close.
 */
void razor_network_close(RazorNetworkClient *client);

/* Has the endpoint look for new network paths at once, without waiting; for a platform
 * whose network changes iroh cannot see itself (Android), called on each change.
 * NULL is allowed. Do not race close. */
void razor_network_network_change(RazorNetworkClient *client);

/* Release exactly one library allocation, including its children. NULL is allowed. */
void razor_network_response_free(RazorNetworkResponse *response);
void razor_network_error_free(RazorNetworkError *error);

#ifdef __cplusplus
}
#endif
#endif
