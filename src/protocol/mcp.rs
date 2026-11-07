//! MCP servers as settings files and the API carry them: one server's `[mcp.<name>]` definition,
//! the operations on those files, and the rows `/mcp` lists.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
    path::PathBuf,
};

/// A whole named server: exactly one command or HTTP URL, which only a disabled entry may omit.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Definition")]
pub(crate) struct Mcp {
    pub(crate) enabled: bool,
    pub(crate) search: Option<bool>,
    pub(crate) timeout: Option<NonZeroU64>,
    pub(crate) tools: Option<BTreeSet<String>>,
    #[serde(flatten)]
    pub(crate) connection: Option<Connection>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    #[serde(default = "enabled")]
    enabled: bool,
    search: Option<bool>,
    timeout: Option<NonZeroU64>,
    tools: Option<BTreeSet<String>>,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, Reference>,
    oauth: Option<OAuth>,
}

fn enabled() -> bool {
    true
}

impl TryFrom<Definition> for Mcp {
    type Error = &'static str;
    fn try_from(d: Definition) -> Result<Self, Self::Error> {
        let web = |url: &str| {
            let url = reqwest::Url::parse(url);
            url.is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
        };
        let connection = match (d.command, d.url) {
            (Some(command), None) if !command.trim().is_empty() => {
                let (args, env) = (d.args, d.env);
                Some(Connection::Stdio { command, args, env })
            }
            (None, Some(url)) if web(&url) => {
                let (headers, oauth) = (d.headers, d.oauth);
                Some(Connection::Http {
                    url,
                    headers,
                    oauth,
                })
            }
            (None, None) if !d.enabled => None,
            _ => {
                return Err("An MCP server needs exactly one command or HTTP URL unless disabled.");
            }
        };
        if let Some(Connection::Http { oauth: Some(o), .. }) = &connection
            && o.client_secret.is_some()
            && (o.client_id.is_none() || o.issuer.is_none())
        {
            return Err("An MCP client secret needs its client ID and expected issuer.");
        }
        let (enabled, search, tools) = (d.enabled, d.search, d.tools);
        Ok(Self {
            enabled,
            search,
            timeout: d.timeout,
            tools,
            connection,
        })
    }
}

/// Stdio keeps literal environment values; HTTP credentials are references resolved at use.
#[derive(Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum Connection {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
    Http {
        url: String,
        headers: BTreeMap<String, Reference>,
        oauth: Option<OAuth>,
    },
}

/// A credential's location, never its resolved bytes in configuration or records.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Reference {
    Env(String),
    File(PathBuf),
}

/// Expected authorization server and an optional preregistered client.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OAuth {
    pub(crate) issuer: Option<String>,
    pub(crate) client_id: Option<String>,
    pub(crate) client_secret: Option<Reference>,
}

/// Selects a writable settings file; the deployment pack is never a destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum McpScope {
    Global,
    Project,
    Private,
}

/// File changes are immediate; importing only previews additions and unresolved credentials.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(crate) enum McpOperation {
    List,
    Add {
        name: String,
        scope: McpScope,
        config: Mcp,
    },
    Remove {
        name: String,
        scope: McpScope,
    },
    Enable {
        name: String,
        scope: McpScope,
    },
    Disable {
        name: String,
        scope: McpScope,
    },
    Reconnect {
        name: String,
    },
    SignIn {
        name: String,
    },
    SignOut {
        name: String,
    },
    Import {
        source: PathBuf,
        scope: McpScope,
    },
}

/// Reports references, never credential bytes; connection state belongs to the session.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct McpRow {
    pub(crate) name: String,
    pub(crate) file: Option<PathBuf>,
    /// The writable file `file` is, which enable, disable and remove name; none for a pack's.
    pub(crate) scope: Option<McpScope>,
    pub(crate) config: Mcp,
    pub(crate) state: String,
    /// The session's tools from this server after its allow list: each one's own name and its
    /// description's first line, cut to 120 characters. Empty without a session or connection.
    pub(crate) tools: Vec<(String, String)>,
    pub(crate) signed_in: bool,
    pub(crate) error: Option<String>,
}
