//! Supabase Realtime client for `run_events` inserts: the Phoenix channel frames as pure
//! functions plus one listen loop over a WebSocket. The access token goes only into the join
//! frame and is never logged.

use crate::{Config, Error, Result};
use serde_json::{json, Value};

/// Seconds between Phoenix heartbeats; Realtime drops a socket silent for 60 s.
pub const HEARTBEAT_SECS: u64 = 25;

/// What one server frame means to the listener.
#[derive(Debug, PartialEq, Eq)]
pub enum Frame {
    Joined,
    Refused(String),
    RunEvent(String),
    Closed,
    Other,
}

/// The listener's news for its caller: the channel joined, or a run got a new event.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Up,
    Run(String),
}

/// The Realtime WebSocket URL of `config`: `wss` for an https project, `ws` for http, with
/// the anon key as `apikey`. Errors when the project URL does not parse.
pub fn socket_url(config: &Config) -> Result<String> {
    let mut url = reqwest::Url::parse(&config.url).map_err(|e| Error::Config(e.to_string()))?;
    let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(scheme).map_err(|_| Error::Config("realtime url scheme".into()))?;
    url.set_path("/realtime/v1/websocket");
    url.query_pairs_mut().clear().append_pair("apikey", &config.anon_key).append_pair("vsn", "1.0.0");
    Ok(url.to_string())
}

/// The channel topic for `user_id`'s run events.
pub fn topic(user_id: &str) -> String {
    format!("realtime:run_events:{user_id}")
}

/// The `phx_join` frame that subscribes to inserts on `public.run_events` for `user_id`,
/// carrying `token` so row level security applies. `reference` tags the server's reply.
pub fn join_frame(user_id: &str, token: &str, reference: u64) -> String {
    assert!(!user_id.is_empty() && !token.is_empty(), "join needs a user and a token");
    json!({
        "topic": topic(user_id),
        "event": "phx_join",
        "ref": reference.to_string(),
        "join_ref": reference.to_string(),
        "payload": {
            "config": {
                "broadcast": { "self": false },
                "presence": { "key": "" },
                "postgres_changes": [
                    { "event": "INSERT", "schema": "public", "table": "run_events", "filter": format!("user_id=eq.{user_id}") }
                ]
            },
            "access_token": token
        }
    })
    .to_string()
}

/// The Phoenix heartbeat frame with `reference`.
pub fn heartbeat_frame(reference: u64) -> String {
    json!({ "topic": "phoenix", "event": "heartbeat", "payload": {}, "ref": reference.to_string() }).to_string()
}

/// Reads one server frame for the channel joined with `join_ref`: the join reply, an insert
/// on `run_events` (as its run id), a close or error, or anything else as `Other`.
/// Malformed text is `Other`, never a panic.
pub fn parse(text: &str, join_ref: u64) -> Frame {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Frame::Other };
    let event = v["event"].as_str().unwrap_or_default();
    let payload = &v["payload"];
    match event {
        "phx_reply" if v["ref"].as_str() == Some(&join_ref.to_string()) => match payload["status"].as_str() {
            Some("ok") => Frame::Joined,
            _ => Frame::Refused(reason(payload)),
        },
        "postgres_changes" => {
            let data = &payload["data"];
            let insert = data["type"].as_str() == Some("INSERT") && data["table"].as_str() == Some("run_events");
            match data["record"]["run_id"].as_str() {
                Some(run) if insert => Frame::RunEvent(run.to_string()),
                _ => Frame::Other,
            }
        }
        "system" if payload["status"].as_str() == Some("error") => Frame::Refused(reason(payload)),
        "phx_close" | "phx_error" => Frame::Closed,
        _ => Frame::Other,
    }
}

fn reason(payload: &Value) -> String {
    let text = payload["response"]["reason"].as_str().or(payload["message"].as_str()).unwrap_or("refused");
    text.chars().take(120).collect()
}

