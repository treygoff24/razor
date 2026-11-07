//! `razor install`: the one service that runs `razor serve` and restarts it after a failure.

use super::{Error, TEXT, other};
use crate::config::{setup, shipped};
use std::ffi::CStr;
use std::fs::{File, TryLockError};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::fs;
use tokio::process::Command;

/// Maximum time for runtime tasks to finish after the API has shut down.
pub const TEARDOWN_TIMEOUT: Duration = Duration::from_secs(1);

/// Write and enable the service that runs `razor serve` and restarts it within a minute of a
/// failure: a systemd user unit on Linux, which starts at boot when lingering is on and at login
/// otherwise; a launchd agent on macOS, which starts at login. It is named after the home's
/// folder without a leading dot: razor for `~/.razor`, razor-personal for `~/.razor-personal`.
/// Its command is, in order: on macOS, Razor.app's program (see `app`); `server.wrapper`; a
/// release install's `current/razor`, so an update needs no reinstall, or else this binary; and
/// `serve`. It carries `PATH` and `RAZOR_HOME` as they are now; running this again rewrites it,
/// but a service of the same name for another home is refused, never replaced.
/// Returns the sentence to print, which says when it starts, or that no user manager answered.
pub async fn install(home: &Path) -> Result<String, Error> {
    let t = &*TEXT;
    let config = crate::config::Config::load(home).await;
    let config = config.map_err(|e| Error::Refused(e.to_string()))?;
    let mut exe = std::env::current_exe().map_err(other("finding this binary"))?;
    let mut argv = Vec::new();
    if let Some(root) = exe.parent().and_then(Path::parent).map(Path::to_path_buf)
        && fs::try_exists(root.join("current/razor"))
            .await
            .unwrap_or(false)
    {
        exe = root.join("current/razor");
        argv.extend(app(&root).await?);
    }
    argv.extend(config.server.wrapper);
    argv.extend([exe.display().to_string(), "serve".into()]);
    let (file, label) = service_file(home);
    let macos = cfg!(target_os = "macos");
    let (between, dollar) = if macos {
        ("</string><string>", "$")
    } else {
        ("\" \"", "$$")
    };
    for word in &mut argv {
        *word = encode(word).replace('$', dollar);
    }
    let saved_home = encode(&home.display().to_string());
    let values = [
        ("{label}", encode(&label)),
        ("{command}", argv.join(between)),
        ("{path}", encode(&std::env::var("PATH").unwrap_or_default())),
        ("{home}", saved_home.clone()),
    ];
    let existing = check_service(home).await?;
    stop(home).await?;
    let template = if macos { &t.plist } else { &t.unit };
    let shown = file.display().to_string();
    let text = (values.iter()).fold(template.to_string(), |s, (k, v)| s.replace(k, v));
    write(&file, &text).await?;
    let enabled = if macos {
        // SAFETY: `getuid` always succeeds and touches no memory.
        let domain = format!("gui/{}", unsafe { libc::getuid() });
        // Unloading the old agent fails when there is none, which is fine.
        run(&["launchctl", "bootout", &format!("{domain}/{label}")]).await;
        run(&["launchctl", "bootstrap", &domain, &file.to_string_lossy()]).await
    } else {
        let unit = format!("{label}.service");
        run(&["systemctl", "--user", "daemon-reload"]).await
            && run(&["systemctl", "--user", "enable", "--now", &unit]).await
    };
    if !enabled && !existing {
        fs::remove_file(&file)
            .await
            .map_err(other("removing the unenabled service file"))?;
    }
    let linger =
        Path::new("/var/lib/systemd/linger").join(std::env::var("USER").unwrap_or_default());
    let at_boot = !cfg!(target_os = "macos") && fs::try_exists(linger).await.unwrap_or(false);
    let sentence = match (enabled, at_boot) {
        (false, _) if existing => {
            return Ok(format!(
                "No user service manager enabled Razor; the existing {shown} was kept, so check this shell's service-manager connection and run razor install again."
            ));
        }
        (false, _) => &t.no_manager,
        (true, true) => &t.boot,
        (true, false) => &t.login,
    };
    Ok(sentence.replace("{file}", &shown))
}

/// A service value in the platform's file format: XML text or a systemd quoted word.
fn encode(value: &str) -> String {
    if cfg!(target_os = "macos") {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    } else {
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
            .replace('\n', "\\n")
    }
}

