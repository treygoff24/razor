//! The C interface in `include/razor_network.h`: its structs, the checks on every
//! borrowed input, and the allocations it hands out. The work is in `request`.
// Each function's safety contract is its comment in razor_network.h.
#![allow(clippy::missing_safety_doc)]
mod request;

use crate::{Connect, Error, Protocol, client::prepare};
use http::{HeaderMap, Request};
use iroh::{EndpointAddr, EndpointId, RelayUrl, SecretKey};
pub use request::{RazorNetworkClient, RazorNetworkRequest};
use std::{
    ffi::{CStr, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice, str,
    time::Duration,
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RazorNetworkBytes {
    pub data: *const u8,
    pub len: usize,
}
#[repr(C)]
pub struct RazorNetworkHeader {
    pub name: RazorNetworkBytes,
    pub value: RazorNetworkBytes,
}
#[repr(C)]
pub struct RazorNetworkPeer {
    pub key: [u8; 32],
    pub relays: *const RazorNetworkBytes,
    pub relay_count: usize,
}
#[repr(C)]
pub struct RazorNetworkRequestHead {
    pub method: RazorNetworkBytes,
    pub path: RazorNetworkBytes,
    pub headers: *const RazorNetworkHeader,
    pub header_count: usize,
    pub body_length: u64,
    pub protocol: u32,
    pub connect: u32,
    pub connect_timeout_ms: u64,
}
#[repr(C)]
pub struct RazorNetworkError {
    pub code: u32,
    pub message: *const c_char,
}
#[repr(C)]
pub struct RazorNetworkResponse {
    pub status: u16,
    pub headers: *const RazorNetworkHeader,
    pub header_count: usize,
}
/// A response and the header bytes it points into, freed together.
#[repr(C)]
struct ResponseAllocation {
    response: RazorNetworkResponse,
    headers: Vec<RazorNetworkHeader>,
    bytes: Vec<Box<[u8]>>,
}

/// A request as copied from C, before its task starts.
struct Input {
    peer: EndpointAddr,
    protocol: Protocol,
    connect: Connect,
    request: Request<()>,
    length: u64,
}

/// A C error; each has one static sentence naming no key and no body.
#[derive(Clone, Copy, PartialEq)]
enum Failure {
    Refused,
    NotSent,
    Uncertain,
    Cancelled,
    Invalid,
    Other,
}

impl Failure {
    fn into_raw(self) -> *mut RazorNetworkError {
        let (code, message) = match self {
            Self::Refused => (6, REFUSED),
            Self::NotSent => (2, NOT_SENT),
            Self::Uncertain => (3, UNCERTAIN),
            Self::Cancelled => (4, CANCELLED),
            Self::Invalid => (5, INVALID),
            Self::Other => (5, OTHER),
        };
        let message = message.as_ptr();
        Box::into_raw(Box::new(RazorNetworkError { code, message }))
    }
}

const REFUSED: &CStr = c"This key is not enrolled on that machine; pair it again.";
const NOT_SENT: &CStr = c"The request was not sent; try connecting again.";
const UNCERTAIN: &CStr = c"The request may have run; check its result before sending again.";
const CANCELLED: &CStr = c"The request was cancelled; check its result before sending again.";
const INVALID: &CStr = c"The network input is invalid; check the call's arguments.";
const OTHER: &CStr = c"The network operation failed; check the connection and try again.";

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        match error {
            Error::Refused => Self::Refused,
            Error::NotSent { .. } => Self::NotSent,
            Error::Uncertain { .. } => Self::Uncertain,
            Error::Other { .. } => Self::Other,
        }
    }
}

/// Runs an entry call's body; a panic becomes Other and never crosses into C.
fn guard<T>(f: impl FnOnce() -> Result<T, Failure>) -> Result<T, Failure> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(Err(Failure::Other))
}

/// For calls that return nothing: a panic is dropped at the boundary.
fn quietly(f: impl FnOnce()) {
    let _ = catch_unwind(AssertUnwindSafe(f));
}

/// NULL on success, or an allocated error.
fn status(f: impl FnOnce() -> Result<(), Failure>) -> *mut RazorNetworkError {
    guard(f).err().map_or(ptr::null_mut(), Failure::into_raw)
}

/// Sets `*error` to NULL or to an allocated error. Without valid error storage nothing runs.
unsafe fn allocate<T>(
    error: *mut *mut RazorNetworkError,
    f: impl FnOnce() -> Result<*mut T, Failure>,
) -> *mut T {
    // SAFETY: the caller lends writable error storage for the call; put refuses NULL or misalignment.
    if unsafe { put(error, ptr::null_mut()) }.is_err() {
        return ptr::null_mut();
    }
    guard(f).unwrap_or_else(|failure| {
        // SAFETY: put has just written this storage.
        unsafe { error.write(failure.into_raw()) };
        ptr::null_mut()
    })
}

