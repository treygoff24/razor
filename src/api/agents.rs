//! On-demand Earlier roots, newest first; reads never add rows to the live feed.
use super::*;
use crate::protocol::{AgentPage, Body as EntryBody, Child, views::SectionKind};

#[derive(Deserialize)]
pub(super) struct QueryPage {
    view: Option<String>,
    after: Option<SessionId>,
}
/// At most 50 Earlier roots and 1 MiB; a filtered empty page still advances next.
pub(super) async fn page(State(app): State<App>, Query(q): Query<QueryPage>) -> Reply<AgentPage> {
    let mut saved = app
        .views
        .definition
        .borrow()
        .as_deref()
        .cloned()
        .ok_or_else(|| bad("The server that keeps your views is unavailable; reconnect to it."))?;
    if let Some(view) = q.view {
        if !saved.views.is_empty() && !saved.views.iter().any(|v| v.name == view) {
            return Err(bad("That view no longer exists; open Earlier again."));
        }
        saved.views.retain(|v| v.name == view);
    }
    let mut before = q.after;
    let config = app.controller.config().await;
    let server = status::identity(&app, &config);
    let unread = app
        .deliveries
        .list(None, 256, &[])
        .await?
        .into_iter()
        .filter(|d| !d.read && !d.archived)
        .map(|d| d.session)
        .collect();
    let mut ids = crate::record::stored(&app.home)
        .await
        .map_err(crate::controller::Error::from)?;
    ids.retain(|id| before.as_ref().is_none_or(|b| id < b));
    ids.sort_unstable_by(|a, b| b.cmp(a));
    let (mut exhausted, mut used) = (ids.len() <= 256, 0);
    let mut page = AgentPage {
        agents: Vec::new(),
        next: None,
    };
    let (mut parents, mut settled) = (BTreeMap::new(), BTreeMap::new());
    for id in ids.into_iter().take(256) {
        if page.agents.len() == 50 {
            exhausted = false;
            break;
        }
        if let Some(EntryBody::Start { child, .. }) = crate::record::start(&app.home, &id).await
            && promoted(&app, child.as_ref(), &mut parents, &mut settled).await?
        {
            let row = app.controller.agent(&id).await?;
            let layouts = super::views::resolve(
                &saved,
                std::slice::from_ref(&row),
                std::slice::from_ref(&server),
                &unread,
                crate::protocol::now(),
            );
            let earlier = layouts
                .iter()
                .flat_map(|l| &l.sections)
                .any(|s| s.kind == SectionKind::Earlier && !s.sessions.is_empty());
            if earlier {
                let size = serde_json::to_vec(&row)
                    .map_err(other("Encoding archived agent"))?
                    .len()
                    + 2;
                if size > (1 << 20) - 4096 {
                    return Err(bad("An archived agent exceeds the 1 MiB page limit."));
                }
                if used + size > (1 << 20) - 4096 {
                    exhausted = false;
                    break;
                }
                used += size;
                page.agents.push(row);
            }
        }
        before = Some(id);
    }
    if !exhausted {
        page.next = before;
    }
    Ok(Json(page))
}

/// A settled helper's surviving children become roots only here; each ancestor is read once.
async fn promoted(
    app: &App,
    child: Option<&Child>,
    parents: &mut BTreeMap<SessionId, Option<Child>>,
    settled: &mut BTreeMap<SessionId, std::collections::BTreeSet<SessionId>>,
) -> Result<bool, Refusal> {
    let Some(child) = child else {
        return Ok(true);
    };
    if child.parent.address().is_some() {
        return Ok(false);
    }
    if !parents.contains_key(&child.parent) {
        parents.insert(
            child.parent.clone(),
            match crate::record::start(&app.home, &child.parent).await {
                Some(EntryBody::Start { child, .. }) => child,
                _ => None,
            },
        );
    }
    let Some(parent) = &parents[&child.parent] else {
        return Ok(false);
    };
    if parent.parent.address().is_some() {
        return Ok(false);
    }
    if !settled.contains_key(&parent.parent) {
        let summary = app.controller.summary(&parent.parent).await?;
        settled.insert(
            parent.parent.clone(),
            summary
                .children
                .into_iter()
                .filter(|c| c.settled.is_some())
                .map(|c| c.session)
                .collect(),
        );
    }
    Ok(settled[&parent.parent].contains(&child.parent))
}