/// The file and name used by both installation and starting an installed service.
fn service_file(home: &Path) -> (PathBuf, String) {
    let folder = home.file_name().unwrap_or_default().to_string_lossy();
    let label = folder.trim_start_matches('.').to_owned();
    let user_home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let file = if cfg!(target_os = "macos") {
        user_home.join(format!("Library/LaunchAgents/{label}.plist"))
    } else {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| user_home.join(".config"));
        config.join(format!("systemd/user/{label}.service"))
    };
    (file, label)
}

/// A matching service file; absent or owned by another home returns None.
pub async fn installed(home: &Path) -> Result<Option<(PathBuf, String)>, Error> {
    let (file, label) = service_file(home);
    let old = match fs::read_to_string(&file).await {
        Ok(old) => old,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(other("reading the service file")(e)),
    };
    let home = encode(&home.display().to_string());
    let owns = if cfg!(target_os = "macos") {
        old.split_once("<key>RAZOR_HOME</key>")
            .is_some_and(|(_, rest)| {
                rest.trim_start()
                    .starts_with(&format!("<string>{home}</string>"))
            })
    } else {
        old.split_once("\nEnvironment=").is_some_and(|(_, rest)| {
            rest.lines()
                .next()
                .is_some_and(|line| line.contains(&format!("\"RAZOR_HOME={home}\"")))
        })
    };
    Ok(owns.then_some((file, label)))
}

/// Whether this home has a service; refuse an existing file belonging to another home.
pub async fn check_service(home: &Path) -> Result<bool, Error> {
    let installed = installed(home).await?.is_some();
    let (file, _) = service_file(home);
    if !installed
        && fs::try_exists(&file)
            .await
            .map_err(other("checking the service file"))?
    {
        return Err(Error::Refused(
            TEXT.taken.replace("{file}", &file.display().to_string()),
        ));
    }
    Ok(installed)
}

/// Start this home's service, refusing manager failures. False means no matching service.
pub async fn start(home: &Path) -> Result<bool, Error> {
    let Some((file, label)) = installed(home).await? else {
        return Ok(false);
    };
    let started = if cfg!(target_os = "macos") {
        // SAFETY: getuid always succeeds and touches no memory.
        let domain = format!("gui/{}", unsafe { libc::getuid() });
        let service = format!("{domain}/{label}");
        if run(&["launchctl", "print", &service]).await {
            run(&["launchctl", "kickstart", &service]).await
        } else {
            run(&["launchctl", "bootstrap", &domain, &file.to_string_lossy()]).await
        }
    } else {
        run(&["systemctl", "--user", "start", &format!("{label}.service")]).await
    };
    if !started {
        return Err(Error::Refused(format!(
            "Starting {} failed; run razor install.",
            file.display()
        )));
    }
    Ok(true)
}

/// Whether a nonblocking whole-file flock was taken; false means another open file description
/// holds it.
fn taken(attempt: Result<(), TryLockError>) -> Result<bool, Error> {
    match attempt {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(e)) => Err(other("locking server.lock")(e)),
    }
}

/// Whether this home's server holds its lock; a shared probe never excludes another probe.
pub fn server_running(home: &Path) -> Result<bool, Error> {
    let file = match File::open(home.join("server.lock")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(other("opening server.lock")(e)),
    };
    Ok(!taken(file.try_lock_shared())?)
}

/// Acquire the server's flock and write its PID; closing other descriptors cannot unlock it.
pub fn server_lock(home: &Path) -> Result<File, Error> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(home.join("server.lock"))
        .map_err(other("opening server.lock"))?;
    if !taken(file.try_lock())? {
        return Err(Error::Refused(format!(
            "A Razor server is already running for {}; run razor restart.",
            home.display()
        )));
    }
    file.set_len(0)
        .and_then(|()| writeln!(file, "{}", std::process::id()))
        .map_err(other("writing the server PID"))?;
    Ok(file)
}

