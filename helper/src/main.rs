//! omarchy-thread — the working half of the Omarchy plugin for the Thread.
//!
//! The QML in the bar is deliberately thin: it runs this binary and renders
//! what it prints. Everything that can go wrong — serving a world over
//! `.well-known`, holding a socket open to a presence relay, checking that a
//! published domain is actually conformant — happens here, where it can be
//! tested without a compositor.
//!
//! It speaks only public surfaces of the Thread: the world manifest format
//! (crates.io `thread-manifest`), `.well-known/thread/world.json` resolution,
//! and the presence wire protocol. No Pixygon-only APIs — a room served from
//! this machine is readable by any conformant browser, and this binary would
//! work against anybody's relay.
//!
//!   omarchy-thread new <name>      scaffold a room you own
//!   omarchy-thread rooms           list your rooms
//!   omarchy-thread serve [room]    serve it + watch who is inside (daemon)
//!   omarchy-thread status          one line of JSON for the bar
//!   omarchy-thread open [room]     serve, then open it in the browser
//!   omarchy-thread invite [room]   the address to hand someone
//!   omarchy-thread publish <host>  put it on your own domain, then verify
//!   omarchy-thread stop            stop serving
//!   omarchy-thread doctor          what's installed, what's reachable

use std::collections::BTreeMap;
use std::fs;
use std::net::{TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

const DEFAULT_RELAY: &str = "wss://relay.pixygon.io";
const DEFAULT_PORT: u16 = 7777;

// ── where things live ───────────────────────────────────────────────────────

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

/// Rooms are plain directories you own — a world.json and whatever it points
/// at. Nothing is hidden in a database; you can copy one to a USB stick.
fn rooms_dir() -> PathBuf {
    std::env::var("OMARCHY_THREAD_ROOMS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join("Worlds"))
}

fn state_dir() -> PathBuf {
    std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".local/state"))
        .join("omarchy-thread")
}

fn state_path() -> PathBuf {
    state_dir().join("state.json")
}

fn relay() -> String {
    std::env::var("OMARCHY_THREAD_RELAY").unwrap_or_else(|_| DEFAULT_RELAY.into())
}