/// A borrowed handle or struct; NULL or misaligned is invalid.
unsafe fn borrow<'a, T>(p: *const T) -> Result<&'a T, Failure> {
    if !p.is_aligned() {
        return Err(Failure::Invalid);
    }
    // SAFETY: a non-NULL C pointer is valid for the whole call.
    unsafe { p.as_ref() }.ok_or(Failure::Invalid)
}

/// Writes an out value; NULL or misaligned storage is invalid.
unsafe fn put<T>(p: *mut T, value: T) -> Result<(), Failure> {
    if p.is_null() || !p.is_aligned() {
        return Err(Failure::Invalid);
    }
    // SAFETY: a non-NULL out pointer is exclusive, writable storage for the call.
    unsafe { p.write(value) };
    Ok(())
}

/// A borrowed array: NULL only with zero length, aligned, at most isize::MAX bytes.
unsafe fn array<'a, T>(p: *const T, len: usize) -> Result<&'a [T], Failure> {
    if len == 0 {
        return Ok(&[]);
    }
    if p.is_null() || !p.is_aligned() || len > isize::MAX as usize / size_of::<T>() {
        return Err(Failure::Invalid);
    }
    // SAFETY: the caller lends len initialized elements for the call.
    Ok(unsafe { slice::from_raw_parts(p, len) })
}

/// The bytes one call may copy as metadata (method, path, headers and relays,
/// with their descriptors): 64 KiB, checked before each array is borrowed.
struct Metadata(usize);

impl Metadata {
    unsafe fn slice<'a, T>(&mut self, p: *const T, len: usize) -> Result<&'a [T], Failure> {
        let size = len.checked_mul(size_of::<T>());
        self.0 = size
            .and_then(|n| self.0.checked_sub(n))
            .ok_or(Failure::Invalid)?;
        // SAFETY: the caller lends len elements for the call.
        unsafe { array(p, len) }
    }
    unsafe fn bytes<'a>(&mut self, bytes: RazorNetworkBytes) -> Result<&'a [u8], Failure> {
        // SAFETY: the caller lends these bytes for the call.
        unsafe { self.slice(bytes.data, bytes.len) }
    }
    unsafe fn relays(
        &mut self,
        p: *const RazorNetworkBytes,
        count: usize,
    ) -> Result<Vec<RelayUrl>, Failure> {
        // SAFETY: the caller lends count relay descriptors and their bytes for the call.
        let relays = unsafe { self.slice(p, count) }?;
        let parse = |relay| {
            // SAFETY: as above.
            let text = str::from_utf8(unsafe { self.bytes(relay) }?);
            text.ok()
                .and_then(|t| t.parse().ok())
                .ok_or(Failure::Invalid)
        };
        relays.iter().copied().map(parse).collect()
    }
}

unsafe fn input(peer: &RazorNetworkPeer, head: &RazorNetworkRequestHead) -> Result<Input, Failure> {
    let mut metadata = Metadata(65_536);
    // SAFETY: the request's descriptors and their bytes are borrowed for the call.
    let (method, path, headers, relays) = unsafe {
        (
            metadata.bytes(head.method)?,
            str::from_utf8(metadata.bytes(head.path)?).map_err(|_| Failure::Invalid)?,
            metadata.slice(head.headers, head.header_count)?,
            metadata.relays(peer.relays, peer.relay_count)?,
        )
    };
    let mut request = Request::builder().method(method).uri(path);
    for header in headers {
        // SAFETY: as above.
        let (name, value) =
            unsafe { (metadata.bytes(header.name)?, metadata.bytes(header.value)?) };
        request = request.header(name, value);
    }
    let mut request = request.body(()).map_err(|_| Failure::Invalid)?;
    prepare(&mut request, head.body_length).map_err(|_| Failure::Invalid)?;
    let protocol = match head.protocol {
        0 => Protocol::Api,
        1 => Protocol::Pairing,
        _ => return Err(Failure::Invalid),
    };
    let connect = match (head.connect, head.connect_timeout_ms) {
        (0, 0) => Connect::Cached,
        (1, ms) => Connect::Fresh {
            timeout: Duration::from_millis(ms),
        },
        _ => return Err(Failure::Invalid),
    };
    let key = EndpointId::from_bytes(&peer.key).map_err(|_| Failure::Invalid)?;
    let peer = relays
        .into_iter()
        .fold(EndpointAddr::new(key), EndpointAddr::with_relay_url);
    Ok(Input {
        peer,
        protocol,
        connect,
        request,
        length: head.body_length,
    })
}

