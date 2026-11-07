//! Compiled terminal bindings; defaults, descriptions and contexts live in data/keys.toml.
use super::{
    App, Error,
    composer::{completion, shortcodes},
    keys::replied,
    text::*,
};
use crossterm::event::{KeyCode as K, KeyEvent, KeyModifiers as M};
use ratatui::text::Line;
use std::{
    collections::{BTreeMap, HashMap},
    sync::LazyLock,
};
type Keys = BTreeMap<String, BTreeMap<String, Vec<String>>>;
#[derive(serde::Deserialize)]
struct Catalogue {
    keys: Keys,
    descriptions: BTreeMap<String, String>,
}
static CATALOGUE: LazyLock<Result<Catalogue, toml::de::Error>> =
    LazyLock::new(|| toml::from_str(include_str!("data/keys.toml")));
type Map = HashMap<(&'static str, KeyEvent), (&'static str, KeyEvent)>;
#[derive(Default)]
pub struct Bindings {
    maps: Map,
    keys: BTreeMap<String, Vec<String>>,
    prefix: bool,
}

fn chord(s: &str) -> Option<KeyEvent> {
    let (mut s, mut mods) = (s, M::NONE);
    while let Some((head, tail)) = s.split_once('-') {
        mods |= match head.to_ascii_lowercase().as_str() {
            "ctrl" => M::CONTROL,
            "alt" => M::ALT,
            "shift" => M::SHIFT,
            "super" => M::SUPER,
            _ => break,
        };
        s = tail;
    }
    let code = match s.to_ascii_lowercase().as_str() {
        "enter" => K::Enter,
        "esc" | "escape" => K::Esc,
        "space" => K::Char(' '),
        "tab" => K::Tab,
        "backtab" => K::BackTab,
        "backspace" => K::Backspace,
        "delete" => K::Delete,
        "insert" => K::Insert,
        "home" => K::Home,
        "end" => K::End,
        "up" => K::Up,
        "down" => K::Down,
        "left" => K::Left,
        "right" => K::Right,
        "pageup" => K::PageUp,
        "pagedown" => K::PageDown,
        f if f.starts_with('f') && f.len() > 1 => {
            K::F(f[1..].parse().ok().filter(|n| (1..=24).contains(n))?)
        }
        _ => {
            let mut chars = s.chars();
            let c = chars.next()?;
            if chars.next().is_some() || c.is_control() {
                return None;
            }
            K::Char(if mods.contains(M::SHIFT) {
                c.to_ascii_uppercase()
            } else if !mods.is_empty() {
                c.to_ascii_lowercase()
            } else {
                c
            })
        }
    };
    Some(KeyEvent::new(code, mods))
}

fn normalized(mut k: KeyEvent) -> KeyEvent {
    if let K::Char(c) = k.code {
        if k.modifiers.contains(M::SHIFT) {
            k.code = K::Char(c.to_ascii_uppercase());
        }
        if c != ' ' {
            k.modifiers.remove(M::SHIFT);
        }
    }
    if k.code == K::Tab && k.modifiers.contains(M::SHIFT) {
        k.code = K::BackTab;
    }
    if k.code == K::BackTab {
        k.modifiers.remove(M::SHIFT);
    }
    KeyEvent::new(k.code, k.modifiers)
}

/// Rejects invalid actions, chords and collisions before replacing any bindings. Under the prefix
/// an entry may name any other action in full, `"global.new"`, which then runs as its own key does.
pub(super) fn compile(overrides: &Keys) -> Result<Bindings, Error> {
    let data = CATALOGUE.as_ref().map_err(super::fail(
        "The default key catalogue is invalid; reinstall Razor.",
    ))?;
    let unknown = |context: &str| {
        super::error(format!(
            "Unknown key action in {context}; use /keys for the available actions."
        ))
    };
    if let Some(context) = overrides.keys().find(|c| !data.keys.contains_key(*c)) {
        return Err(unknown(context));
    }
    let mut bindings = Bindings::default();
    for (context, defaults) in &data.keys {
        let mine = overrides.get(context);
        let added = mine
            .into_iter()
            .flatten()
            .filter(|(n, _)| !defaults.contains_key(*n));
        for (name, keys) in defaults.iter().chain(added) {
            let keys = mine.and_then(|m| m.get(name)).unwrap_or(keys);
            // The action's full name and its first default key, by which the handlers know it.
            let named = match name.contains('.') && context == "prefix" {
                true => name.clone(),
                false => format!("{context}.{name}"),
            };
            let (scope, action) = named.split_once('.').ok_or_else(|| unknown(context))?;
            let first = (data.keys.get(scope).and_then(|a| a.get(action))).map(|d| d.first());
            let (full, _) = (data.descriptions.get_key_value(&named))
                .filter(|_| first.is_some())
                .ok_or_else(|| unknown(context))?;
            let native = first
                .flatten()
                .and_then(|s| chord(s))
                .unwrap_or(K::Null.into());
            if keys.len() > 8 {
                return Err(super::error(format!(
                    "Too many keys for {full}; keep at most eight."
                )));
            }
            for s in keys {
                let key = chord(s).ok_or_else(|| super::error(format!("Invalid key {s} for {full}; use modifiers and a named key or one character.")))?;
                if let Some((other, _)) = bindings
                    .maps
                    .insert((context.as_str(), normalized(key)), (full, native))
                    && other != full
                {
                    return Err(super::error(format!(
                        "Key {s} is assigned twice in {context}; choose distinct keys."
                    )));
                }
            }
            bindings
                .keys
                .insert(format!("{context}.{name}"), keys.clone());
        }
    }
    Ok(bindings)
}

