//! `GET /v1/status`: this server's agents, its own and its directly paired homes', each change as
//! it comes, with what the server says of itself.

use super::*;
use crate::protocol::AgentView;

/// The display label is shared by status, push and device enrollment.
pub(super) fn label(server: &config::Server) -> String {
    let label = [&server.label, &server.name]
        .into_iter()
        .find(|label| !label.is_empty())
        .cloned()
        .unwrap_or_else(profiles::machine);
    label.chars().take(64).collect()
}

/// Identity shared by layouts and status, without sampling load or the residents' calendars.
pub(super) fn identity(app: &App, config: &config::Config) -> Server {
    Server {
        id: app.id.to_string(),
        label: label(&config.server),
        machine: if config.server.name.is_empty() {
            profiles::machine()
        } else {
            config.server.name.clone()
        },
        realm: config.server.realm.clone(),
        residents: crate::residents::homes(config),
        ..Default::default()
    }
}

/// Local metadata is independent of labels; remote config never replaces local display choices.
async fn server(app: &App) -> Server {
    let config = app.controller.config().await;
    Server {
        transcribe: !config.transcribe.is_empty() && !config.provider_only,
        call: !config.provider_only
            && (config.live.url.is_some() || std::env::var_os("OPENAI_API_KEY").is_some()),
        shared: voice::owns(app, &config.server.shared),
        video: true,
        follow: true,
        suggestions: config.ui.suggestions,
        meetings: app.meetings.available,
        load: load::load().await,
        release_ready: crate::residents::release_ready().await,
        residents: app.residents.days().await,
        views_version: Some(app.views.views.borrow().version),
        silent: *app.asleep.borrow(),
        ..identity(app, &config)
    }
}

/// The live snapshot, local changes and optional paired server metadata.
pub(super) async fn status(
    State(app): State<App>,
    Extension(gone): Extension<Gone>,
    Query(q): Query<Page>,
) -> Response {
    let heartbeat = tokio::time::interval_at(tokio::time::Instant::now() + BEAT, BEAT);
    let sources = (
        app.controller.status(),
        app.deliveries.subscribe(),
        app.peers.subscribe(),
        heartbeat,
        app.views.published.subscribe(),
        app.views.views.subscribe(),
        app.asleep.clone(),
        app.views.retained.subscribe(),
        None,
        std::collections::BTreeSet::new(),
        app,
        q,
        true,
    );
    let lines = stream::unfold(sources, |mut sources| async move {
        let (
            local,
            deliveries,
            peers,
            beat,
            views,
            saved,
            asleep,
            retained,
            previous,
            sent,
            app,
            q,
            open,
        ) = &mut sources;
        loop {
            let mut line = if std::mem::take(open) {
                local.recv().await?
            } else {
                tokio::select! {
                    line = views::next(views, previous), if q.views => return Some((line?, sources)),
                    _ = saved.changed(), if q.heartbeat => Live::Server(server(app).await.into()),
                    Ok(()) = asleep.changed() => Live::Server(server(app).await.into()),
                    Ok(()) = retained.changed() => {
                        if sent.iter().eq(retained.borrow().iter().filter(|id| !q.local || id.address().is_none())) {
                            continue;
                        }
                        *local = app.controller.status();
                        local.recv().await?
                    },
                    line = local.recv() => line?,
                    d = deliveries.recv() => Live::Delivery(d.ok()?.0.into()),
                    peer = peers.recv(), if !q.local || q.servers => match peer.ok()? {
                        Live::Agent(_) | Live::Delivery(_) if q.local => continue,
                        Live::Server(_) if q.local && !q.servers => continue,
                        Live::Server(s) if s.name.is_empty() => { *local = app.controller.status(); continue; },
                        line => line,
                    },
                    _ = beat.tick(), if q.heartbeat => Live::Server(server(app).await.into()),
                }
            };
            let visible = |a: &AgentView| {
                let retained = retained.borrow();
                retained.contains(&a.session)
                    || views::live(a, crate::protocol::now())
                        && a.resident.is_none()
                        && a.parent.as_ref().is_none_or(|p| retained.contains(p))
            };
            if let Live::Agents {
                server: home,
                servers,
                agents,
                ..
            } = &mut line
            {
                **home = server(app).await;
                *servers = if q.local && !q.servers {
                    vec![*home.clone()]
                } else {
                    app.peers.extend(agents, *home.clone()).await
                };
                agents.retain(|a| visible(a) && (!q.local || a.session.address().is_none()));
            }
            if let Live::Agent(row) = &line {
                if !sent.contains(&row.session) && !visible(row) {
                    continue;
                }
                sent.insert(row.session.clone());
            }
            if !q.local {
                let rows = match &mut line {
                    Live::Agents { agents, .. } => agents.as_mut_slice(),
                    Live::Agent(row) => std::slice::from_mut(&mut **row),
                    _ => &mut [],
                };
                app.controller.parent_rows(rows);
            }
            if let Live::Agents { agents, .. } = &line {
                *sent = agents.iter().map(|a| a.session.clone()).collect();
            }
            return Some((line_of(&line)?, sources));
        }
    });
    ndjson(lines, gone)
}
