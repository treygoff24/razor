//! Codex's `web.run` on the ChatGPT sign-in (docs/design/responses.md): one search request per
//! call, its `output` text the call's result.

use super::{Error, Responses, bounded, other};
use crate::protocol::SessionId;
use serde_json::{Value, json};

/// The most bytes a search response may hold.
const RESPONSE: usize = 4 << 20;

impl Responses {
    /// One `web.run` call: POSTs Codex's search request, in Codex's default cached mode, to
    /// `{base}/alpha/search` with the sign-in's bearer and account id, refreshing the sign-in
    /// once on 401, and returns the response's `output`. `commands` is sent as the model wrote
    /// it, and the backend refuses what it cannot run. Code mode, and so this call, is never on
    /// the public API route.
    pub(super) async fn search(
        &self,
        base: &str,
        session: &SessionId,
        model: &str,
        input: Vec<Value>,
        commands: Value,
    ) -> Result<String, Error> {
        let url = format!("{}/alpha/search", base.trim_end_matches('/'));
        let commands = if commands.is_null() {
            json!({})
        } else {
            commands
        };
        let body = json!({"id": session.as_str(), "model": model, "input": input,
            "commands": commands, "max_output_tokens": 10_000,
            "settings": {"allowed_callers": ["direct"], "external_web_access": false}});
        let request = || self.client.post(&url).json(&body);
        let (response, _) = self
            .signed(session, request, "sending a web search")
            .await?;
        // The status alone, never the endpoint's message: one may echo the request's bearer.
        if !response.status().is_success() {
            let (status, message) = (response.status().as_u16(), "the web search failed".into());
            return Err(match status {
                401 => Error::Auth,
                _ => Error::Rejected { status, message },
            });
        }
        let found: Value = serde_json::from_slice(&bounded(response, RESPONSE).await?)
            .map_err(other("reading a web search's answer"))?;
        found["output"]
            .as_str()
            .map(str::to_owned)
            .ok_or(Error::Incomplete)
    }
}
