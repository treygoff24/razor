//! Local settings commands share the loader's validation and the MCP editor's atomic writer.

use super::{Error, Settings, pack};
use crate::protocol::McpScope;
use std::path::Path;
use toml_edit::{DocumentMut, Item, Key, Table, Value};

/// Read or edit this home's settings in the current project without starting a server.
/// `None` means invalid command syntax, before any settings file is read or written.
pub async fn run(home: &Path, words: &[String]) -> Result<Option<String>, Error> {
    let (mut flags, mut scope, mut args) = (0, McpScope::Global, Vec::new());
    for word in words {
        match word.as_str() {
            "--project" => {
                flags += 1;
                scope = McpScope::Project;
            }
            "--local" => {
                flags += 1;
                scope = McpScope::Private;
            }
            _ => args.push(word.clone()),
        }
    }
    let valid = match args.first().map(String::as_str) {
        Some("get") => args.len() == 2 && flags == 0,
        Some("set") => args.len() == 3,
        Some("unset") => args.len() == 2,
        Some("path") => args.len() == 1,
        _ => false,
    };
    if flags > 1 || !valid {
        return Ok(None);
    }
    execute(home, &args, scope).map(Some)
}

fn execute(home: &Path, args: &[String], scope: McpScope) -> Result<String, Error> {
    let cwd = std::env::current_dir().map_err(|source| Error::Other {
        source,
        context: "changing settings",
    })?;
    let settings = Settings::for_edit(home.to_path_buf())?.project(&cwd)?;
    if matches!(args[0].as_str(), "get" | "path")
        && let Some(error) = settings.errors.first()
    {
        return Err(Error::Invalid(error.clone()));
    }
    if args[0] == "path" {
        return Ok(settings.destination(scope)?.0.display().to_string());
    }
    let keys = Key::parse(&args[1]).map_err(|e| {
        Error::Invalid(format!(
            "{} is not a dotted settings key: {e}; use a key from configuration.md.",
            args[1]
        ))
    })?;
    let keys: Vec<_> = keys.iter().map(Key::get).collect();
    if args[0] == "get" {
        return get(&settings, &keys, &args[1]);
    }
    edit(&settings, args, scope, &keys)
}

fn edit(
    settings: &Settings,
    args: &[String],
    scope: McpScope,
    keys: &[&str],
) -> Result<String, Error> {
    let (path, project) = settings.destination(scope)?;
    let before = pack::text(&path, true)?;
    let mut document: DocumentMut = (before.parse())
        .map_err(|e: toml_edit::TomlError| pack::unparsed(&path, &before, e.span(), e.message()))?;
    if args[0] == "set" {
        if keys.len() > 1
            && keys[0] == "models"
            && let Some(models) = document.get("models").and_then(Item::as_table_like)
            && let Some((existing, _)) = models.iter().find(|(name, _)| {
                *name != keys[1]
                    && crate::profiles::id((*name).to_string())
                        == crate::profiles::id(keys[1].to_string())
            })
        {
            return Err(Error::Invalid(format!(
                "Model {} is already written as {existing}; use that spelling.",
                keys[1]
            )));
        }
        // Each second-level value replaces its inherited entry whole. Copy it only when
        // absent here; an entry already in the destination keeps its comments and spelling.
        if keys.len() > 2
            && document
                .get(keys[0])
                .and_then(|item| item.get(keys[1]))
                .is_none()
            && let Some(value) = settings.inherited(scope, keys)?
        {
            let entry = table(document.as_item_mut(), &keys[..1])?.entry(keys[1]);
            entry.or_insert(value);
        }
        let value = args[2]
            .parse::<Value>()
            .unwrap_or_else(|_| Value::from(args[2].as_str()));
        let parent = table(document.as_item_mut(), &keys[..keys.len() - 1])?;
        let key = keys[keys.len() - 1];
        let mut value = Item::Value(value);
        if let Some(old) = parent.get(key).and_then(Item::as_value)
            && let Some(new) = value.as_value_mut()
        {
            *new.decor_mut() = old.decor().clone();
        }
        parent.insert(key, value);
    } else if !remove(document.as_item_mut(), keys) {
        let (path, key) = (path.display(), &args[1]);
        return Ok(format!("{path} has no override for {key}."));
    }
    let after = document.to_string();
    settings.check_edit(&path, &after, project)?;
    if !project && let Some(home) = path.parent() {
        super::make_home(home)?;
    }
    pack::replace(&path, &before, &after)?;
    if project && scope == McpScope::Private && before.is_empty() {
        let _ = pack::exclude_private(&path);
    }
    Ok(format!("Saved {} in {}.", args[1], path.display()))
}

fn get(settings: &Settings, keys: &[&str], shown: &str) -> Result<String, Error> {
    let config =
        serde_json::to_value(&settings.config).map_err(|e| Error::Invalid(e.to_string()))?;
    // Model aliases are normalized by Config, while file provenance keeps their spelling.
    let model = (keys.first() == Some(&"models") && keys.len() > 1)
        .then(|| crate::profiles::id(keys[1].to_string()));
    let value = keys
        .iter()
        .enumerate()
        .try_fold(&config, |value, (index, key)| {
            value.get(if index == 1 {
                model.as_deref().unwrap_or(key)
            } else {
                key
            })
        })
        .ok_or_else(|| {
            Error::Invalid(format!(
                "{shown} has no configured value; see configuration.md for settings."
            ))
        })?;
    let mut lines = vec![serde_json::to_string(value).map_err(|e| Error::Invalid(e.to_string()))?];
    let mut source_keys = keys.to_vec();
    if let Some(model) = &model
        && let Some(names) = settings
            .values
            .get("models")
            .and_then(toml::Value::as_table)
        && let Some(name) = names
            .keys()
            .find(|name| crate::profiles::id((*name).clone()) == *model)
    {
        source_keys[1] = name;
    }
    let prefix = source_keys[..source_keys.len().min(2)].join(".");
    let sources: Vec<_> = settings
        .sources
        .iter()
        .filter(|(key, _)| {
            *key == &prefix || keys.len() == 1 && key.starts_with(&format!("{prefix}."))
        })
        .collect();
    if sources.is_empty() {
        lines.push("source: built-in default".into());
    } else {
        lines.extend(
            sources
                .into_iter()
                .map(|(key, file)| format!("source: {} ({key})", file.display())),
        );
    }
    Ok(lines.join("\n"))
}

fn table<'a>(item: &'a mut Item, keys: &[&str]) -> Result<&'a mut dyn toml_edit::TableLike, Error> {
    if item.is_none() {
        let mut new = Table::new();
        new.set_implicit(true);
        *item = Item::Table(new);
    }
    let current = item.as_table_like_mut().ok_or_else(|| {
        Error::Invalid(
            "The settings key crosses a value; use a dotted key through tables only.".into(),
        )
    })?;
    match keys.split_first() {
        Some((key, rest)) => table(current.entry(key).or_insert(Item::None), rest),
        None => Ok(current),
    }
}

fn remove(item: &mut Item, keys: &[&str]) -> bool {
    let Some(current) = item.as_table_like_mut() else {
        return false;
    };
    let Some((key, rest)) = keys.split_first() else {
        return false;
    };
    if rest.is_empty() {
        return current.remove(key).is_some();
    }
    let Some(child) = current.get_mut(key) else {
        return false;
    };
    remove(child, rest)
}