/// The name co-travelers see. Not an identity claim — the reference relay runs
/// open — just a courtesy so a roster reads as people rather than numbers.
fn traveler_name() -> String {
    std::env::var("OMARCHY_THREAD_NAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "a traveler".into())
}

// ── the room template ───────────────────────────────────────────────────────

/// A room that is worth standing in on the first run: a floor, a ring of
/// pillars, a low table to gather around, spawns that face each other, and a
/// door back out to the wider Thread. Builtin meshes only — no asset fetch, so
/// a fresh room is conformant and renderable the instant it is written.
///
/// GEOMETRY CONTRACT: a builtin mesh is **one unit, centred on its origin**, so
/// `scale.y` is the finished height in metres and a thing rests on the floor
/// when `position.y == scale.y / 2`. (The reference browser's `cylinder` was
/// two units until Infinite v0.111.0; anything authored against that older
/// build stands at double height and half-buried. `rests_on_the_floor` below
/// is what stops this drifting again.)
fn template_world(slug: &str, title: &str) -> Value {
    let mut prefabs = vec![
        json!({ "id": "60000001", "mesh": { "builtin": "plane" },
                "material": { "base_color": [0.13, 0.13, 0.17, 1.0], "roughness": 0.95 } }),
        json!({ "id": "60000002", "mesh": { "builtin": "cylinder" },
                "material": { "base_color": [0.72, 0.74, 0.82, 1.0], "roughness": 0.6 } }),
        json!({ "id": "60000003", "mesh": { "builtin": "cylinder" },
                "material": { "base_color": [0.35, 0.62, 0.68, 1.0], "roughness": 0.35, "metallic": 0.2 } }),
    ];
    prefabs.push(json!({ "id": "60000004", "mesh": { "builtin": "cube" },
                         "material": { "base_color": [0.09, 0.10, 0.13, 1.0], "roughness": 0.9 } }));

    let mut placements = vec![
        json!({ "prefab": "60000001", "name": "floor", "position": [0, 0, 0], "scale": [16, 1, 16] }),
        json!({ "prefab": "60000003", "name": "table", "position": [0, 0.38, 0], "scale": [2.4, 0.75, 2.4] }),
    ];

    // Eight pillars on a ring: enough to read as a room, cheap enough to load
    // instantly. The gap at the north is the doorway.
    let pillars = 8;
    for i in 0..pillars {
        let a = (i as f64) * std::f64::consts::TAU / (pillars as f64) + 0.39;
        let (x, z) = (a.cos() * 6.4, a.sin() * 6.4);
        placements.push(json!({
            "prefab": "60000002",
            "name": format!("pillar-{}", i + 1),
            "position": [round2(x), 1.6, round2(z)],
            "scale": [0.34, 3.2, 0.34]
        }));
    }
    placements.push(json!({ "prefab": "60000004", "name": "lintel",
                            "position": [0, 3.3, -6.4], "scale": [3.2, 0.3, 0.5] }));

    let mut spawns = vec![];
    for i in 0..4 {
        let a = (i as f64) * std::f64::consts::TAU / 4.0;
        let (x, z) = (a.cos() * 3.2, a.sin() * 3.2);
        spawns.push(json!({
            "name": if i == 0 { "entry".to_string() } else { format!("seat-{}", i) },
            "position": [round2(x), 0, round2(z)],
            "yaw": round2(a + std::f64::consts::PI)
        }));
    }

    json!({
        "thread": "thread/0.1",
        "world": {
            "id": slug,
            "title": title,
            "description": format!("{} — a room on the Thread, served from a desk.", title),
            "author": { "id": format!("did:omarchy:{}", traveler_name()), "name": traveler_name() },
            "codex": [],
            "license": "CC-BY-4.0"
        },
        "environment": {
            "year": 0,
            "sky": { "zenith": [0.03, 0.04, 0.07], "horizon": [0.14, 0.14, 0.20], "sun_dir": [0.3, 0.7, 0.2] }
        },
        "spawns": spawns,
        "prefabs": prefabs,
        "placements": placements,
        "portals": [
            { "id": "out", "position": [0, 1.5, -6.4], "scale": [2.4, 3.0, 0.2],
              "to": "thread://pixygon.io", "label": "The Thread", "preview": "static" }
        ],
        "presence": { "mode": "relay", "relays": [relay()], "max_occupants": 32, "voice": true }
    })
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

// ── rooms on disk ───────────────────────────────────────────────────────────

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn list_rooms() -> Vec<(String, PathBuf)> {
    let mut rooms = vec![];
    if let Ok(entries) = fs::read_dir(rooms_dir()) {
        for e in entries.flatten() {
            let p = e.path();
            if p.join("world.json").is_file() {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    rooms.push((name.to_string(), p.clone()));
                }
            }
        }
    }
    rooms.sort_by(|a, b| a.0.cmp(&b.0));
    rooms
}

fn resolve_room(arg: Option<&str>) -> Result<(String, PathBuf), String> {
    let rooms = list_rooms();
    match arg {
        Some(name) => rooms
            .into_iter()
            .find(|(n, _)| n == name)
            .ok_or_else(|| format!("no room called \"{name}\" in {}", rooms_dir().display())),
        None => {
            // No argument: the room that is serving, else the only one there is.
            if let Some(cur) = read_state().get("room").and_then(Value::as_str) {
                if let Some(hit) = rooms.iter().find(|(n, _)| n == cur) {
                    return Ok(hit.clone());
                }
            }
            match rooms.len() {
                0 => Err(format!(
                    "no rooms yet — make one with:  omarchy-thread new \"My Room\""
                )),
                _ => Ok(rooms[0].clone()),
            }
        }
    }
}

fn read_world(dir: &Path) -> Result<Value, String> {
    let raw = fs::read_to_string(dir.join("world.json"))
        .map_err(|e| format!("cannot read {}/world.json: {e}", dir.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("{}/world.json is not valid JSON: {e}", dir.display()))
}

/// Validate against the reference types, not against our own opinion of the
/// format — the same crate a browser uses, so "valid here" means "valid there".
fn validate_world(raw: &str) -> Result<(), String> {
    let manifest: thread_manifest::WorldManifest =
        serde_json::from_str(raw).map_err(|e| format!("not a thread/0.1 manifest: {e}"))?;
    manifest.validate().map_err(|e| format!("{e}"))
}

// ── state the bar reads ─────────────────────────────────────────────────────

fn read_state() -> Value {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({ "serving": false, "count": 0, "occupants": [] }))
}

fn write_state(v: &Value) {
    let _ = fs::create_dir_all(state_dir());
    // Write-then-rename: the bar polls this file and must never read a half
    // written one.
    let tmp = state_dir().join("state.json.tmp");
    if fs::write(&tmp, serde_json::to_vec_pretty(v).unwrap_or_default()).is_ok() {
        let _ = fs::rename(&tmp, state_path());
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The address of this machine on the local network. A room served from a desk
/// is reachable by anyone on the same network without a domain, a tunnel, or an
/// account — which is the whole point of the local tier.
fn lan_addr() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("1.1.1.1:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

fn free_port(preferred: u16) -> u16 {
    for port in preferred..preferred + 40 {
        if TcpListener::bind(("0.0.0.0", port)).is_ok() {
            return port;
        }
    }
    preferred
}

// ── serving a world over .well-known ────────────────────────────────────────

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "json" => "application/json",
        "glb" | "gltf" => "model/gltf-binary",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "wasm" => "application/wasm",
        "ogg" => "audio/ogg",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

/// The resolution contract in twenty lines: `.well-known/thread/world.json`,
/// JSON content type, and `Access-Control-Allow-Origin: *` so a browser on any
/// origin may read it. Assets resolve relative to the manifest directory.
fn serve_http(dir: PathBuf, port: u16, state: Arc<Mutex<Value>>) {
    let server = match tiny_http::Server::http(("0.0.0.0", port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot serve on port {port}: {e}");
            return;
        }
    };
    for request in server.incoming_requests() {
        let url = request.url().split('?').next().unwrap_or("/").to_string();
        let rel = url.trim_start_matches('/');
        let mapped = if rel == ".well-known/thread/world.json" || rel == "world.json" {
            Some("world.json".to_string())
        } else if let Some(rest) = rel.strip_prefix(".well-known/thread/") {
            Some(rest.to_string())
        } else if rel.is_empty() {
            Some("world.json".to_string())
        } else {
            Some(rel.to_string())
        };

        // Never serve outside the room directory, whatever the path claims.
        let safe = mapped.filter(|m| !m.contains("..")).map(|m| dir.join(m));
        let response = match safe {
            Some(path) if path.is_file() => {
                let body = fs::read(&path).unwrap_or_default();
                let ctype = content_type(path.to_str().unwrap_or(""));
                let mut r = tiny_http::Response::from_data(body);
                for (k, v) in [
                    ("Content-Type", ctype),
                    ("Access-Control-Allow-Origin", "*"),
                    ("Cache-Control", "no-cache"),
                ] {
                    if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
                        r.add_header(h);
                    }
                }
                r
            }
            _ => tiny_http::Response::from_string("not found").with_status_code(404),
        };
        let _ = request.respond(response);

        if let Ok(mut s) = state.lock() {
            s["requests"] = json!(s["requests"].as_u64().unwrap_or(0) + 1);
            write_state(&s);
        }
    }
}

// ── watching who is inside ──────────────────────────────────────────────────

/// Hold a socket to the relay and keep the roster current.
///
/// The bar OBSERVES; it never joins. An observer is not an occupant: it takes
/// no body, no pose, no seat in the room. (If the relay is older than the
/// observe verb it simply won't answer, and the bar shows the room as served
/// but unwatched — the plugin degrades to a launcher rather than lying.)
fn watch_presence(world_id: String, state: Arc<Mutex<Value>>) {
    // rustls will not pick a crypto backend for us; say which one out loud.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let url = format!("{}/thread/{}", relay().trim_end_matches('/'), world_id);
    loop {
        match tungstenite::connect(&url) {
            Ok((mut socket, _)) => {
                let hello = json!({ "t": "observe" });
                if socket.send(tungstenite::Message::Text(hello.to_string())).is_err() {
                    sleep_secs(5);
                    continue;
                }
                if let Ok(mut s) = state.lock() {
                    s["relay_connected"] = json!(true);
                    write_state(&s);
                }
                let mut roster: BTreeMap<u64, String> = BTreeMap::new();
                loop {
                    match socket.read() {
                        Ok(tungstenite::Message::Text(txt)) => {
                            if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                        if v.get("t").and_then(Value::as_str) == Some("welcome")
                            && v.get("observer").and_then(Value::as_bool) != Some(true)
                        {
                            // Not an observer welcome: an older relay would have
                            // seated us in the room. Leave rather than haunt it.
                            let _ = socket.close(None);
                            break;
                        }
                    }
                    if apply_presence(&txt, &mut roster) {
                                if let Ok(mut s) = state.lock() {
                                    s["occupants"] = json!(roster
                                        .values()
                                        .map(|n| json!({ "name": n }))
                                        .collect::<Vec<_>>());
                                    s["count"] = json!(roster.len());
                                    s["updated"] = json!(now_secs());
                                    write_state(&s);
                                }
                            }
                        }
                        Ok(tungstenite::Message::Close(_)) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            }
            Err(_) => {}
        }
        if let Ok(mut s) = state.lock() {
            s["relay_connected"] = json!(false);
            s["count"] = json!(0);
            s["occupants"] = json!([]);
            write_state(&s);
        }
        sleep_secs(5);
    }
}

/// Fold one relay frame into the roster. Returns true when the roster changed.
fn apply_presence(txt: &str, roster: &mut BTreeMap<u64, String>) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(txt) else {
        return false;
    };
    let name_of = |o: &Value, id: u64| -> String {
        o.get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("traveler {id}"))
    };
    match v.get("t").and_then(Value::as_str) {
        Some("welcome") => {
            // The relay marks an observer welcome explicitly, so we can be sure
            // we are watching the room rather than standing in it.
            roster.clear();
            if let Some(list) = v.get("occupants").and_then(Value::as_array) {
                for o in list {
                    if let Some(id) = o.get("id").and_then(Value::as_u64) {
                        roster.insert(id, name_of(o, id));
                    }
                }
            }
            true
        }
        Some("join") => match v.get("id").and_then(Value::as_u64) {
            Some(id) => {
                roster.insert(id, name_of(&v, id));
                true
            }
            None => false,
        },
        Some("leave") => match v.get("id").and_then(Value::as_u64) {
            Some(id) => roster.remove(&id).is_some(),
            None => false,
        },
        _ => false,
    }
}

fn sleep_secs(n: u64) {
    std::thread::sleep(Duration::from_secs(n));
}

// ── commands ────────────────────────────────────────────────────────────────

fn cmd_new(args: &[String]) -> Result<(), String> {
    let title = if args.is_empty() { "A Room".to_string() } else { args.join(" ") };
    let slug = {
        let s = slugify(&title);
        if s.is_empty() { "room".to_string() } else { s }
    };
    let dir = rooms_dir().join(&slug);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    let designed = generate_with_cli(&dir, &title).is_some();
    if !designed {
        let world = template_world(&slug, &title);
        let raw = serde_json::to_string_pretty(&world).unwrap_or_default();
        validate_world(&raw)?;
        fs::write(dir.join("world.json"), format!("{raw}\n"))
            .map_err(|e| format!("cannot write world.json: {e}"))?;
    }

    println!("{}", json!({
        "room": slug, "path": dir.display().to_string(), "title": title,
        "built_by": if designed { "thread level" } else { "built-in template" }
    }));
    eprintln!("✓ {} — a room of your own at {}", title, dir.display());
    if designed {
        eprintln!("  designed by the Thread level creator");
    }
    eprintln!("  open it:    omarchy-thread open {slug}");
    Ok(())
}

/// Check a world.json — a room of yours, or any file. Uses the same reference
/// types a browser uses, so "valid here" means "valid there".
fn cmd_validate(args: &[String]) -> Result<(), String> {
    let path = match args.first() {
        Some(a) if Path::new(a).is_file() => PathBuf::from(a),
        other => resolve_room(other.map(String::as_str))?.1.join("world.json"),
    };
    let raw = fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    match validate_world(&raw) {
        Ok(()) => {
            println!("{}", json!({ "path": path.display().to_string(), "valid": true }));
            eprintln!("✓ {} is a conformant thread/0.1 world", path.display());
            Ok(())
        }
        Err(e) => {
            println!("{}", json!({ "path": path.display().to_string(), "valid": false, "error": e }));
            Err(e)
        }
    }
}

/// Ask the Thread CLI's level designer for a room, when the author has it.
///
/// The generator knows far more about making a place worth standing in than a
/// hardcoded template does — it shops the Quarry for real columns and arches.
/// But it is not a dependency: it lives in a different toolchain, it wants the
/// network, and a stranger installing this plugin will not have it. So it is
/// strictly an upgrade path, and any failure falls back to the built-in room
/// rather than leaving someone with no room at all.
fn generate_with_cli(dir: &Path, title: &str) -> Option<Value> {
    if std::env::var("OMARCHY_THREAD_NO_CLI").is_ok() || which("thread").is_none() {
        return None;
    }
    let figure = std::env::var("OMARCHY_THREAD_FIGURE").unwrap_or_else(|_| "hall".into());
    let out = dir.join("world.json");
    let args = json!([title, 14, 5.2, 12, "classical", "marble", "dusk"]).to_string();
    // Ask for a relay outright. Older CLIs don't know the flag and exit
    // non-zero, so fall back to asking without it — losing the flag should
    // cost you the relay, not the whole designed room.
    let run = |extra: &[&str]| -> bool {
        Command::new("thread")
            .args(["level", "--figure", &figure, "--args", &args, "-o"])
            .arg(&out)
            .args(extra)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|st| st.success())
            .unwrap_or(false)
    };
    let relay = relay();
    if !run(&["--relay", &relay]) && !run(&[]) {
        let _ = fs::remove_file(&out);
        return None;
    }
    let mut world: Value = serde_json::from_str(&fs::read_to_string(&out).ok()?).ok()?;

    // The designer builds the room; the plugin supplies what a *hosted* room
    // needs and it leaves blank — who made it, and the relay that lets anyone
    // else be in it. Without presence a generated room is beautiful and empty.
    let w = world.get_mut("world")?.as_object_mut()?;
    // The designer emits these keys and leaves them empty, so "absent" is not
    // the test — null and "" both mean nobody filled this in.
    let blank = |v: Option<&Value>| match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.trim().is_empty(),
        _ => false,
    };
    let mut fill = |key: &str, value: Value| {
        if blank(w.get(key)) {
            w.insert(key.to_string(), value);
        }
    };
    fill("author", json!({ "id": format!("did:omarchy:{}", traveler_name()), "name": traveler_name() }));
    fill("description", json!(format!("{title} — a room on the Thread, served from a desk.")));
    fill("license", json!("CC-BY-4.0"));
    fill("codex", json!([]));
    ensure_relay_presence(&mut world);
    // A veil with nowhere to go is not a door.
    if let Some(portals) = world.get_mut("portals").and_then(Value::as_array_mut) {
        portals.retain(|p| p.get("to").and_then(Value::as_str).map(|t| t.starts_with("thread://")).unwrap_or(false));
    }

    let raw = serde_json::to_string_pretty(&world).ok()?;
    if validate_world(&raw).is_err() {
        let _ = fs::remove_file(&out);
        return None;
    }
    fs::write(&out, format!("{raw}\n")).ok()?;
    Some(world)
}

