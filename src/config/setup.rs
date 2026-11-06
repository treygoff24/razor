//! `razor setup`, the first run (docs/design/setup.md), and the optional keys it saves in
//! keys.toml. Setup asks in the person's terminal and blocks on them throughout; nothing else
//! runs in its process. Beyond saving configuration, each step is a program the person could run
//! themselves: `razor install`, each provider's own sign-in, `agent-browser install` and `razor
//! doctor`; then main opens the terminal app if they want it. Setup writes to standard error,
//! which is never buffered, so a prompt shows before its answer is read.

use super::{Config, Error, mcp::replace, pack};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use serde_json::Value;
use std::io::{self, IsTerminal};
use std::net::{IpAddr, SocketAddr};
use std::ops::RangeInclusive;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use toml_edit::DocumentMut;

/// The ports setup gives a tailnet address and `razor devices find` looks for servers on, besides the
/// server's own.
pub(crate) const PORTS: RangeInclusive<u16> = 7433..=7442;

/// The variables keys.toml may set: the web providers' keys and ElevenLabs' for voice. A model's
/// sign-in is never one of them; each provider's own login keeps it.
pub fn names() -> impl Iterator<Item = &'static str> {
    (crate::tools::web::keys().map(|(name, _)| name)).chain(["ELEVENLABS_API_KEY"])
}

/// Sets each of `names` that the environment lacks from home/keys.toml, so an exported value
/// wins. Main calls it before any thread starts. An error never holds a value.
pub fn load(home: &Path) -> Result<(), Error> {
    let keys = saved(&home.join("keys.toml"))?.1;
    for name in names().filter(|name| std::env::var_os(name).is_none()) {
        if let Some(key) = keys.get(name).and_then(toml_edit::Item::as_str) {
            // SAFETY: main calls this before the runtime starts any thread.
            unsafe { std::env::set_var(name, key) }
        }
    }
    Ok(())
}

/// keys.toml's text, for a compare-and-replace, and its keys: only `names`, each a nonempty
/// string with no NUL (`set_var` would panic and print it). A TOML error quotes the file, so none
/// is shown.
fn saved(path: &Path) -> Result<(String, DocumentMut), Error> {
    let before = pack::text(path, true)?;
    let keys = before.parse::<DocumentMut>().ok().filter(|keys| {
        keys.iter().all(|(name, value)| {
            names().any(|n| n == name)
                && value
                    .as_str()
                    .is_some_and(|v| !v.trim().is_empty() && !v.contains('\0'))
        })
    });
    let wrong = || {
        let (names, path) = (names().collect::<Vec<_>>().join(", "), path.display());
        Error::Invalid(format!("{path} may set only {names}, as nonempty strings."))
    };
    Ok((before, keys.ok_or_else(wrong)?))
}

/// Save optional keys together at 0600, retaining each saved value on Enter. Cancellation
/// saves none; a running server needs a restart before it sees changed keys.
pub fn keys(home: &Path) -> Result<(), Error> {
    terminal("keys")?;
    let path = home.join("keys.toml");
    let (before, mut keys) = saved(&path)?;
    eprintln!(
        "Web search and voice keys are optional; typing is hidden, Enter keeps a saved one. An ElevenLabs key needs Text to Speech and Voices (read) permissions."
    );
    for name in names() {
        let key = secret(name)?;
        if !key.is_empty() {
            keys[name] = toml_edit::value(key);
        }
    }
    super::make_home(home)?;
    let after = keys.to_string();
    replace(&path, &before, &after)?;
    if after != before {
        eprintln!("Keys saved; run razor restart to use them in a running server.");
    }
    Ok(())
}

