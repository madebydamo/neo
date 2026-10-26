use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use rocket::{get, State};
use rocket_ws::{Channel, Message, WebSocket};

use crate::commands::web::action_bar::action_bar_oob_fragment;
use crate::commands::web::locks::locks_oob_element;
use crate::commands::web::types::AppConfig;
use crate::commands::web::units::{
    extract_unit_state_from_oob, is_pull_in_flight, query_unit_states,
    unit_controls_oob_fragment_with_state, unit_name_valid, unit_state_key,
};

/// Per-connection unit watch: which units the pane shows and what was last pushed.
#[derive(Default)]
struct UnitWatch {
    watched: HashSet<String>,
    /// unit -> (state key, pull in flight) last pushed (skip identical re-renders).
    last: HashMap<String, (String, bool)>,
}

impl UnitWatch {
    fn apply(&mut self, op: &str, units: Vec<String>) {
        match op {
            "watch_replace" => {
                self.watched = units.into_iter().collect();
                self.last.clear();
            }
            "watch" => self.watched.extend(units),
            "unwatch" => {
                for u in &units {
                    self.watched.remove(u);
                    self.last.remove(u);
                }
            }
            _ => {}
        }
    }

    /// Control fragments for watched units whose state changed since the last push.
    /// One batched `systemctl show` (uncached: the pane wants ~500ms freshness).
    async fn changed_fragments(&mut self, config: &AppConfig) -> Vec<String> {
        let mut units: Vec<String> = self.watched.iter().cloned().collect();
        units.sort();
        let mut states: Vec<_> = query_unit_states(&units).await.into_iter().collect();
        states.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out = Vec::new();
        for (u, st) in states {
            let now = (unit_state_key(&st), is_pull_in_flight(config, &u));
            if self.last.get(&u) != Some(&now) {
                out.push(unit_controls_oob_fragment_with_state(&u, &st, now.1));
                self.last.insert(u, now);
            }
        }
        out
    }