/// A room this plugin hosts is meant to have people in it.
///
/// The designer states solo explicitly now (`{"mode":"solo"}`) rather than
/// omitting presence, which is better for every other reader and quietly fatal
/// here: a check for "absent or null" passes right over it and the room ships
/// silent. Presence is therefore decided by whether a relay is actually named,
/// never by whether the key exists.
fn ensure_relay_presence(world: &mut Value) {
    let named_relay = world
        .get("presence")
        .and_then(|p| p.get("relays"))
        .and_then(Value::as_array)
        .map(|r| r.iter().any(|v| v.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false)))
        .unwrap_or(false);
    if named_relay {
        return;
    }
    world["presence"] = json!({
        "mode": "relay",
        "relays": [relay()],
        "max_occupants": 32,
        "voice": true
    });
}

fn cmd_rooms() -> Result<(), String> {
    let state = read_state();
    let current = state.get("room").and_then(Value::as_str).unwrap_or("");
    let rooms: Vec<Value> = list_rooms()
        .into_iter()
        .map(|(name, path)| {
            let world = read_world(&path).unwrap_or_else(|_| json!({}));
            json!({
                "room": name,
                "title": world.pointer("/world/title").and_then(Value::as_str).unwrap_or(&name),
                "path": path.display().to_string(),
                "serving": name == current && state["serving"].as_bool().unwrap_or(false),
            })
        })
        .collect();
    println!("{}", json!({ "rooms": rooms }));
    Ok(())
}