fn response_allocation(status: u16, map: &HeaderMap) -> *mut RazorNetworkResponse {
    let bytes: Vec<Box<[u8]>> = map
        .iter()
        .flat_map(|(name, value)| [name.as_str().as_bytes().into(), value.as_bytes().into()])
        .collect();
    let lend = |b: &[u8]| RazorNetworkBytes {
        data: b.as_ptr(),
        len: b.len(),
    };
    let headers: Vec<_> = bytes
        .chunks(2)
        .map(|pair| RazorNetworkHeader {
            name: lend(&pair[0]),
            value: lend(&pair[1]),
        })
        .collect();
    let response = RazorNetworkResponse {
        status,
        headers: headers.as_ptr(),
        header_count: headers.len(),
    };
    Box::into_raw(Box::new(ResponseAllocation {
        response,
        headers,
        bytes,
    }))
    .cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_open(
    key: *const u8,
    relays: *const RazorNetworkBytes,
    relay_count: usize,
    error: *mut *mut RazorNetworkError,
) -> *mut RazorNetworkClient {
    // SAFETY: the inputs are borrowed for the call and error is writable storage.
    unsafe {
        allocate(error, || {
            let key = array(key, 32)?.try_into().map_err(|_| Failure::Invalid)?;
            let relays = Metadata(65_536).relays(relays, relay_count)?;
            RazorNetworkClient::open(SecretKey::from_bytes(key), relays)
                .map(|client| Box::into_raw(Box::new(client)))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_request(
    client: *mut RazorNetworkClient,
    peer: *const RazorNetworkPeer,
    head: *const RazorNetworkRequestHead,
    error: *mut *mut RazorNetworkError,
) -> *mut RazorNetworkRequest {
    // SAFETY: the caller borrows a live client and input structs for the call.
    unsafe {
        allocate(error, || {
            let input = input(borrow(peer)?, borrow(head)?)?;
            let request = borrow(client)?.request(input)?;
            Ok(Box::into_raw(Box::new(request)))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_write(
    request: *mut RazorNetworkRequest,
    bytes: RazorNetworkBytes,
) -> *mut RazorNetworkError {
    // SAFETY: the caller borrows a live handle and the bytes for the call.
    status(|| unsafe { borrow(request)?.write(array(bytes.data, bytes.len)?) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_finish(
    request: *mut RazorNetworkRequest,
) -> *mut RazorNetworkError {
    // SAFETY: the caller borrows a live handle for the call.
    status(|| unsafe { borrow(request) }?.finish())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_response(
    request: *mut RazorNetworkRequest,
    error: *mut *mut RazorNetworkError,
) -> *mut RazorNetworkResponse {
    // SAFETY: the caller borrows a live handle and writable error storage.
    unsafe { allocate(error, || borrow(request)?.response()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_read(
    request: *mut RazorNetworkRequest,
    output: *mut u8,
    capacity: usize,
    read: *mut usize,
) -> *mut RazorNetworkError {
    status(|| {
        // SAFETY: the caller lends count storage, capacity writable bytes and a live handle.
        unsafe {
            put(read, 0)?;
            if output.is_null() || capacity == 0 || capacity > isize::MAX as usize {
                return Err(Failure::Invalid);
            }
            let output = slice::from_raw_parts_mut(output, capacity);
            put(read, borrow(request)?.read(output)?)
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_cancel(request: *mut RazorNetworkRequest) {
    // SAFETY: cancellation borrows a live handle (NULL is a no-op) and may race its I/O calls.
    quietly(|| {
        if let Ok(request) = unsafe { borrow(request) } {
            request.cancel();
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_request_free(request: *mut RazorNetworkRequest) {
    if !request.is_null() {
        // SAFETY: the caller has joined every call on the handle and frees it exactly once.
        quietly(|| unsafe { Box::from_raw(request) }.cancel());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_close(client: *mut RazorNetworkClient) {
    if !client.is_null() {
        // SAFETY: close owns the allocation; no request is created and no close runs at once.
        quietly(|| unsafe { Box::from_raw(client) }.close());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_network_change(client: *mut RazorNetworkClient) {
    // SAFETY: the caller borrows a live client for the call (NULL is a no-op).
    quietly(|| {
        if let Ok(client) = unsafe { borrow(client) } {
            client.network_change();
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_response_free(response: *mut RazorNetworkResponse) {
    if !response.is_null() {
        // SAFETY: the caller returns one response allocation exactly once.
        drop(unsafe { Box::from_raw(response.cast::<ResponseAllocation>()) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn razor_network_error_free(error: *mut RazorNetworkError) {
    if !error.is_null() {
        // SAFETY: the caller returns one error allocation exactly once.
        drop(unsafe { Box::from_raw(error) });
    }
}
