//! `razor setup`, the first run (docs/design/setup.md), and the keys it saves in keys.toml.
//! Setup asks in the person's terminal and blocks on them throughout; nothing else
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

/// The optional keys `razor keys` asks for: the web providers' and ElevenLabs' for voice.
pub fn names() -> impl Iterator<Item = &'static str> {
    (crate::tools::web::keys().map(|(name, _)| name)).chain(["ELEVENLABS_API_KEY"])
}

/// The names keys.toml listed when this process started.
static SAVED: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// Sets each variable home/keys.toml lists that the environment lacks, so an exported value wins,
/// and keeps the file's names for `environment`. Main calls it before any thread starts. An error
/// never holds a value. A leftover key file, which Razor no longer reads, is named on standard
/// error: doctor's terminal, serve's log.
pub fn load(home: &Path) -> Result<(), Error> {
    let old = home.join("key");
    if old.exists() {
        let old = old.display();
        eprintln!(
            "Key file: {old} is no longer read; save its key as OPENAI_API_KEY in keys.toml, name the model openai/<id>, then delete the file."
        );
    }
    let keys = saved(&home.join("keys.toml"))?.1;
    for (name, key) in keys
        .iter()
        .filter(|(name, _)| std::env::var_os(name).is_none())
    {
        // SAFETY: main calls this before the runtime starts any thread; `saved` checked both.
        unsafe { std::env::set_var(name, key.as_str().unwrap_or_default()) }
    }
    let _ = SAVED.set(keys.iter().map(|(name, _)| name.to_owned()).collect());
    Ok(())
}

/// The environment agents' shells, MCP servers and Cua Driver's daemon start from: this
/// process's, less `names`, every name keys.toml listed, set from it or exported, and Claude's
/// keys. Razor's own keys stay with Razor: a program that printed its environment would put them
/// in the record (Trey, 2026-10-28, for the web keys; 6 Nov 2026, for every key keys.toml holds).
pub(crate) fn environment() -> std::collections::BTreeMap<String, String> {
    let saved = SAVED.get().into_iter().flatten().map(String::as_str);
    let withheld: Vec<_> = names().chain(saved).chain(crate::claude::KEYS).collect();
    let vars = std::env::vars();
    vars.filter(|(name, _)| !withheld.contains(&name.as_str()))
        .collect()
}

/// Where `name`'s value comes from, never the value: keys.toml when the file holds what the
/// environment has, else the environment; `None` when it is unset.
pub(crate) fn source(home: &Path, name: &str) -> Option<&'static str> {
    let value = std::env::var(name).ok().filter(|value| !value.is_empty())?;
    let keys = saved(&home.join("keys.toml")).map(|(_, keys)| keys);
    let filed =
        keys.is_ok_and(|keys| keys.get(name).and_then(toml_edit::Item::as_str) == Some(&value));
    Some(["the environment", "keys.toml"][usize::from(filed)])
}

/// keys.toml's text, for a compare-and-replace, and its keys: each named in letters, digits and
/// `_`, not starting with a digit, and a nonempty string with no NUL (`set_var` would panic and
/// print it). A TOML error quotes the file, so none is shown.
fn saved(path: &Path) -> Result<(String, DocumentMut), Error> {
    let before = pack::text(path, true)?;
    let keys = before.parse::<DocumentMut>().ok().filter(|keys| {
        keys.iter().all(|(name, value)| {
            !name.starts_with(|c: char| c.is_ascii_digit())
                && !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && (value.as_str()).is_some_and(|v| !v.trim().is_empty() && !v.contains('\0'))
        })
    });
    let wrong = || {
        Error::Invalid(format!(
            "{} holds lines NAME = \"key\", each name letters, digits and _, each key a nonempty string.",
            path.display()
        ))
    };
    Ok((before, keys.ok_or_else(wrong)?))
}