fn cmd_status() -> Result<(), String> {
    let mut state = read_state();
    let token = fs::read_to_string(passport_path()).unwrap_or_default().trim().to_string();
    state["signed_in_as"] = passport_claims(&token)
        .get("name")
        .cloned()
        .unwrap_or(Value::Null);
    // A stale state file (the daemon was killed) must not show a room as live.
    if state["serving"].as_bool().unwrap_or(false) {
        let pid = state["pid"].as_u64().unwrap_or(0);
        if pid == 0 || !Path::new(&format!("/proc/{pid}")).exists() {
            state = json!({ "serving": false, "count": 0, "occupants": [] });
            write_state(&state);
        }
    }
    println!("{state}");
    Ok(())
}

fn cmd_serve(args: &[String]) -> Result<(), String> {
    let (name, dir) = resolve_room(args.first().map(String::as_str))?;
    let world = read_world(&dir)?;
    let raw = fs::read_to_string(dir.join("world.json")).unwrap_or_default();
    validate_world(&raw)?;

    let world_id = world
        .pointer("/world/id")
        .and_then(Value::as_str)
        .unwrap_or(&name)
        .to_string();
    let title = world
        .pointer("/world/title")
        .and_then(Value::as_str)
        .unwrap_or(&name)
        .to_string();

    let port = free_port(
        std::env::var("OMARCHY_THREAD_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT),
    );
    let addr = lan_addr();

    let state = Arc::new(Mutex::new(json!({
        "room": name,
        "world_id": world_id,
        "title": title,
        "serving": true,
        "pid": std::process::id(),
        "port": port,
        "url": format!("http://{addr}:{port}/.well-known/thread/world.json"),
        "invite": format!("thread://{addr}:{port}"),
        "relay": relay(),
        "relay_connected": false,
        "occupants": [],
        "count": 0,
        "requests": 0,
        "updated": now_secs(),
    })));
    write_state(&state.lock().unwrap());

    eprintln!("◈ {title} is open");
    eprintln!("  address:  thread://{addr}:{port}");
    eprintln!("  manifest: http://{addr}:{port}/.well-known/thread/world.json");

    {
        let state = Arc::clone(&state);
        let world_id = world_id.clone();
        std::thread::spawn(move || {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                watch_presence(world_id, Arc::clone(&state))
            }))
            .is_err()
            {
                if let Ok(mut s) = state.lock() {
                    s["relay_connected"] = json!(false);
                    write_state(&s);
                }
            }
        });
    }
    serve_http(dir, port, state);
    Ok(())
}