/// Keeps the last valid table when a live settings update contains an invalid binding.
pub fn reload(app: &mut App) {
    match compile(&app.ui.keys) {
        Ok(b) => app.bindings = b,
        Err(e) => app.say(&e.to_string()),
    }
}

/// All effective chords for one qualified action, including an explicit unbound label.
pub fn hint(app: &App, action: &str) -> String {
    match app.bindings.keys.get(action) {
        Some(keys) if !keys.is_empty() => keys.join("/"),
        _ => "unbound".into(),
    }
}

fn keyed(app: &App, action: &str) -> bool {
    (app.bindings.keys.get(action)).is_some_and(|keys| !keys.is_empty())
}

/// Whether each action a hint names in angle brackets, or one of each `|` choice, has a key.
pub fn bound(app: &App, template: &str) -> bool {
    let mut named = (template.split('<').skip(1)).filter_map(|part| part.split_once('>'));
    named.all(|(choice, _)| choice.split('|').any(|action| keyed(app, action)))
}

// Of a `|` choice of actions, the first bound one, else the first.
fn pick<'a>(app: &App, choice: &'a str) -> Option<&'a str> {
    let action = choice.split('|').find(|a| keyed(app, a));
    let action = action.or(choice.split('|').next());
    action.filter(|a| app.bindings.keys.contains_key(*a))
}

/// `text` of a hint's parts split by `sep`, leaving out each part that names an unbound action.
pub fn hints(app: &App, template: &str, sep: &str) -> String {
    let parts = template.split(sep).filter(|part| bound(app, part));
    text(app, &parts.collect::<Vec<_>>().join(sep))
}

/// Expands action names in angle brackets in a visible hint; of a `|` choice, the first bound one.
pub fn text(app: &App, template: &str) -> String {
    template
        .split('<')
        .enumerate()
        .map(|(i, part)| match part.split_once('>') {
            Some((choice, tail)) if let Some(action) = pick(app, choice) => {
                hint(app, action) + tail
            }
            _ if i > 0 => format!("<{part}"),
            _ => part.into(),
        })
        .collect()
}

/// Lists every action, context, description and effective chord, a row each.
pub fn rows(app: &App) -> Vec<Line<'static>> {
    let Ok(data) = CATALOGUE.as_ref() else {
        return Vec::new();
    };
    let row = |action: &String| {
        let named = action.strip_prefix("prefix.").filter(|a| a.contains('.'));
        let about = &data.descriptions[named.unwrap_or(action)];
        span(format!("{action}: {} · {about}", hint(app, action)), DATA).into()
    };
    app.bindings.keys.keys().map(row).collect()
}