    /// Keep `last` coherent with a broadcast fragment so the poller does not re-send it.
    fn note_broadcast(&mut self, config: &AppConfig, fragment: &str) {
        if let Some((unit, state)) = extract_unit_state_from_oob(fragment) {
            if self.watched.contains(&unit) {
                let pulling = is_pull_in_flight(config, &unit);
                self.last.insert(unit, (state, pulling));
            }
        }
    }
}

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
    let config = Arc::clone(config);
    let mut rx = config.unit_updates.subscribe();
    let initial_bar = action_bar_oob_fragment(&config);
    ws.channel(move |mut stream| {
        Box::pin(async move {
            use rocket::futures::{SinkExt, StreamExt};

            // Immediate action-bar snapshot so the navbar is correct before the watcher ticks.
            if stream.send(Message::Text(initial_bar)).await.is_err() {
                return Ok(());
            }
            // Lock holders too: changes made while this client was away are not re-sent.
            if stream
                .send(Message::Text(locks_oob_element().await))
                .await
                .is_err()
            {
                return Ok(());
            }

            let mut watch = UnitWatch::default();
            let mut tick = tokio::time::interval(Duration::from_millis(500));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // Don't fire immediately; first poll after 500ms (watch handlers push a snapshot).
            tick.tick().await;

            loop {
                let fragments = tokio::select! {
                    client_msg = stream.next() => {
                        match client_msg {
                            Some(Ok(Message::Text(text))) => {
                                let Some((op, units)) = parse_ws_unit_command(&text) else {
                                    continue;
                                };
                                watch.apply(&op, units);
                                // Immediate snapshot for newly watched units so the pane
                                // does not wait a full tick after open/reconnect.
                                watch.changed_fragments(&config).await
                            }
                            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                            Some(Ok(_)) => continue, // ping/pong/binary — ignore
                        }
                    }
                    // Live poll only for units this browser pane registered.
                    _ = tick.tick(), if !watch.watched.is_empty() => {
                        watch.changed_fragments(&config).await
                    }
                    update = rx.recv() => {
                        match update {
                            Ok(fragment) => {
                                // Action-bar + burst unit updates + pull progress from HTTP handlers.
                                watch.note_broadcast(&config, &fragment);
                                vec![fragment]
                            }
                            // Lagged: drop missed messages, keep the socket alive and
                            // resend the state they may have carried.
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                vec![action_bar_oob_fragment(&config), locks_oob_element().await]
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                };
                for frag in fragments {
                    if stream.send(Message::Text(frag)).await.is_err() {
                        return Ok(());
                    }
                }
            }
            // Connection closed → watched set drops with this task (no more polling).
            Ok(())
        })
    })
}

/// Live output of a background op (activation / update / repair / genswitch).
///
/// Streams JSON messages (see [`crate::commands::web::ops::monitor`]): state on change,
/// appended log bytes, `end` once the op is terminal and drained. `offset` resumes the
/// log after a reconnect — neo-web restarts during a switch, the op keeps running in its
/// own systemd unit and writes to disk, and the client picks up where it left off.
#[get("/ws/op/<id>?<offset>")]
pub fn ws_op(ws: WebSocket, id: &str, offset: Option<u64>) -> Channel<'static> {
    use crate::commands::web::ops::monitor::{read_state_message, LogCursor};
    use crate::commands::web::ops::store::log_path;
    use crate::commands::web::util::op_id_ok;

    let id = id.to_string();
    ws.channel(move |mut stream| {
        Box::pin(async move {
            use rocket::futures::{SinkExt, StreamExt};

            async fn send<S>(stream: &mut S, v: &serde_json::Value) -> bool
            where
                S: rocket::futures::Sink<Message> + Unpin,
            {
                stream.send(Message::Text(v.to_string())).await.is_ok()
            }

            if !op_id_ok(&id) {
                let _ = send(
                    &mut stream,
                    &serde_json::json!({"t": "error", "message": "invalid operation id"}),
                )
                .await;
                return Ok(());
            }

            let log = log_path(&id);
            let mut cursor = LogCursor::new(offset.unwrap_or(0));
            let mut last_raw = String::new();
            // Ticks seen since the op went terminal: give the writer a moment to flush
            // trailing log lines before the final drain.
            let mut terminal_ticks = 0u32;
            let mut since_ping = 0u32;
            let mut tick = tokio::time::interval(Duration::from_millis(200));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    client_msg = stream.next() => {
                        match client_msg {
                            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                            Some(Ok(_)) => {}
                        }
                    }
                    _ = tick.tick() => {
                        let terminal = match read_state_message(&id) {
                            Some((raw, msg)) => {
                                let done = msg["status"] != "in_progress";
                                if raw != last_raw {
                                    last_raw = raw;
                                    if !send(&mut stream, &msg).await {
                                        break;
                                    }
                                }
                                done
                            }
                            None => {
                                if last_raw.is_empty() {
                                    let _ = send(&mut stream, &serde_json::json!({
                                        "t": "state", "id": id, "status": "missing",
                                    })).await;
                                }
                                true
                            }
                        };
                        if terminal {
                            terminal_ticks += 1;
                        }
                        let flush = terminal_ticks >= 3;
                        for m in cursor.read(&log, flush) {
                            if !send(&mut stream, &m).await {
                                return Ok(());
                            }
                        }
                        if flush {
                            let _ = send(&mut stream, &serde_json::json!({"t": "end"})).await;
                            break;
                        }
                        // Keep idle proxies (SWAG) from timing out long silent builds.
                        since_ping += 1;
                        if since_ping >= 100 {
                            since_ping = 0;
                            if !send(&mut stream, &serde_json::json!({"t": "ping"})).await {
                                break;
                            }
                        }
                    }
                }
            }
            Ok(())
        })
    })
}