fn cmd_invite(args: &[String]) -> Result<(), String> {
    let state = read_state();
    if state["serving"].as_bool().unwrap_or(false) {
        println!("{}", state["invite"].as_str().unwrap_or(""));
        return Ok(());
    }
    let (name, _) = resolve_room(args.first().map(String::as_str))?;
    Err(format!("\"{name}\" is not open yet — omarchy-thread open {name}"))
}

/// Walk somewhere. With a `thread://` address this is pure browsing — no
/// serving, no room of your own — which is the other half of being a citizen
/// of a medium rather than a host stuck admiring their own house.
fn cmd_open(args: &[String]) -> Result<(), String> {
    if let Some(addr) = args.first().filter(|a| a.starts_with("thread://")) {
        if which("infinite").is_none() {
            return Err(format!(
                "no Thread browser installed — get one at https://thread.pixygon.io/browser ({addr})"
            ));
        }
        Command::new("infinite")
            .arg(addr)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot open the browser: {e}"))?;
        println!("{addr}");
        return Ok(());
    }
    let (name, _) = resolve_room(args.first().map(String::as_str))?;
    let state = read_state();
    let already = state["serving"].as_bool().unwrap_or(false)
        && state["room"].as_str() == Some(name.as_str());
    if !already {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        Command::new(exe)
            .arg("serve")
            .arg(&name)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start the server: {e}"))?;
        // The daemon writes its state as soon as the socket is up.
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(50));
            if read_state()["serving"].as_bool().unwrap_or(false) {
                break;
            }
        }
    }
    let invite = read_state()["invite"].as_str().unwrap_or_default().to_string();
    if invite.is_empty() {
        return Err("the room did not come up".into());
    }
    if which("infinite").is_some() {
        let _ = Command::new("infinite").arg(&invite).spawn();
    } else {
        eprintln!("The Infinite browser isn't installed — the room is open at {invite}");
        eprintln!("Get it: https://thread.pixygon.io/browser");
    }
    println!("{invite}");
    Ok(())
}

