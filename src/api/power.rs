//! macOS sleep: from just before the machine sleeps until it wakes, this home's server line says
//! `silent`, so the phone stops calling it.

use crate::protocol::{Millis, now};
use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::sync::watch;

// IOMessage.h: iokit_common_msg(0x270), (0x280) and (0x300).
const CAN_SLEEP: u32 = 0xe000_0270;
const WILL_SLEEP: u32 = 0xe000_0280;
const POWERED_ON: u32 = 0xe000_0300;

static ASLEEP: OnceLock<watch::Sender<Option<Millis>>> = OnceLock::new();
static ROOT: AtomicU32 = AtomicU32::new(0);

type Callback = extern "C" fn(*mut c_void, u32, u32, *mut c_void);

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IORegisterForSystemPower(
        refcon: *mut c_void,
        port: *mut *mut c_void,
        callback: Callback,
        notifier: *mut u32,
    ) -> u32;
    fn IOAllowPowerChange(root: u32, notification: isize) -> i32;
    fn IONotificationPortGetRunLoopSource(port: *mut c_void) -> *mut c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRun();
}

/// Holds when the machine is about to sleep, from the sleep notice until it has woken, and
/// None while it is awake; holds sleep back one second, for the status streams to send the line.
pub(super) fn watch(asleep: watch::Sender<Option<Millis>>) {
    if ASLEEP.set(asleep).is_err() {
        return;
    }
    std::thread::spawn(|| {
        let (mut port, mut notifier) = (std::ptr::null_mut(), 0);
        // SAFETY: IOKit fills both out-parameters this thread owns; the callback reads only statics.
        let root = unsafe {
            IORegisterForSystemPower(std::ptr::null_mut(), &mut port, changed, &mut notifier)
        };
        if root == 0 {
            eprintln!("razor: cannot watch for sleep; peers notice when it stops answering.");
            return;
        }
        ROOT.store(root, Ordering::Relaxed);
        // SAFETY: the port lives as long as this thread, whose run loop never returns.
        unsafe {
            let source = IONotificationPortGetRunLoopSource(port);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode);
            CFRunLoopRun();
        }
    });
}

extern "C" fn changed(_: *mut c_void, _: u32, message: u32, notification: *mut c_void) {
    let Some(asleep) = ASLEEP.get() else { return };
    let allow = || {
        // SAFETY: the root port and the notification id are the ones IOKit gave this callback.
        unsafe { IOAllowPowerChange(ROOT.load(Ordering::Relaxed), notification as isize) };
    };
    match message {
        CAN_SLEEP => allow(),
        WILL_SLEEP => {
            eprintln!("razor: going to sleep; this server's line says so.");
            asleep.send_replace(Some(now()));
            std::thread::sleep(Duration::from_secs(1));
            allow();
        }
        POWERED_ON => {
            eprintln!("razor: awake.");
            asleep.send_replace(None);
        }
        _ => {}
    }
}