/// True when doctor passed and the person wants the terminal app. Each provider owns its
/// sign-in; console selects Claude's Console billing rather than its subscription.
pub async fn run(home: &Path, console: bool) -> Result<bool, Error> {
    terminal("setup")?;
    let loaded = Config::load(home).await?;
    let (claude, chatgpt) = loop {
        let typed = line("Use Claude, ChatGPT or both [Claude]")?;
        match letters(&typed).to_lowercase().as_str() {
            "" | "c" | "claude" => break (true, false),
            "g" | "gpt" | "chatgpt" | "openai" | "codex" => break (false, true),
            "b" | "both" => break (true, true),
            _ => eprintln!("Razor read {typed:?}; type claude, chatgpt or both, then Enter."),
        }
    };
    super::make_home(home)?;
    // A top-level key may always open the file, so the person's own lines stay as they are.
    let file = home.join("config.toml");
    let config = pack::text(&file, true)?;
    if !claude && !pack::parse(&config, &file, false)?.0.contains_key("model") {
        replace(&file, &config, &format!("model = \"sol\"\n{config}"))?;
    }
    reach(&file, &loaded.server).await?;
    let razor = std::env::current_exe().map_err(|e| {
        Error::Invalid(format!(
            "Cannot find the Razor executable ({e}); reinstall Razor, then run razor setup."
        ))
    })?;
    if yes("Run Razor in the background when you log in")? {
        ran(Command::new(&razor).arg("install"));
    }
    let check = (claude && loaded.claude.sign_in.is_empty()).then_some(&[][..]);
    let missing = crate::api::ready::credentials(home, check, None).await;
    if claude && (console || missing.get("Claude").is_some_and(Option::is_some)) {
        let signed = crate::claude::login(console).await;
        signed.unwrap_or_else(|e| {
            eprintln!(
                "{e} Run razor setup{}.",
                if console { " --console" } else { "" }
            )
        });
    }
    if chatgpt && missing.get("ChatGPT sign-in").is_some_and(Option::is_some) {
        ran(Command::new(&razor).arg("login"));
    }
    // A release ships agent-browser beside razor, a folder only `serve` puts on PATH.
    let browser = tool("agent-browser");
    if !ran(Command::new(&browser).arg("install")) && cfg!(target_os = "linux") {
        eprintln!(
            "Missing system libraries? Run '{}' install --with-deps; it uses sudo.",
            browser.display().to_string().replace('\'', "'\\''")
        );
    }
    if !ran(Command::new(&razor).arg("doctor")) {
        let why = "Fix what doctor reports, then run razor setup again.";
        return Err(Error::Invalid(why.into()));
    }
    yes("Open Razor for your first message")
}

/// Names this server, and with Tailscale running gives it an address other machines reach, where
/// the settings set neither; says what it wrote and where to change it.
async fn reach(file: &Path, server: &super::Server) -> Result<(), Error> {
    let before = pack::text(file, true)?;
    let shown = file.display();
    let parsed = before.parse::<DocumentMut>();
    let mut settings = parsed.map_err(|e| Error::Invalid(format!("{shown}: {e}")))?;
    let name = device_name(&crate::profiles::machine());
    if settings.get("server").and_then(|s| s.get("name")).is_none()
        && server.name.is_empty()
        && !name.is_empty()
    {
        settings["server"]["name"] = toml_edit::value(&name);
        eprintln!(
            "This server is named {name}: other machines pair with it under that name, and new sessions here start with it. To change it, edit name under [server] in {shown}."
        );
    }
    if settings
        .get("server")
        .and_then(|s| s.get("address"))
        .is_none()
        && server.address.is_none()
    {
        match address().await {
            Some(address) => {
                settings["server"]["address"] = toml_edit::value(&address);
                eprintln!(
                    "Other machines on your tailnet reach this server at {address}. To change it, edit address under [server] in {shown}, then run razor restart."
                );
            }
            None => eprintln!(
                "No Tailscale address is available here, so other machines cannot reach this server yet. To pair two machines, put both on one tailnet (tailscale.com) and run razor setup on each, or set address under [server] in {shown} to an address the other can reach."
            ),
        }
    }
    let after = settings.to_string();
    match after == before {
        true => Ok(()),
        false => replace(file, &before, &after),
    }
}

/// `host`'s first label as a device name: lowercase letters, digits and single hyphens, at most 32.
fn device_name(host: &str) -> String {
    let mut name = String::new();
    for c in host.split('.').next().unwrap_or_default().chars() {
        let c = if c.is_ascii_alphanumeric() {
            c.to_ascii_lowercase()
        } else {
            '-'
        };
        if name.len() < 32 && !(c == '-' && (name.is_empty() || name.ends_with('-'))) {
            name.push(c);
        }
    }
    name.trim_end_matches('-').into()
}

