//! `razor`, the binary: the command line's words to the library's commands. `razor serve` runs
//! the API server; the session commands, `razor login`, `razor devices` and `razor mcp` are its
//! clients (docs/design/claude-slice.md and docs/design/api.md say exactly what they read and
//! write); `razor plugins` needs no server, and `razor setup` runs the others in order for a
//! first run (docs/design/setup.md).

use razor_harness::api;
use razor_harness::config::{self, INSTALL_DIR, setup};
use razor_harness::extensions;
use razor_harness::protocol::{ClientInput, Effort, End, SessionId, Speed};
use razor_harness::residents;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tokio::io::AsyncReadExt;

/// Why `razor` stopped, as one sentence for the person: a usage error exits 2, a failure 1.
enum Error {
    Usage(String),
    Failed(String),
}
use Error::{Failed, Usage};

/// A failure in the library is its own sentence.
impl<E: std::error::Error> From<E> for Error {
    fn from(e: E) -> Self {
        Failed(e.to_string())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Text,
    Json,
    Stream,
}

#[derive(Default)]
struct Args {
    model: Option<String>,
    effort: Option<Effort>,
    speed: Option<Speed>,
    resume: Option<SessionId>,
    continue_newest: bool,
    agents: bool,
    print: Option<Mode>,
    message: Option<String>,
}

fn mode(word: &str) -> Option<Mode> {
    match word {
        "text" => Some(Mode::Text),
        "json" => Some(Mode::Json),
        "stream" => Some(Mode::Stream),
        _ => None,
    }
}

/// One of a protocol choice's values, by the word its JSON spells it with.
fn choice<T: serde::de::DeserializeOwned>(word: &str) -> Option<T> {
    serde_json::from_value(Value::from(word)).ok()
}

fn parse(mut words: std::vec::IntoIter<String>) -> Result<Args, Error> {
    let mut args = Args::default();
    let printing = words
        .as_slice()
        .iter()
        .any(|word| word == "--print" || word.starts_with("--print="));
    while let Some(word) = words.next() {
        let mut value = |flag: &str| {
            words
                .next()
                .ok_or_else(|| Usage(format!("{flag} needs a value.")))
        };
        match word.as_str() {
            "--model" => args.model = Some(value("--model")?),
            "--speed" => {
                let speed = choice(&value("--speed")?);
                let why = || Usage("--speed is standard, fast or ultrafast.".into());
                args.speed = Some(speed.ok_or_else(why)?);
            }
            "--effort" => {
                let effort = value("--effort")?;
                let why = || {
                    Usage(format!(
                        "--effort is low, medium, high, xhigh or max, not {effort}."
                    ))
                };
                args.effort = Some(choice(&effort).ok_or_else(why)?);
            }
            "--resume" => {
                let id = SessionId::try_from(value("--resume")?);
                args.resume = Some(id.map_err(|e| Usage(e.to_string()))?);
            }
            "--continue" => args.continue_newest = true,
            "--agents" => args.agents = true,
            "--print" => {
                args.print = Some(Mode::Text);
                let mut ahead = words.clone();
                if let Some(m) = ahead.next().as_deref().and_then(mode) {
                    args.print = Some(m);
                    words = ahead;
                }
            }
            w if w.starts_with("--print=") => {
                let m = mode(&w[8..]);
                args.print = Some(m.ok_or_else(|| {
                    Usage(format!("--print is text, json or stream, not {}.", &w[8..]))
                })?)
            }
            w if w.starts_with("--") => return Err(Usage(format!("razor has no flag {w}."))),
            _ if args.message.is_none() && !printing && !word.chars().any(char::is_whitespace) => {
                return Err(Usage(unknown(&word)));
            }
            _ if args.message.is_some() => {
                return Err(Usage("Give the message as one argument, in quotes.".into()));
            }
            _ => args.message = Some(word),
        }
    }
    if args.resume.is_some() && args.continue_newest {
        return Err(Usage(
            "--resume and --continue cannot be used together.".into(),
        ));
    }
    Ok(args)
}

/// What `razor help`, `--help` and `-h` print, before anything is read.
const USAGE: &str = "\
Usage: razor [options] [\"message\"]  open the terminal app, with a first message if given
       razor --print[=text|json|stream] [options] [\"message\"]  answer once; else read stdin
       razor <command>
Options:
  --model <name>             choose a model (for example opus, sonnet or sol)
  --effort <low|medium|high|xhigh|max>  choose reasoning effort
  --speed <standard|fast|ultrafast>    choose service speed
  --resume <id>              reopen a session
  --continue                 reopen this folder's newest session
  --agents                   open the agents list beside your last conversation
  --print[=text|json|stream]  answer once; read stdin when no message is given
  --version                  print the installed version
  --help, -h                 show this help
Print modes: text prints the final reply; json prints the session id and final message;
stream prints each event as one JSON line.
Exit codes: 0 success; 1 failed; 2 invalid usage.
Environment: RAZOR_HOME selects the home (default ~/.razor).
Docs: ~/.local/share/razor/current/docs/README.md in a standard install, or https://github.com/treygoff24/razor/tree/main/docs

";

const VOICE_USE: &str = "Use razor config get|unset voice.<field> or set voice.<field> <value>.";
const COMMANDS: &[&str] = &[
    "help                       show this help",
    "setup [--console]          choose Claude, ChatGPT or an API key, offer a service and sign in\n  --console bills Claude through Anthropic Console",
    "keys                       save optional web-search and voice keys privately",
    "doctor                     check prerequisites and the default model, with fixes",
    "login [claude|chatgpt|console] sign in to an AI account (default ChatGPT)",
    "install                    run the server in the background at login",
    "serve                      run the server in this terminal",
    "stop                       stop this home's server",
    "restart                    stop and start this home's server",
    "update                     download and select the latest release; restart to use it; prints any action needed",
    "devices                    list paired devices\ndevices add <name>         show a code here; wait for the other device\ndevices join <name>        paste a code on the joining device\ndevices add <name> --key <key>  enroll a program on this machine with a key\ndevices remove <name>      remove a paired device",
    "mcp list                   list MCP servers\nmcp add <name> --scope <global|project|private> --config '<JSON>'\nmcp remove|enable|disable <name> --scope <global|project|private>\nmcp reconnect|sign-in|sign-out <name>  reconnect or change sign-in\nmcp import <file> --scope <global|project|private>  preview definitions; apply with /mcp import in the app\nmcp complete '<redirect address>'  finish a browser sign-in\n  --scope defaults to private",
    "config get <key>           print an effective setting and its source\nconfig set <key> <value> [--project|--local]  save a setting\nconfig unset <key> [--project|--local]  remove an override\nconfig path [--project|--local]  print the settings file\n  default: your own file; --project: shared project; --local: private project\nconfig get|set|unset voice.<field>  read or change a voice setting this home's server keeps",
    "plugins                    list configured plugins",
    "dream [folder]             run this project's dreaming pass now; print its session id",
];

/// Print a command's syntax, or all help; unknown commands return false without side effects.
fn help(command: Option<&str>) -> bool {
    if let Some(command) = command {
        let Some(text) = COMMANDS
            .iter()
            .find(|text| text.split_whitespace().next() == Some(command))
        else {
            return false;
        };
        println!("Usage: razor {text}\nRun razor --help for launch options and all commands.");
    } else {
        println!("{USAGE}Commands:");
        for text in COMMANDS {
            println!("  {}", text.replace('\n', "\n  "))
        }
    }
    true
}

fn unknown(command: &str) -> String {
    let pointer = match command {
        "task" | "memory" | "agent" | "message" => format!(
            "{command} is a tool in your Razor session; there is no razor {command} command. "
        ),
        _ => format!("Razor has no command {command}. "),
    };
    let commands = COMMANDS
        .iter()
        .filter_map(|text| text.split_whitespace().next())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{pointer}Commands: {commands}. To send a one-word message, open razor and type it, or use razor --print {command}."
    )
}

/// `--version` and every way of asking for help, answered before anything is read; None for
/// any other command line.
fn informed(words: &[String]) -> Option<ExitCode> {
    let first = words.first().map(String::as_str);
    let refused = |word: &str| {
        eprintln!("{} Run razor --help.", unknown(word));
        Some(ExitCode::from(2))
    };
    let (topic, word) = match first {
        Some("--version") if words.len() == 1 => {
            println!("razor {}", env!("CARGO_PKG_VERSION"));
            return Some(ExitCode::SUCCESS);
        }
        Some("--version") => {
            eprintln!("Run razor --version without arguments. Run razor --help.");
            return Some(ExitCode::from(2));
        }
        Some("help" | "--help" | "-h") if words.len() == 1 => (None, "help"),
        Some("help") if words.len() == 2 => match words[1].as_str() {
            "--help" | "-h" => (Some("help"), "help"),
            topic => (Some(topic), topic),
        },
        Some("help" | "--help" | "-h") => return refused(&words[1]),
        _ if words
            .iter()
            .skip(1)
            .any(|w| matches!(w.as_str(), "--help" | "-h")) =>
        {
            let command = |w: &&str| !w.starts_with('-') && !w.chars().any(char::is_whitespace);
            (first.filter(command), first.unwrap_or_default())
        }
        _ => return None,
    };
    if help(topic) {
        return Some(ExitCode::SUCCESS);
    }
    refused(word)
}

/// The process's environment, set before any thread starts: serve and doctor fill absent key
/// variables from keys.toml, every program Razor runs reaches this home's Cua daemon, and the
/// server's agents find Razor's own programs.
fn environment(home: Option<&Path>, first: Option<&str>) -> Result<(), ExitCode> {
    if let (Some("serve" | "doctor"), Some(home)) = (first, home)
        && let Err(e) = setup::load(home)
    {
        eprintln!("{e} Repair the named keys.toml, then run razor keys.");
        return Err(ExitCode::FAILURE);
    }
    // Every cua-driver Razor runs, serve's daemon, agents' calls and doctor's probe, reaches this
    // home's daemon (residents::desktop) and sends neither Cua's telemetry nor an update check
    // (docs/security.md).
    if let Some(home) = home {
        let cua = [
            ("CUA_DRIVER_EMBEDDED", "1"),
            ("CUA_DRIVER_RS_TELEMETRY_ENABLED", "false"),
            ("CUA_DRIVER_RS_UPDATE_CHECK", "false"),
        ];
        // SAFETY: no other threads exist before the runtime starts.
        unsafe {
            std::env::set_var("CUA_DRIVER_SOCKET", home.join(residents::DESKTOP_SOCKET));
            for (name, value) in cua {
                std::env::set_var(name, value)
            }
        }
    }
    // Agents' shells find Razor's own programs first and the ripgrep and jq it ships in tools/
    // last, so the person's own copies of those win. An empty entry would be the working folder,
    // so none is kept.
    if let (Some("serve"), Some(dir)) = (first, &*INSTALL_DIR) {
        let old = std::env::var_os("PATH").unwrap_or_default();
        let old = std::env::split_paths(&old).filter(|p| !p.as_os_str().is_empty());
        let dirs = [dir.clone()].into_iter().chain(old);
        if let Ok(path) = std::env::join_paths(dirs.chain([dir.join("tools")])) {
            // SAFETY: no other threads exist before the runtime starts.
            unsafe { std::env::set_var("PATH", path) };
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    std::sync::LazyLock::force(&INSTALL_DIR);
    let words: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = informed(&words) {
        return code;
    }
    let first = words.first().map(String::as_str);
    let home = std::env::var_os("RAZOR_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".razor")));
    if let Err(code) = environment(home.as_deref(), first) {
        return code;
    }
    let serving = first == Some("serve");
    let mut runtime = match serving {
        true => tokio::runtime::Builder::new_multi_thread(),
        false => tokio::runtime::Builder::new_current_thread(),
    };
    // A current-thread runtime ignores the worker count.
    let runtime = match runtime.worker_threads(2).enable_all().build() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("Razor cannot start its runtime: {e}.");
            return ExitCode::FAILURE;
        }
    };
    let ran = match home {
        Some(home) => runtime.block_on(run(home, words)),
        None => Err(Usage(
            "Set RAZOR_HOME or HOME, so razor knows where its state lives.".into(),
        )),
    };
    if serving {
        runtime.shutdown_timeout(residents::TEARDOWN_TIMEOUT);
    }
    match ran {
        Ok(code) => code,
        Err(Usage(sentence)) => {
            eprintln!("{sentence} Run razor --help.");
            ExitCode::from(2)
        }
        Err(Failed(sentence)) => {
            eprintln!("{sentence}");
            ExitCode::FAILURE
        }
    }
}

async fn run(home: PathBuf, words: Vec<String>) -> Result<ExitCode, Error> {
    match words.first().map(String::as_str) {
        Some(
            command
            @ ("serve" | "keys" | "install" | "stop" | "restart" | "update" | "doctor" | "plugins"),
        ) if words.len() != 1 => {
            return Err(Usage(format!("Run razor {command} without arguments.")));
        }
        Some("serve") => api::serve(home).await?,
        Some("update") => tokio::select! {
            sentence = residents::update_release(None) => println!("{}", sentence?),
            _ = tokio::signal::ctrl_c() => return Ok(ExitCode::from(130)),
        },
        Some("setup") if words.len() == 1 || words[1..] == ["--console"] => {
            if setup::run(&home, words.len() == 2).await? {
                session(home, Args::default()).await?;
            }
        }
        Some("setup") => return Err(Usage("Use razor setup or razor setup --console.".into())),
        Some("keys") => setup::keys(&home)?,
        Some(command @ ("stop" | "restart")) => {
            residents::stop(&home).await?;
            if command == "restart" {
                api::connect(&home).await?;
            }
        }
        Some("install") => println!("{}", residents::install(&home).await?),
        Some("mcp") => {
            let request = api::mcp::parse(&home, &words[1..]).map_err(|e| Usage(e.to_string()))?;
            let (client, key) = api::connect(&home).await?;
            println!("{}", api::mcp::run(&client, &key, request).await?);
        }
        Some("config") if words.get(2).is_some_and(|k| k.starts_with("voice.")) => {
            manage(home, &words).await?
        }
        Some("config") => match config::cli::run(&home, &words[1..]).await? {
            Some(text) => println!("{text}"),
            None => return Err(Usage("Use razor config get <key>, set <key> <value>, unset <key>, or path; writes and path accept --project or --local.".into())),
        },
        Some("dream") => {
            if words.len() > 2 { return Err(Usage("Use razor dream [folder].".into())); }
            let server = api::connection(&home).await?;
            let folder = words.get(1).map(PathBuf::from).unwrap_or_else(|| server.cwd.clone());
            let folder = if folder.is_absolute() { folder } else { server.cwd.join(folder) };
            let url = server.url.join("v1/residents/project/run/dream")?;
            let response = server.http.post(url).bearer_auth(&server.key).query(&[("cwd", &folder)])
                .send().await?;
            let status = response.status();
            let text = response.text().await?;
            if !status.is_success() { return Err(Failed(text)); }
            let result: Value = serde_json::from_str(&text)?;
            if result["off"] == true { println!("Dreaming is off for this project."); }
            else { println!("{}", result["session"].as_str().ok_or_else(|| Failed("Dreaming returned no session id.".into()))?); }
        }
        Some("login" | "devices") => manage(home, &words).await?,
        Some("doctor") => api::doctor(&home).await?,
        Some("plugins") => {
            let config = config::Config::load(&home).await?;
            print!("{}", extensions::list(&home, &config.plugins).await);
        }
        _ => session(home, parse(words.into_iter())?).await?,
    }
    Ok(ExitCode::SUCCESS)
}

/// `razor login` and `razor devices [add <name> [--key <key>] | join <name> | remove <name>]`,
/// through the server.
async fn manage(home: PathBuf, words: &[String]) -> Result<(), Error> {
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    match words[..] {
        ["login"]
        | ["login", "claude" | "chatgpt" | "console"]
        | ["devices"]
        | ["devices", "add" | "join" | "remove", _] => {}
        ["login", ..] => {
            return Err(Usage("Use razor login [claude|chatgpt|console].".into()));
        }
        ["devices", "add", _, "--key", _] => {}
        ["config", "get" | "unset", _] | ["config", "set", _, _] => {}
        ["config", ..] => return Err(Usage(VOICE_USE.into())),
        _ => return Err(Usage(api::DEVICES_USE.into())),
    }
    let (client, key) = api::connect(&home).await?;
    let text = match words[..] {
        ["login", ..] => {
            let provider = words.get(1).copied().unwrap_or("chatgpt");
            let provider = serde_json::from_value(serde_json::json!(provider))?;
            api::login(&client, &key, provider)
                .await
                .map_err(|e| Failed(e.to_string()))?;
            "The sign-in completed.".into()
        }
        ["devices", "add", name] => api::add_command(&client, &key, name).await?,
        ["devices", "join", name] => api::join_command(&client, &key, name).await?,
        ["devices", "add", name, _, given] => api::enroll(&client, &key, name, given).await?,
        ["devices", "remove", name] => api::revoke(&client, &key, name).await?,
        ["config", ..] => api::voice(&client, &key, &words[1..]).await?,
        _ => api::devices(&client, &key).await?,
    };
    println!("{text}");
    Ok(())
}

/// The session commands: the terminal app, which stages launch choices until the first input is
/// admitted, or with `--print` one turn, read to its end.
async fn session(home: PathBuf, args: Args) -> Result<(), Error> {
    let mut text = String::new();
    match args.message {
        Some(message) => text = message,
        None if args.print.is_some() => {
            let read = tokio::io::stdin().read_to_string(&mut text).await;
            read.map_err(|e| Usage(format!("Standard input is not UTF-8 text: {e}.")))?;
        }
        None => {}
    }
    if args.print.is_some() && text.trim().is_empty() {
        return Err(Usage("The message is empty.".into()));
    }
    let c = api::connection(&home).await?;
    let session = match args.continue_newest {
        true => Some(api::newest(&c).await?),
        false => args.resume,
    };
    let (model, effort, speed) = (args.model, args.effort, args.speed);
    let input = ClientInput {
        text,
        model,
        effort,
        speed,
        ..Default::default()
    };
    let Some(mode) = args.print else {
        return Ok(razor_harness::terminal::run(c, session, input, home, args.agents).await?);
    };
    let (session, end, answer) = api::turn(&c, session, &input, mode == Mode::Stream).await?;
    let sentence = match end {
        End::Done => {
            match mode {
                Mode::Text => println!("{answer}"),
                Mode::Json => println!("{}", json!({ "session": session, "message": answer })),
                Mode::Stream => {}
            }
            return Ok(());
        }
        End::Error(sentence) => sentence,
        End::Stopped => "The turn was stopped.".into(),
    };
    if mode == Mode::Json {
        println!("{}", json!({ "session": session, "error": sentence }));
    }
    Err(Failed(sentence))
}