fn cmd_stop() -> Result<(), String> {
    let state = read_state();
    if let Some(pid) = state["pid"].as_u64() {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
    write_state(&json!({ "serving": false, "count": 0, "occupants": [] }));
    eprintln!("the room is closed");
    Ok(())
}

/// Put the room on a domain you own, then check the internet agrees.
///
/// Publishing is three files in a directory — that is the whole protocol. We
/// write the tree, hand you the one rsync line, and then verify the live host
/// the way a browser would, so "published" is a fact rather than a hope.
fn cmd_publish(args: &[String]) -> Result<(), String> {
    let host = args
        .first()
        .ok_or("usage: omarchy-thread publish <your-domain.com> [room]")?
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_string();
    let (name, dir) = resolve_room(args.get(1).map(String::as_str))?;
    let raw = fs::read_to_string(dir.join("world.json")).unwrap_or_default();
    validate_world(&raw)?;

    let out = dir.join(".publish/.well-known/thread");
    fs::create_dir_all(&out).map_err(|e| format!("cannot prepare the upload: {e}"))?;
    fs::write(out.join("world.json"), &raw).map_err(|e| format!("{e}"))?;
    for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let p = entry.path();
        let is_asset = p.is_file()
            && p.file_name().and_then(|n| n.to_str()).map(|n| n != "world.json").unwrap_or(false);
        if is_asset {
            if let Some(fname) = p.file_name() {
                let _ = fs::copy(&p, out.join(fname));
            }
        }
    }

    println!(
        "{}",
        json!({ "room": name, "host": host, "staged": out.display().to_string() })
    );
    eprintln!("✓ staged {}", out.display());
    eprintln!("  upload it:  rsync -av {}/ you@{}:/var/www/{}/.well-known/thread/", out.display(), host, host);
    eprintln!("  then:       omarchy-thread verify {host}");
    Ok(())
}

/// Check a live host the way a browser does — nothing about this is
/// Pixygon-specific, so it is equally a conformance check on somebody else's
/// domain.
fn cmd_verify(args: &[String]) -> Result<(), String> {
    let host = args
        .first()
        .ok_or("usage: omarchy-thread verify <your-domain.com>")?
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let url = format!("https://{host}/.well-known/thread/world.json");
    let out = Command::new("curl")
        .args(["-sS", "-D", "-", "--max-time", "15", &url])
        .output()
        .map_err(|e| format!("curl is needed to check a live host: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (head, body) = text.split_once("\r\n\r\n").or_else(|| text.split_once("\n\n")).unwrap_or(("", ""));

    let ok_status = head.lines().next().map(|l| l.contains(" 200")).unwrap_or(false);
    let lower = head.to_lowercase();
    let ok_cors = lower.contains("access-control-allow-origin: *");
    let ok_type = lower.contains("content-type: application/json");
    let manifest = validate_world(body.trim());

    let findings = json!({
        "url": url,
        "reachable": ok_status,
        "cors": ok_cors,
        "content_type": ok_type,
        "manifest": match &manifest { Ok(_) => json!(true), Err(e) => json!(e) },
        "conformant": ok_status && ok_cors && ok_type && manifest.is_ok(),
    });
    println!("{findings}");
    let mark = |b: bool| if b { "✓" } else { "✗" };
    eprintln!("{} reachable            {url}", mark(ok_status));
    eprintln!("{} Access-Control-Allow-Origin: *", mark(ok_cors));
    eprintln!("{} Content-Type: application/json", mark(ok_type));
    match &manifest {
        Ok(_) => eprintln!("✓ a conformant thread/0.1 world"),
        Err(e) => eprintln!("✗ manifest: {e}"),
    }
    if findings["conformant"].as_bool().unwrap_or(false) {
        eprintln!("\n◈ thread://{host} is live on the Thread.");
        Ok(())
    } else {
        Err("not conformant yet".into())
    }
}

fn which(bin: &str) -> Option<PathBuf> {
    std::env::var("PATH").ok().and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(bin))
            .find(|p| p.is_file())
    })
}

fn cmd_doctor() -> Result<(), String> {
    let rooms = list_rooms();
    let report = json!({
        "rooms_dir": rooms_dir().display().to_string(),
        "rooms": rooms.len(),
        "browser": which("infinite").map(|p| p.display().to_string()),
        "relay": relay(),
        "lan": lan_addr(),
        "name": traveler_name(),
        "state": state_path().display().to_string(),
    });
    println!("{report}");
    eprintln!("rooms      {} in {}", rooms.len(), rooms_dir().display());
    match which("infinite") {
        Some(p) => eprintln!("browser    {}", p.display()),
        None => eprintln!("browser    not installed — https://thread.pixygon.io/browser"),
    }
    eprintln!("relay      {}", relay());
    eprintln!("address    {}", lan_addr());
    let token = fs::read_to_string(passport_path()).unwrap_or_default();
    match passport_claims(token.trim()).get("name").and_then(Value::as_str) {
        Some(n) => eprintln!("passport   {n}"),
        None => eprintln!("passport   none — anonymous (fine)"),
    }
    if rooms.is_empty() {
        eprintln!("\nNo rooms yet. Make one:  omarchy-thread new \"Monday Standup\"");
    }
    Ok(())
}

/// Where the reference browser looks for the traveler's Passport. The plugin
/// does not mint identity and does not hold a second copy of it — it writes the
/// one file Infinite already reads, so signing in once is signing in.
fn passport_path() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".config"))
        .join("infinite")
        .join("passport.token")
}