/// Saves `key` as `name` in keys.toml at 0600, keeping its other lines.
fn save(home: &Path, name: &str, key: &str) -> Result<(), Error> {
    let path = home.join("keys.toml");
    let (before, mut keys) = saved(&path)?;
    keys[name] = toml_edit::value(key);
    replace(&path, &before, &keys.to_string())
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
    let prompt = "Use Claude, ChatGPT, both, or an API key [Claude]";
    let (claude, chatgpt) = ask(
        prompt,
        "type claude, chatgpt, both or key",
        |typed| match letters(typed).to_lowercase().as_str() {
            "" | "c" | "claude" => Some((true, false)),
            "g" | "gpt" | "chatgpt" | "codex" => Some((false, true)),
            "b" | "both" => Some((true, true)),
            "k" | "key" | "api" | "apikey" => Some((false, false)),
            _ => None,
        },
    )?;
    if (claude || chatgpt) && loaded.provider_only {
        let why = "provider_only is on, and a Claude or ChatGPT sign-in never satisfies it; run razor setup and choose key, or set provider_only = false first.";
        return Err(Error::Invalid(why.into()));
    }
    super::make_home(home)?;
    // A top-level key may always open the file, so the person's own lines stay as they are.
    let file = home.join("config.toml");
    let before = pack::text(&file, true)?;
    let parsed = before.parse::<DocumentMut>();
    let mut settings = parsed.map_err(|e| Error::Invalid(format!("{}: {e}", file.display())))?;
    let mut only = false;
    if !claude && !chatgpt {
        let provider = provider(home, &mut settings)?;
        only = yes("Contact nothing but your model provider? (for work machines)")?;
        // Either answer is written, so a rerun's no turns a saved yes off.
        settings["provider_only"] = toml_edit::value(only);
        if only {
            implicit(&mut settings, "providers")[&provider].or_insert(toml_edit::table());
        }
    } else if !claude && settings.get("model").is_none() {
        settings["model"] = toml_edit::value("sol");
    }
    let connect = !only && yes("Connect to your Razor servers on other machines")?;
    reach(&mut settings, &file, &loaded.server, connect).await;
    let after = settings.to_string();
    if after != before {
        replace(&file, &before, &after)?;
    }
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
    let shown = browser.display().to_string().replace('\'', "'\\''");
    if only {
        eprintln!(
            "With provider_only, setup downloads no browser; to add it, run '{shown}' install."
        );
    } else if !ran(Command::new(&browser).arg("install")) && cfg!(target_os = "linux") {
        eprintln!("Missing system libraries? Run '{shown}' install --with-deps; it uses sudo.");
    }
    if !ran(Command::new(&razor).arg("doctor")) {
        let why = "Fix what doctor reports, then run razor setup again.";
        return Err(Error::Invalid(why.into()));
    }
    yes("Open Razor for your first message")
}

/// Asks which provider's API key to use and the key, hidden; saves it in keys.toml at 0600 and
/// makes the provider's model the default, and calls' when the settings name none. Another
/// endpoint also gets its `[providers]` table and, with a known window, its model's row; with no
/// key typed or saved, it sends none. Its provider's name.
fn provider(home: &Path, settings: &mut DocumentMut) -> Result<String, Error> {
    const SHIPPED: [(&str, &str); 4] = [
        ("anthropic", "anthropic/opus"),
        ("openai", "openai/gpt-6.1-sol"),
        ("google", "gemini-3.8-flash"),
        ("runinfra", "deepseek-v4.1-flash-1017"),
    ];
    let prompt =
        "Whose key: anthropic, openai, google, runinfra, or other for another endpoint [other]";
    let help = "type anthropic, openai, google, runinfra or other";
    let read = |typed: &str| match letters(typed).to_lowercase().as_str() {
        "" | "other" => Some(None),
        typed => SHIPPED.iter().find(|(p, _)| *p == typed).map(Some),
    };
    let (name, model, key) = match ask(prompt, help, read)? {
        Some((name, model)) => {
            let route = crate::profiles::model(model, &Config::default()).ok();
            let key = route.and_then(|r| r.endpoint.key).unwrap_or_default();
            (name.to_string(), model.to_string(), key)
        }
        None => other(settings)?,
    };
    let typed = secret(&format!(
        "{key} (typing is hidden; Enter keeps a saved one)"
    ))?;
    if !typed.is_empty() {
        save(home, &key, &typed)?;
    } else if saved(&home.join("keys.toml"))?.1.get(&key).is_none() {
        // Another endpoint with no key sends none; a shipped one's doctor names the missing key.
        let providers = settings.get_mut("providers").and_then(|p| p.get_mut(&name));
        providers
            .and_then(|p| p.as_table_like_mut())
            .map(|t| t.remove("key"));
    }
    settings["model"] = toml_edit::value(&model);
    if settings.get("call_model").is_none() {
        settings["call_model"] = toml_edit::value(&model);
    }
    Ok(name)
}