/// This machine's tailnet address with the first port of `PORTS` free on it: its MagicDNS name
/// when that resolves here to its Tailscale IPv4, else that IPv4.
async fn address() -> Option<String> {
    let status = tailnet().await?;
    let ip: IpAddr = status["Self"]["TailscaleIPs"]
        .as_array()?
        .iter()
        .filter_map(|ip| ip.as_str()?.parse::<IpAddr>().ok())
        .find(IpAddr::is_ipv4)?;
    let mut ports = PORTS.clone();
    let mut port = None;
    for candidate in &mut ports {
        if tokio::net::TcpListener::bind((ip, candidate)).await.is_ok() {
            port = Some(candidate);
            break;
        }
    }
    let port = port?;
    let name = status["Self"]["DNSName"].as_str().unwrap_or_default();
    let named = format!("{}:{port}", name.trim_end_matches('.'));
    let found = tokio::time::timeout(Duration::from_secs(2), tokio::net::lookup_host(&named)).await;
    Some(
        match found.is_ok_and(|found| found.is_ok_and(|mut found| found.any(|a| a.ip() == ip))) {
            true => named,
            false => SocketAddr::new(ip, port).to_string(),
        },
    )
}

/// `tailscale status --json`, from PATH or the macOS app, within 5 seconds; None when Tailscale is
/// absent, fails or is not running.
pub(crate) async fn tailnet() -> Option<Value> {
    for program in [
        "tailscale",
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    ] {
        let mut command = tokio::process::Command::new(program);
        // The macOS app's program opens its window unless told it is a command.
        command
            .args(["status", "--json"])
            .env("TAILSCALE_BE_CLI", "1");
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(5), command.output()).await
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let status: Value = serde_json::from_slice(&output.stdout).ok()?;
        return (status["BackendState"] == "Running").then_some(status);
    }
    None
}

/// What the person typed, as letters and digits: a terminal's escape sequence (a focus report,
/// a pasted bracket) is dropped whole, and a backspace or delete a remote tty passed through
/// literally removes the character before it.
fn letters(typed: &str) -> String {
    let (mut out, mut chars) = (String::new(), typed.chars());
    while let Some(c) = chars.next() {
        match c {
            '\x1b' if matches!(chars.next(), Some('[' | 'O')) => {
                _ = chars.find(|n| n.is_ascii_alphabetic() || *n == '~')
            }
            '\x08' | '\x7f' => _ = out.pop(),
            c if c.is_ascii_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    out
}

/// A shipped tool, with PATH as the fallback for source builds.
pub fn tool(name: &str) -> std::path::PathBuf {
    super::shipped(name).unwrap_or(name.into())
}

fn terminal(command: &str) -> Result<(), Error> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::Invalid(format!(
            "Run razor {command} in a terminal."
        )));
    }
    Ok(())
}

/// Runs `command` in this terminal: whether it exited 0.
fn ran(command: &mut Command) -> bool {
    command.status().is_ok_and(|status| status.success())
}

/// One line, edited by the terminal itself, which also bounds it (4,095 bytes on Linux, 1,024
/// on macOS). The end of input stops setup.
fn line(prompt: &str) -> Result<String, Error> {
    eprint!("{prompt}: ");
    let mut text = String::new();
    match io::stdin()
        .read_line(&mut text)
        .map_err(|e| stopped("setup", e))?
    {
        0 => Err(stopped("setup", io::ErrorKind::UnexpectedEof.into())),
        _ => Ok(text.trim().to_string()),
    }
}

/// A [y/N] question: whether the answer starts with y.
pub fn yes(question: &str) -> Result<bool, Error> {
    Ok(line(&format!("{question} [y/N]"))?.starts_with(['y', 'Y']))
}

/// One line that never shows: raw mode covers the prompt and the typing, and a typed Ctrl-C or
/// Ctrl-D stops key entry with the terminal restored. It keeps at most 4 KiB.
fn secret(prompt: &str) -> Result<String, Error> {
    crossterm::terminal::enable_raw_mode().map_err(|e| stopped("keys", e))?;
    eprint!("{prompt}: ");
    let mut key = String::new();
    let typed = loop {
        match event::read() {
            Ok(Event::Key(k)) => match (k.code, k.modifiers.contains(KeyModifiers::CONTROL)) {
                (KeyCode::Enter, _) => break Ok(key),
                (KeyCode::Char('c' | 'd'), true) => break Err(io::ErrorKind::Interrupted.into()),
                (KeyCode::Backspace, _) => drop(key.pop()),
                (KeyCode::Char(c), false) if key.len() < 4 << 10 => key.push(c),
                _ => {}
            },
            Err(e) => break Err(e),
            _ => {}
        }
    };
    let restored = crossterm::terminal::disable_raw_mode();
    eprintln!();
    restored.and(typed).map_err(|e| stopped("keys", e))
}

fn stopped(command: &str, error: io::Error) -> Error {
    Error::Invalid(format!(
        "Input stopped ({error}); run razor {command} again."
    ))
}