fn available(app: &App, context: &str, action: &str, k: KeyEvent) -> bool {
    let modal = app.panel.is_some() || app.picker.is_some() || app.start.is_some();
    let modal = modal || app.grid.is_some() || app.goto.is_some();
    let empty = app.composer.text.is_empty();
    match context {
        // With words or images in the draft, the clear key empties it rather than arming quit;
        // the overview, which shows no draft, leaves it to quit.
        "clear" => {
            let shown = !modal && app.list.overview.is_none();
            action == "composer.clear" && !app.composer.is_empty() && shown
        }
        "early" => action == "global.record" || action == "global.quit" && app.grid.is_none(),
        "voice" => super::audio::captures(app, k),
        "goto" => app.goto.is_some(),
        "mcp" => super::mcp::captures(app, k),
        "panel" => app.panel.is_some(),
        "picker" => app.picker.is_some() && app.panel.is_none(),
        "new" => action == "global.new" && app.panel.is_none() && app.picker.is_none(),
        "start" => app.start.is_some() && app.panel.is_none() && app.picker.is_none(),
        "open" => action == "list.open" && !modal,
        "list" => {
            let shown = app.list.open || app.list.overview.is_some();
            action != "list.open" && shown && app.list.keys && !modal
        }
        "emoji" => {
            app.grid.is_some() && app.panel.is_none() && app.picker.is_none() && app.start.is_none()
        }
        "transcript" if app.picker.is_some() => app.panel.is_none(),
        _ if modal => false,
        // While the draft holds words, the prefix and half-page keys are the editor's.
        _ if action == "global.prefix" || action.starts_with("transcript.half") => empty,
        "global" | "transcript" | "skill" | "composer" | "editor" => action != "composer.clear",
        "attachments" => app.composer.chosen.is_some(),
        "completion" => completion::captures(app),
        "shortcode" => shortcodes::typed(app).is_some(),
        "suggestion" => app.suggestion().is_some(),
        "children" => empty,
        // Ahead of browsing, the switch takes its key unless browsing has started.
        "switch" => action == "composer.switch" && app.selected.is_none(),
        // Tab and Shift-Tab start browsing with an empty draft; the other keys browse once it has.
        // Enter reads only a reply; on any other target it opens it, as a click does.
        "targets" => match k.code {
            K::Tab | K::BackTab => empty,
            _ if action == "targets.read" => {
                app.selected.as_ref().is_some_and(|a| replied(a).is_some())
            }
            _ => app.selected.is_some(),
        },
        "queued_input" => {
            empty
                && app
                    .transcript()
                    .is_some_and(|t| t.queued.difference(&t.taking).next().is_some())
        }
        // A child's split view draws the band with its keys off: they go to the split's own input.
        "question" => {
            empty
                && app.answering.is_none()
                && !app.split_view()
                && app.shown_ask.as_ref().is_some_and(|q| match k.code {
                    K::Char(c) => q.options.len() > c as usize - '1' as usize,
                    // Left and Right step between questions only while several wait.
                    _ => super::questions::waiting(app) > 1,
                })
        }
        // With words selected the arrows are the editor's, which collapses the selection.
        "line_start" | "draft_end" if app.composer.selection().is_some() => false,
        "line_start" => {
            app.composer.cursor() == 0 || app.composer.text[..app.composer.cursor()].ends_with('\n')
        }
        "draft_end" => app.list.open && app.composer.cursor() == app.composer.text.len(),
        "attached_images" => {
            app.composer.vertical_edge(false) && app.composer.active_images().next().is_some()
        }
        "input_history" => app.composer.vertical_edge(k.code == K::Down),
        _ => false,
    }
}

/// The contexts in precedence order, each looking its keys up in its scope's table, named after a
/// colon where it is not the context's own.
const CONTEXTS: &str = "clear:composer early:global voice goto mcp panel picker new:global start open:list list emoji global transcript skill attachments completion shortcode suggestion switch:composer targets children queued_input question line_start draft_end attached_images input_history composer editor";

fn contexts() -> impl Iterator<Item = (&'static str, &'static str)> {
    (CONTEXTS.split_whitespace()).map(|c| c.split_once(':').unwrap_or((c, c)))
}

/// Resolves one physical key once, preserving modal precedence and ordinary printable input.
pub fn translate(app: &mut App, key: KeyEvent) -> Option<(KeyEvent, &'static str, &'static str)> {
    let k = normalized(key);
    let text = matches!(key.code, K::Char(_))
        && !(key.modifiers).intersects(M::CONTROL | M::ALT | M::SUPER | M::META);
    // A panel being searched takes typed characters, its own keys' included.
    if text && app.panel.is_some() && app.panel_search.is_some() {
        return Some((key, "panel", ""));
    }
    // So does Go to's search line.
    if text && app.goto.as_ref().is_some_and(|g| g.typing) {
        return Some((key, "text", ""));
    }
    // Armed, with the draft still empty, a prefix action's key is that action where its own
    // context would take it, and the prefix again is the key itself.
    let armed = std::mem::take(&mut app.bindings.prefix) && app.composer.text.is_empty();
    let maps = &app.bindings.maps;
    if armed && let Some((action, native)) = maps.get(&("prefix", k)).copied() {
        let scope = action
            .split_once('.')
            .map_or("prefix", |(context, _)| context);
        let mut own = contexts().filter(|(_, s)| *s == scope);
        if scope == "prefix" || own.any(|(c, _)| available(app, c, action, native)) {
            return Some((native, scope, action));
        }
    }
    let again = (maps.get(&("global", k))).is_some_and(|(a, _)| *a == "global.prefix");
    if armed && !again {
        app.say("That prefix action is unbound or unavailable here; /keys lists the actions.");
        return None;
    }
    for (context, scope) in contexts() {
        if let Some((action, native)) = app.bindings.maps.get(&(scope, k)).copied()
            && available(app, context, action, native)
        {
            if action != "global.prefix" {
                return Some((native, scope, action));
            }
            if !armed {
                app.bindings.prefix = true;
                return None;
            }
        }
    }
    text.then_some((key, "text", ""))
}