/// Read the claims out of a Passport without verifying it. Verification is the
/// relay's job against the issuer's keys; here we only want a name to show, and
/// an unreadable token is simply an anonymous one.
fn passport_claims(token: &str) -> Value {
    let Some(payload) = token.split('.').nth(1) else {
        return json!({});
    };
    let mut buf = payload.replace('-', "+").replace('_', "/");
    while buf.len() % 4 != 0 {
        buf.push('=');
    }
    let Some(bytes) = b64_decode(&buf) else { return json!({}) };
    serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}))
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let val = |c: u8| -> Option<u32> { TABLE.iter().position(|&t| t == c).map(|i| i as u32) };
    let mut out = Vec::new();
    let bytes: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    for chunk in bytes.chunks(4) {
        let mut acc = 0u32;
        let mut n: usize = 0;
        for &c in chunk {
            if c == b'=' {
                break;
            }
            acc = (acc << 6) | val(c)?;
            n += 1;
        }
        acc <<= 6 * (4 - n);
        for i in 0..n.saturating_sub(1) {
            out.push(((acc >> (16 - 8 * i)) & 0xff) as u8);
        }
    }
    Some(out)
}

fn cmd_passport(args: &[String]) -> Result<(), String> {
    let path = passport_path();
    match args.first().map(String::as_str) {
        None | Some("status") => {
            let token = fs::read_to_string(&path).unwrap_or_default().trim().to_string();
            let claims = passport_claims(&token);
            let out = json!({
                "path": path.display().to_string(),
                "present": !token.is_empty(),
                "name": claims.get("name").and_then(Value::as_str),
                "sub": claims.get("sub").and_then(Value::as_str),
            });
            println!("{out}");
            if token.is_empty() {
                eprintln!("no passport — you walk anonymously, which is a valid way to walk");
                eprintln!("  get one:  https://pixygon.io/passport   then:  omarchy-thread passport set <token>");
            } else {
                let who = claims.get("name").and_then(Value::as_str).unwrap_or("(no name claim)");
                eprintln!("signed in as {who}");
            }
            Ok(())
        }
        Some("set") => {
            let token = args.get(1).ok_or("usage: omarchy-thread passport set <token>")?;
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).map_err(|e| format!("{e}"))?;
            }
            fs::write(&path, format!("{}\n", token.trim())).map_err(|e| format!("{e}"))?;
            // A passport is a bearer token: nobody else on this machine needs it.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            }
            eprintln!("✓ passport saved to {}", path.display());
            cmd_passport(&[])
        }
        Some("clear") => {
            let _ = fs::remove_file(&path);
            eprintln!("✓ passport cleared — you are anonymous again");
            Ok(())
        }
        Some(other) => Err(format!("unknown passport command \"{other}\" (status | set | clear)")),
    }
}

fn desktop_file_path() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".local/share"))
        .join("applications")
        .join("omarchy-thread.desktop")
}