/// Another endpoint's format, address, model id and window: writes its `[providers]` table,
/// named after its host's first label, with a key variable named after it, and the model's row
/// when the window is known. Its name, the model's and the key variable's.
fn other(settings: &mut DocumentMut) -> Result<(String, String, String), Error> {
    let prompt = "Its API: chat (OpenAI chat completions), responses (OpenAI Responses) or anthropic (Anthropic Messages) [chat]";
    let formats = ["chat", "responses", "anthropic"];
    let read = |typed: &str| match letters(typed).to_lowercase() {
        typed if typed.is_empty() => Some("chat"),
        typed => formats.into_iter().find(|f| *f == typed),
    };
    let format = ask(prompt, "type chat, responses or anthropic", read)?;
    let prompt = "Its address, the base URL its requests start with (often ending in /v1)";
    let help = "type an http:// or https:// address, such as https://gateway.example.com/v1";
    let read = |typed: &str| {
        let url = reqwest::Url::parse(typed).ok()?;
        let host = url
            .host_str()
            .filter(|_| url.scheme().starts_with("http"))?;
        Some((typed.to_owned(), host.to_owned()))
    };
    let (url, host) = ask(prompt, help, read)?;
    let read = |id: &str| Some(id.to_owned()).filter(|id| !id.is_empty());
    let id = ask(
        "The model id it serves",
        "type the id the endpoint knows",
        read,
    )?;
    let prompt = "Its context window in tokens, if you know it [unknown]";
    let read = |typed: &str| match typed.parse::<i64>() {
        _ if typed.is_empty() => Some(None),
        tokens => tokens.ok().filter(|t| *t > 0).map(Some),
    };
    let window = ask(prompt, "type a number of tokens, such as 200000", read)?;
    let name = Some(device_name(&host)).filter(|n| n.starts_with(char::is_alphabetic));
    let name = name.unwrap_or_else(|| "local".into());
    let key = format!("{}_API_KEY", name.to_uppercase().replace('-', "_"));
    let mut table = toml_edit::Table::new();
    table["format"] = toml_edit::value(format);
    table["base_url"] = toml_edit::value(url);
    table["key"] = toml_edit::value(&key);
    table["models"] = toml_edit::value(toml_edit::Array::from_iter([id.as_str()]));
    implicit(settings, "providers")[&name] = toml_edit::Item::Table(table);
    let model = format!("{name}/{id}");
    if let Some(window) = window {
        implicit(settings, "models")[&model]["window"] = toml_edit::value(window);
    }
    Ok((name, model, key))
}

/// Asks `prompt` until `read` takes the answer, saying `help` after each answer it refuses.
fn ask<T>(prompt: &str, help: &str, read: impl Fn(&str) -> Option<T>) -> Result<T, Error> {
    loop {
        let typed = line(prompt)?;
        match read(&typed) {
            Some(answer) => return Ok(answer),
            None => eprintln!("Razor read {typed:?}; {help}, then Enter."),
        }
    }
}

/// The top-level table `name`, made without a header of its own when missing.
fn implicit<'a>(settings: &'a mut DocumentMut, name: &str) -> &'a mut toml_edit::Item {
    let item = settings.entry(name).or_insert(toml_edit::table());
    if let Some(table) = item.as_table_mut() {
        table.set_implicit(true);
    }
    item
}

/// Names this server where the settings do not, and with `connect` and Tailscale running gives
/// it an address other machines reach where they set none; says what it wrote and where to
/// change it.
async fn reach(settings: &mut DocumentMut, file: &Path, server: &super::Server, connect: bool) {
    let shown = file.display();
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
    if !connect
        || settings
            .get("server")
            .and_then(|s| s.get("address"))
            .is_some()
        || server.address.is_some()
    {
        return;
    }
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