/// SIGTERM the lock holder and wait for release; absent/unused locks succeed without signaling.
pub async fn stop(home: &Path) -> Result<(), Error> {
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .open(home.join("server.lock"))
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(other("opening server.lock")(e)),
    };
    if taken(file.try_lock_shared())? {
        return Ok(());
    }
    let mut text = String::new();
    // A PID is at most ten digits; never read an unbounded file a tool could have edited.
    (&file)
        .take(32)
        .read_to_string(&mut text)
        .map_err(other("reading the server PID"))?;
    // The holder may have exited during the read; never signal from an unused lock file.
    if taken(file.try_lock_shared())? {
        return Ok(());
    }
    let pid = text
        .trim()
        .parse::<libc::pid_t>()
        .ok()
        .filter(|pid| *pid > 1);
    let Some(pid) = pid else {
        return Err(Error::Refused(
            "The server holding server.lock has no PID in it yet; run razor stop again.".into(),
        ));
    };
    // SAFETY: a valid positive PID was read while the server's flock remained held.
    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::ESRCH) {
            return Err(other("stopping the server")(e));
        }
    }
    let released = async {
        loop {
            if taken(file.try_lock_shared())? {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(
        crate::api::SHUTDOWN_TIMEOUT + TEARDOWN_TIMEOUT + Duration::from_secs(1),
        released,
    )
    .await
    .unwrap_or_else(|_| {
        Err(Error::Refused(
            "The server still holds server.lock after its shutdown deadline; run razor stop again."
                .into(),
        ))
    })
}

/// On macOS, the path of Razor.app's program, `src/launcher.rs`, in `root` beside the versioned
/// folders: macOS charges what the service does to the program launchd started, so the person
/// grants permissions to Razor once, and a grant outlives updates. The current release's copy
/// replaces it when there is none, or when the release's is signed with a Developer ID, whose
/// grants hold across new bytes; an ad hoc signed copy never replaces one, since its grants hold
/// only for the same bytes.
async fn app(root: &Path) -> Result<Option<String>, Error> {
    let (app, shipped) = (root.join("Razor.app"), root.join("current/Razor.app"));
    let program = app.join("Contents/MacOS/Razor");
    let (new, old) = (root.join(".Razor.app.new"), root.join(".Razor.app.old"));
    let (shipped, copy) = (shipped.to_string_lossy(), new.to_string_lossy());
    let developer = ["codesign", "--verify", "-R=anchor apple generic", &shipped];
    if cfg!(target_os = "macos")
        && fs::metadata(shipped.as_ref()).await.is_ok()
        && (fs::metadata(&program).await.is_err() || run(&developer).await)
    {
        // A copy beside it, then a rename: new files, never new bytes over the old ones, which
        // macOS kills a signed program for; and the old bundle stays unless the copy is whole.
        let _ = fs::remove_dir_all(&new).await;
        if !run(&["ditto", &shipped, &copy]).await {
            return Err(Error::Refused(format!(
                "Copying {shipped} failed; run razor install again."
            )));
        }
        let _ = fs::remove_dir_all(&old).await;
        let _ = fs::rename(&app, &old).await;
        fs::rename(&new, &app)
            .await
            .map_err(other("replacing Razor.app"))?;
        let _ = fs::remove_dir_all(&old).await;
    }
    let found = fs::metadata(&program).await.is_ok();
    Ok(found.then(|| program.display().to_string()))
}

/// Whether macOS lets this server control the computer (Accessibility) and record the screen,
/// `[accessibility, screen recording]`; None off macOS. Under the service the answer is
/// Razor.app's. With `ask`, the server's home, it first asks macOS for both, which puts Razor in
/// both lists in System Settings and opens macOS's prompts; only the service's own server asks,
/// None from any other, since macOS charges a request to the program launchd started, which
/// elsewhere is the person's terminal. The first call, `desktop`'s at the server's start, loads
/// ApplicationServices, under 1 MiB kept.
pub(crate) async fn permissions(ask: Option<&Path>) -> Option<[bool; 2]> {
    if ask.is_some_and(|home| std::env::var("XPC_SERVICE_NAME").ok() != Some(service_file(home).1))
    {
        return None;
    }
    let ask = ask.is_some();
    let path = c"/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices";
    use std::mem::transmute;
    type Ref = *const libc::c_void;
    // SAFETY: the path and names are C strings, a framework is never unloaded, and each symbol
    // is taken as its C type: the checks and the screen request take nothing and return a C
    // bool; the prompt option and kCFBooleanTrue are CFTypeRef constants, passed by address as
    // one-item arrays; the dictionary retains them with Core Foundation's callbacks and is
    // released.
    let asked = move || unsafe {
        let library = libc::dlopen(path.as_ptr(), libc::RTLD_LAZY);
        if library.is_null() {
            return None;
        }
        let symbol = |name: &CStr| {
            Some(libc::dlsym(library, name.as_ptr()).cast_const()).filter(|s| !s.is_null())
        };
        let check = |name| Some(transmute::<Ref, extern "C" fn() -> bool>(symbol(name)?)());
        if ask {
            type Create = extern "C" fn(Ref, Ref, Ref, isize, Ref, Ref) -> Ref;
            let create = transmute::<Ref, Create>(symbol(c"CFDictionaryCreate")?);
            let options = create(
                std::ptr::null(),
                symbol(c"kAXTrustedCheckOptionPrompt")?,
                symbol(c"kCFBooleanTrue")?,
                1,
                symbol(c"kCFTypeDictionaryKeyCallBacks")?,
                symbol(c"kCFTypeDictionaryValueCallBacks")?,
            );
            let trusted = symbol(c"AXIsProcessTrustedWithOptions")?;
            transmute::<Ref, extern "C" fn(Ref) -> bool>(trusted)(options);
            transmute::<Ref, extern "C" fn(Ref)>(symbol(c"CFRelease")?)(options);
            check(c"CGRequestScreenCaptureAccess")?;
        }
        Some([
            check(c"AXIsProcessTrusted")?,
            check(c"CGPreflightScreenCaptureAccess")?,
        ])
    };
    tokio::task::spawn_blocking(asked).await.ok().flatten()
}

/// The socket in a home on which `desktop` runs Cua Driver's daemon; serve names it to agents'
/// shells as CUA_DRIVER_SOCKET.
pub const DESKTOP_SOCKET: &str = "cua-driver.sock";

/// Runs the Cua Driver Razor ships as a daemon, which agents' `cua-driver` calls need, as this
/// server's direct child on `home`'s own socket, so that on macOS it acts with Razor's
/// permissions (docs/design/starter.md); only where it can work: on macOS with Accessibility on,
/// on Linux with an X display. It starts again after an exit, except one within a minute of its
/// start, which ends it with the exit in the server's log. It ends with the server, which holds
/// the pipe on its standard input.
pub async fn desktop(home: PathBuf) {
    let usable = match permissions(None).await {
        Some([control, _]) => control,
        None => std::env::var_os("DISPLAY").is_some(),
    };
    let program = tokio::task::spawn_blocking(|| shipped("cua-driver"));
    let (true, Ok(Some(program))) = (usable, program.await) else {
        return;
    };
    let (socket, pid) = (home.join(DESKTOP_SOCKET), home.join("cua-driver.pid"));
    loop {
        // One an earlier daemon left: this server holds server.lock, so none answers on it, and
        // the daemon refuses to start beside one.
        let _ = fs::remove_file(&socket).await;
        let mut command = Command::new(&program);
        command.args(["serve", "--embedded", "--parent-liveness-stdio", "--socket"]);
        command.arg(&socket).arg("--pid-file").arg(&pid);
        command.env_clear().envs(setup::environment());
        command.stdin(Stdio::piped()).stdout(Stdio::null());
        let started = std::time::Instant::now();
        let mut child = match command.kill_on_drop(true).spawn() {
            Ok(child) => child,
            Err(e) => return eprintln!("Desktop control is off: cua-driver did not start: {e}."),
        };
        // Held until the daemon exits: `wait` would close it, which stops the daemon.
        let _pipe = child.stdin.take();
        let exit = child.wait().await;
        let exit = exit.map_or_else(|e| e.to_string(), |status| status.to_string());
        if started.elapsed() < Duration::from_secs(60) {
            return eprintln!(
                "Desktop control is off: Cua Driver's daemon exited ({exit}) within a minute of starting; fix what it printed above, then run razor restart."
            );
        }
        eprintln!("Cua Driver's daemon exited ({exit}); starting it again.");
    }
}

async fn write(file: &Path, text: &str) -> Result<(), Error> {
    let write = async {
        fs::create_dir_all(file.parent().unwrap_or(file)).await?;
        fs::write(file, text).await
    };
    write.await.map_err(other("writing the service file"))
}

/// Whether the command ran and exited 0.
async fn run(argv: &[&str]) -> bool {
    let [program, args @ ..] = argv else {
        return false;
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.status().await.is_ok_and(|status| status.success())
}
