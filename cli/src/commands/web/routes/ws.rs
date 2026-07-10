use std::sync::Arc;
use std::time::Duration;

use rocket::{get, State};
use rocket_ws::{Channel, Message, WebSocket};

use crate::commands::web::action_bar::action_bar_oob_fragment;
use crate::commands::web::structs::AppConfig;
use crate::commands::web::units::{
    extract_unit_state_from_oob, unit_active_state_async, unit_controls_oob_fragment_with_state,
    unit_name_valid,
};

/// Parse a client WS control message.
/// Supported forms:
///   {"op":"watch","units":["docker-foo","bar"]}
///   {"op":"unwatch","units":[...]}
///   {"op":"watch_replace","units":[...]}  // drop previous interest, watch only these
/// Unknown / non-JSON messages are ignored (htmx may send form-shaped JSON).
fn parse_ws_unit_command(text: &str) -> Option<(String, Vec<String>)> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let op = v.get("op")?.as_str()?.to_string();
    if op != "watch" && op != "unwatch" && op != "watch_replace" {
        return None;
    }
    let units = v
        .get("units")?
        .as_array()?
        .iter()
        .filter_map(|u| u.as_str().map(|s| s.to_string()))
        .filter(|u| unit_name_valid(u))
        .collect::<Vec<_>>();
    Some((op, units))
}

/// WebSocket endpoint for htmx ws extension (hx-ext="ws" ws-connect="/ws/status").
///
/// - Forwards broadcast OOB fragments (action bar + unit control bursts from actions).
/// - Accepts client `watch` / `unwatch` / `watch_replace` messages listing systemd units;
///   while those units are watched *and this socket is open*, a per-connection poller
///   re-checks ActiveState (~500ms) and pushes OOB HTML only when it changes.
/// - Survives broadcast lag (skips) so a busy action-bar channel cannot kill the socket.
#[get("/ws/status")]
pub async fn ws_status(ws: WebSocket, config: &State<Arc<AppConfig>>) -> Channel<'static> {
    let mut rx = config.unit_updates.subscribe();
    let initial_bar = action_bar_oob_fragment(config);
    ws.channel(move |mut stream| {
        Box::pin(async move {
            use rocket::futures::{SinkExt, StreamExt};
            use std::collections::{HashMap, HashSet};

            // Immediate action-bar snapshot so the navbar is correct before the watcher ticks.
            if stream
                .send(Message::Text(initial_bar.into()))
                .await
                .is_err()
            {
                return Ok(());
            }

            let mut watched: HashSet<String> = HashSet::new();
            // unit -> last ActiveState string we pushed (skip identical re-renders)
            let mut last_state: HashMap<String, String> = HashMap::new();
            let mut tick = tokio::time::interval(Duration::from_millis(500));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // Don't fire immediately; first poll after 500ms (bootstrap GET already filled UI).
            tick.tick().await;

            loop {
                tokio::select! {
                    client_msg = stream.next() => {
                        match client_msg {
                            Some(Ok(Message::Text(text))) => {
                                if let Some((op, units)) = parse_ws_unit_command(&text) {
                                    match op.as_str() {
                                        "watch_replace" => {
                                            watched.clear();
                                            last_state.clear();
                                            for u in units {
                                                watched.insert(u);
                                            }
                                        }
                                        "watch" => {
                                            for u in units {
                                                watched.insert(u);
                                            }
                                        }
                                        "unwatch" => {
                                            for u in &units {
                                                watched.remove(u);
                                                last_state.remove(u);
                                            }
                                        }
                                        _ => {}
                                    }
                                    // Immediate snapshot for newly watched units so the pane
                                    // does not wait a full tick after open/reconnect.
                                    for u in watched.iter().cloned().collect::<Vec<_>>() {
                                        let active = unit_active_state_async(&u).await;
                                        let prev = last_state.get(&u);
                                        if prev.map(|p| p.as_str()) != Some(active.as_str()) {
                                            last_state.insert(u.clone(), active.clone());
                                            let frag =
                                                unit_controls_oob_fragment_with_state(&u, &active);
                                            if stream
                                                .send(Message::Text(frag.into()))
                                                .await
                                                .is_err()
                                            {
                                                return Ok(());
                                            }
                                        }
                                    }
                                }
                            }
                            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                            Some(Ok(_)) => { /* ping/pong/binary — ignore */ }
                        }
                    }
                    _ = tick.tick(), if !watched.is_empty() => {
                        // Live poll only for units this browser pane registered.
                        for u in watched.iter().cloned().collect::<Vec<_>>() {
                            let active = unit_active_state_async(&u).await;
                            let changed =
                                last_state.get(&u).map(|p| p.as_str()) != Some(active.as_str());
                            if changed {
                                last_state.insert(u.clone(), active.clone());
                                let frag = unit_controls_oob_fragment_with_state(&u, &active);
                                if stream.send(Message::Text(frag.into())).await.is_err() {
                                    return Ok(());
                                }
                            }
                        }
                    }
                    update = rx.recv() => {
                        match update {
                            Ok(fragment) => {
                                // Action-bar + burst unit updates from HTTP handlers.
                                // Keep last_state coherent so the poller does not re-send
                                // the same ActiveState right after a broadcast.
                                if let Some((unit, state)) = extract_unit_state_from_oob(&fragment)
                                {
                                    if watched.contains(&unit) {
                                        last_state.insert(unit, state);
                                    }
                                }
                                if stream.send(Message::Text(fragment.into())).await.is_err() {
                                    break;
                                }
                            }
                            // Lagged: drop missed messages and keep the socket alive.
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
            // Connection closed → watched set drops with this task (no more polling).
            Ok(())
        })
    })
}