/// Make `thread://` addresses clickable everywhere on the desktop — a link in a
/// chat window opens the world, the way an http:// link opens a page. This
/// edits your desktop's default-application table, so it only ever happens when
/// you ask for it by name.
fn cmd_handler(args: &[String]) -> Result<(), String> {
    let path = desktop_file_path();
    match args.first().map(String::as_str) {
        None | Some("status") => {
            let current = Command::new("xdg-mime")
                .args(["query", "default", "x-scheme-handler/thread"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            println!(
                "{}",
                json!({ "installed": path.exists(), "handler": current, "desktop_file": path.display().to_string() })
            );
            if current.is_empty() {
                eprintln!("thread:// links are not handled — omarchy-thread handler install");
            } else {
                eprintln!("thread:// opens with {current}");
            }
            Ok(())
        }
        Some("install") => {
            if which("infinite").is_none() {
                eprintln!("note: the Infinite browser isn't installed yet — links will register but not open");
            }
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).map_err(|e| format!("{e}"))?;
            }
            fs::write(
                &path,
                "[Desktop Entry]\n\
                 Type=Application\n\
                 Name=Thread\n\
                 Comment=Open a place on the Thread\n\
                 Exec=infinite %u\n\
                 Terminal=false\n\
                 NoDisplay=true\n\
                 MimeType=x-scheme-handler/thread;\n",
            )
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            let _ = Command::new("update-desktop-database")
                .arg(path.parent().unwrap_or(Path::new(".")))
                .status();
            let ok = Command::new("xdg-mime")
                .args(["default", "omarchy-thread.desktop", "x-scheme-handler/thread"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                return Err("xdg-mime would not set the default handler".into());
            }
            eprintln!("✓ thread:// links now open in the browser");
            Ok(())
        }
        Some("remove") => {
            let _ = fs::remove_file(&path);
            let _ = Command::new("update-desktop-database")
                .arg(path.parent().unwrap_or(Path::new(".")))
                .status();
            eprintln!("✓ thread:// handler removed");
            Ok(())
        }
        Some(other) => Err(format!("unknown handler command \"{other}\" (status | install | remove)")),
    }
}

fn usage() {
    eprintln!("{}", include_str!("usage.txt"));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r.to_vec()),
        None => {
            usage();
            std::process::exit(2);
        }
    };
    let result = match cmd {
        "new" => cmd_new(&rest),
        "rooms" => cmd_rooms(),
        "serve" => cmd_serve(&rest),
        "status" => cmd_status(),
        "open" => cmd_open(&rest),
        "invite" => cmd_invite(&rest),
        "publish" => cmd_publish(&rest),
        "verify" => cmd_verify(&rest),
        "stop" => cmd_stop(),
        "doctor" => cmd_doctor(),
        "validate" => cmd_validate(&rest),
        "passport" => cmd_passport(&rest),
        "handler" => cmd_handler(&rest),
        "-h" | "--help" | "help" => {
            usage();
            Ok(())
        }
        other => Err(format!("unknown command \"{other}\"")),
    };
    if let Err(e) = result {
        eprintln!("✗ {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_is_conformant() {
        let raw = serde_json::to_string(&template_world("test-room", "Test Room")).unwrap();
        validate_world(&raw).expect("the room we hand people must be valid");
    }

    /// Everything in the room stands ON the floor. With unit builtins that is
    /// exactly `position.y == scale.y / 2` — the invariant a doubled mesh
    /// height silently breaks, and the reason the first render of a template
    /// looks "wrong but plausible" rather than obviously broken.
    #[test]
    fn rests_on_the_floor() {
        let world = template_world("test-room", "Test Room");
        let placements = world["placements"].as_array().unwrap();
        let mut standing = 0;
        for p in placements {
            let name = p["name"].as_str().unwrap_or("");
            if name == "floor" || name == "lintel" {
                continue; // the floor IS the ground; the lintel spans the doorway
            }
            let y = p["position"][1].as_f64().unwrap();
            let h = p["scale"][1].as_f64().unwrap();
            assert!(
                (y - h / 2.0).abs() < 0.05,
                "{name} floats or is buried: y={y}, height={h} (expected y≈{})",
                h / 2.0
            );
            standing += 1;
        }
        assert!(standing >= 9, "expected the pillars and the table, got {standing}");
    }

    #[test]
    fn the_room_is_human_sized() {
        let world = template_world("test-room", "Test Room");
        let by_name = |n: &str| -> f64 {
            world["placements"].as_array().unwrap().iter()
                .find(|p| p["name"].as_str() == Some(n))
                .map(|p| p["scale"][1].as_f64().unwrap()).unwrap()
        };
        let table = by_name("table");
        assert!((0.6..=0.9).contains(&table), "a table you could stand at, not vault: {table}m");
        let pillar = by_name("pillar-1");
        assert!((2.4..=4.0).contains(&pillar), "a pillar, not a tower: {pillar}m");
    }

    #[test]
    fn a_hosted_room_is_never_left_solo() {
        // The three shapes a designer can hand back.
        let mut omitted = json!({ "world": { "id": "a" } });
        ensure_relay_presence(&mut omitted);
        assert_eq!(omitted["presence"]["mode"], "relay");

        let mut stated_solo = json!({ "world": { "id": "a" }, "presence": { "mode": "solo", "voice": false } });
        ensure_relay_presence(&mut stated_solo);
        assert_eq!(stated_solo["presence"]["mode"], "relay", "stated solo must still be upgraded");
        assert_eq!(stated_solo["presence"]["voice"], true);

        // An author who named their own relay keeps it.
        let mut own = json!({ "presence": { "mode": "relay", "relays": ["wss://elsewhere.example"], "voice": true } });
        ensure_relay_presence(&mut own);
        assert_eq!(own["presence"]["relays"][0], "wss://elsewhere.example");

        // An empty list is a gap, not a choice.
        let mut empty = json!({ "presence": { "mode": "relay", "relays": [] } });
        ensure_relay_presence(&mut empty);
        assert!(empty["presence"]["relays"][0].as_str().unwrap().starts_with("wss://"));
    }

    #[test]
    fn slugs_are_addresses() {
        assert_eq!(slugify("Monday Standup!"), "monday-standup");
        assert_eq!(slugify("  spaced  out  "), "spaced-out");
    }

    #[test]
    fn roster_follows_the_relay() {
        let mut roster = BTreeMap::new();
        assert!(apply_presence(
            r#"{"t":"welcome","id":1,"occupants":[{"id":2,"name":"ada"}]}"#,
            &mut roster
        ));
        assert_eq!(roster.len(), 1);
        assert!(apply_presence(r#"{"t":"join","id":3,"name":"linus"}"#, &mut roster));
        assert_eq!(roster.len(), 2);
        assert!(apply_presence(r#"{"t":"leave","id":2}"#, &mut roster));
        assert_eq!(roster.values().cloned().collect::<Vec<_>>(), vec!["linus"]);
        // A pose storm must not churn the bar.
        assert!(!apply_presence(r#"{"t":"pose","id":3}"#, &mut roster));
    }

    #[test]
    fn claims_read_without_verifying() {
        // {"sub":"did:pixygon:ada","name":"Ada"} as an unsigned JWT body.
        let token = "eyJhbGciOiJub25lIn0.eyJzdWIiOiJkaWQ6cGl4eWdvbjphZGEiLCJuYW1lIjoiQWRhIn0.";
        let claims = passport_claims(token);
        assert_eq!(claims.get("name").and_then(Value::as_str), Some("Ada"));
        assert_eq!(claims.get("sub").and_then(Value::as_str), Some("did:pixygon:ada"));
    }

    #[test]
    fn an_opaque_token_is_simply_anonymous() {
        assert_eq!(passport_claims("not-a-jwt"), json!({}));
    }

    #[test]
    fn unnamed_travelers_still_count() {
        let mut roster = BTreeMap::new();
        apply_presence(r#"{"t":"join","id":9}"#, &mut roster);
        assert_eq!(roster.get(&9).map(String::as_str), Some("traveler 9"));
    }
}