/// Connects to Realtime, joins `user_id`'s run events with `token` and calls `on` with
/// `Event::Up` once joined and `Event::Run` for each insert, sending heartbeats meanwhile.
/// Returns an error when the socket fails, the join is refused or the server closes it.
pub async fn listen(config: &Config, user_id: &str, token: &str, mut on: impl FnMut(Event)) -> Result<()> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let url = socket_url(config)?;
    let (socket, _) = tokio_tungstenite::connect_async(url.as_str()).await.map_err(|e| Error::Http(format!("realtime: {e}")))?;
    let (mut tx, mut rx) = socket.split();
    let join_ref = 1;
    tx.send(Message::text(join_frame(user_id, token, join_ref))).await.map_err(|e| Error::Http(format!("realtime: {e}")))?;
    let mut beat = tokio::time::interval(std::time::Duration::from_secs(HEARTBEAT_SECS));
    beat.tick().await;
    let mut reference = join_ref;
    loop {
        tokio::select! {
            _ = beat.tick() => {
                reference += 1;
                tx.send(Message::text(heartbeat_frame(reference))).await.map_err(|e| Error::Http(format!("realtime: {e}")))?;
            }
            message = rx.next() => {
                let text = match message {
                    Some(Ok(Message::Text(text))) => text.to_string(),
                    Some(Ok(Message::Close(_))) | None => return Err(Error::Http("realtime: socket closed".into())),
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => return Err(Error::Http(format!("realtime: {e}"))),
                };
                match parse(&text, join_ref) {
                    Frame::Joined => on(Event::Up),
                    Frame::RunEvent(run) => on(Event::Run(run)),
                    Frame::Refused(why) => return Err(Error::Status(403, format!("realtime: {why}"))),
                    Frame::Closed => return Err(Error::Http("realtime: channel closed".into())),
                    Frame::Other => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "0190f2a0-0000-7000-8000-0000000000ac";

    #[test]
    fn socket_url_uses_ws_schemes_and_the_anon_key() {
        let key = "anon-key-0123456789abcdef";
        let https = Config::new("https://abc.supabase.co", key).unwrap();
        assert_eq!(socket_url(&https).unwrap(), format!("wss://abc.supabase.co/realtime/v1/websocket?apikey={key}&vsn=1.0.0"));
        let local = Config::new("http://127.0.0.1:54321", key).unwrap();
        assert!(socket_url(&local).unwrap().starts_with("ws://127.0.0.1:54321/realtime/v1/websocket?"));
    }

    #[test]
    fn join_frame_subscribes_to_the_users_run_event_inserts_with_the_token() {
        let v: Value = serde_json::from_str(&join_frame(USER, "jwt-token", 7)).unwrap();
        assert_eq!(v["event"], "phx_join");
        assert_eq!(v["topic"], format!("realtime:run_events:{USER}"));
        assert_eq!(v["ref"], "7");
        assert_eq!(v["payload"]["access_token"], "jwt-token");
        let change = &v["payload"]["config"]["postgres_changes"][0];
        assert_eq!((change["event"].as_str(), change["table"].as_str()), (Some("INSERT"), Some("run_events")));
        assert_eq!(change["filter"], format!("user_id=eq.{USER}"));
        let beat: Value = serde_json::from_str(&heartbeat_frame(8)).unwrap();
        assert_eq!((beat["topic"].as_str(), beat["event"].as_str(), beat["ref"].as_str()), (Some("phoenix"), Some("heartbeat"), Some("8")));
    }

    #[test]
    fn parse_reads_the_join_reply_inserts_and_closes() {
        assert_eq!(parse(r#"{"event":"phx_reply","ref":"1","payload":{"status":"ok","response":{}}}"#, 1), Frame::Joined);
        assert_eq!(parse(r#"{"event":"phx_reply","ref":"2","payload":{"status":"ok","response":{}}}"#, 1), Frame::Other);
        assert_eq!(
            parse(r#"{"event":"phx_reply","ref":"1","payload":{"status":"error","response":{"reason":"bad token"}}}"#, 1),
            Frame::Refused("bad token".into())
        );
        let insert = r#"{"event":"postgres_changes","payload":{"data":{"type":"INSERT","table":"run_events","record":{"run_id":"r-1","seq":3}}}}"#;
        assert_eq!(parse(insert, 1), Frame::RunEvent("r-1".into()));
        assert_eq!(parse(&insert.replace("run_events", "runs"), 1), Frame::Other);
        assert_eq!(parse(&insert.replace("INSERT", "DELETE"), 1), Frame::Other);
        assert_eq!(parse(r#"{"event":"system","payload":{"status":"error","message":"token expired"}}"#, 1), Frame::Refused("token expired".into()));
        assert_eq!(parse(r#"{"event":"phx_close","payload":{}}"#, 1), Frame::Closed);
        assert_eq!(parse("not json", 1), Frame::Other);
    }
}
