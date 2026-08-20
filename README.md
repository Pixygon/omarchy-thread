# Thread — a room on your bar

[![helper](https://github.com/Pixygon/omarchy-thread/actions/workflows/ci.yml/badge.svg)](https://github.com/Pixygon/omarchy-thread/actions/workflows/ci.yml)

Open a 3D room, served off your own machine, and see who walks into it from the
Omarchy bar.

```
◈ 2
```

That is two people standing in a place you are hosting. Click the doorway to
open a room, copy its address, and paste it to somebody. They walk in. Nobody
signed up for anything, and nothing you made left your disk.

## What it actually is

The [Thread](https://github.com/Pixygon/thread-spec) is an open spatial medium:
addressable 3D worlds that link to one another the way pages do. A world is a
`world.json` file. A host publishes it at `/.well-known/thread/world.json`. Any
conformant browser can walk in. That is the whole protocol.

This plugin makes your desktop one of those hosts.

- **A room is a directory.** `~/Worlds/monday-standup/world.json` — a floor, a
  ring of pillars, a table, four places to stand. Copy it to a USB stick if you
  like; it is yours, meshes and all. If you happen to have the Thread CLI
  installed, rooms are designed by its level creator instead — real columns and
  arches, meshed onto your own disk under `models/` and referenced by relative
  path, so the room owes nothing to anyone's CDN. If you don't have it, the
  built-in room is used and nothing about the plugin changes. It is an upgrade,
  never a dependency.
- **Serving it is one command.** The helper serves the manifest with the
  content type and CORS header the spec requires, on your LAN address.
- **The bar watches the room, without entering it.** It *observes* the presence
  relay: a roster and join/leave events, no body, no pose. Your bar never
  appears inside your world as a phantom. (That mode was added to the reference
  relay and the wire spec for this plugin — see
  [presence-wire-v0.1](https://github.com/Pixygon/thread-spec/blob/main/specs/presence-wire-v0.1.md).)
- **If you own a domain, it becomes a real address on the internet.**
  `omarchy-thread publish you.com` stages the three files; `verify` then checks
  the live host exactly the way a browser would and tells you the truth.

## Try it in thirty seconds

```bash
omarchy-thread new "Monday Standup"   # a room of your own
omarchy-thread open                   # serve it, and step inside
omarchy-thread invite                 # thread://192.168.1.20:7777 — paste this to someone
```

Anyone on your network opens that address in a Thread browser and is standing
in the room with you. For people who aren't on your network, put it on a domain:

```bash
omarchy-thread publish you.com
rsync -av ~/Worlds/monday-standup/.publish/ you@you.com:/var/www/you.com/.well-known/thread/
omarchy-thread verify you.com
#  ✓ reachable  ✓ CORS  ✓ content-type  ✓ a conformant thread/0.1 world
#  ◈ thread://you.com is live on the Thread.
```

## Install

**The plugin** (QML, no build step):

```bash
git clone https://github.com/Pixygon/omarchy-thread ~/.config/omarchy/plugins/io.pixygon.thread
```

Then add the **Thread** widget to your bar in Omarchy's bar settings. The
plugin reloads itself on save, like every Omarchy plugin.

**The helper** (`omarchy-thread`), which does the serving and the watching:

```bash
cd ~/.config/omarchy/plugins/io.pixygon.thread/helper
cargo build --release
install -Dm755 target/release/omarchy-thread ~/.local/bin/omarchy-thread
```

It is ~700 lines of Rust with four dependencies, all from crates.io. Read it
before you run it; it is short on purpose.

The room assumes a builtin mesh is one unit tall and centred — true in the
reference browser from Infinite v0.111.0 onward. On an older build the pillars
render at double height and half-sunk, which is a browser to update rather than
a world to edit.

**A browser to walk in with** — [Infinite](https://thread.pixygon.io/browser)
is the reference one. The plugin works without it (it will still serve rooms
and show you who is inside), it just can't open the door for you.

## What it needs, and what it touches

Documented plainly, because you should know before installing anything:

| | |
|---|---|
| **Binaries it runs** | `omarchy-thread` (this repo), `wl-copy` (copying the address), `infinite` (only if installed, to open a room), `curl` (only in `verify`) |
| **Network in** | An HTTP listener on port 7777+ bound to `0.0.0.0`, serving *only* the room directory, while a room is open. Nothing listens when no room is open. |
| **Network out** | One WebSocket to the presence relay in your world's manifest (default `wss://relay.pixygon.io`), to learn who is inside. Set `OMARCHY_THREAD_RELAY` to point at your own, or delete `presence.relays` from the world to run with no relay at all. |
| **Files it writes** | `~/Worlds/` (your rooms) and `~/.local/state/omarchy-thread/state.json` (what the bar reads) |
| **Privileges** | None. No root, no polkit, no system config, no pacman repo, no autostart. |
| **Accounts** | None required. The relay accepts anonymous travelers; a Passport is optional and only makes you recognisable instead of "traveler 7". |

## Identity, and thread:// links

The plugin is not a renderer — [Infinite](https://thread.pixygon.io/browser) is
the browser, the way Firefox is for the web. What the plugin does is make this
desktop a first-class citizen of the Thread:

```bash
omarchy-thread handler install   # thread:// links open a world, anywhere on the desktop
omarchy-thread passport status   # who you are, if you want to be anyone
```

**Anonymous is the default and always works** — the relay takes travelers with
no identity at all. A Passport is the portable "you" (a name, an avatar, and
consent) that makes you recognisable across worlds instead of "traveler 7". If
you have one, `omarchy-thread passport set <token>` writes it to
`~/.config/infinite/passport.token` — the same file the browser already reads,
so signing in once signs you in everywhere. `passport clear` makes you a
stranger again.

`handler install` is the only thing here that touches your desktop's settings
(the `x-scheme-handler/thread` default), it is never done for you, and
`handler remove` undoes it.

## Configuration

| Variable | Default | |
|---|---|---|
| `OMARCHY_THREAD_ROOMS` | `~/Worlds` | where your rooms live |
| `OMARCHY_THREAD_PORT` | `7777` | first port tried when serving |
| `OMARCHY_THREAD_RELAY` | `wss://relay.pixygon.io` | the relay the bar observes |
| `OMARCHY_THREAD_NAME` | `$USER` | the name co-travelers see |
| `OMARCHY_THREAD_FIGURE` | `hall` | which figure the level creator builds, when present |
| `OMARCHY_THREAD_NO_CLI` | unset | set it to always use the built-in room |
| `OMARCHY_THREAD_STORE` | unset | set it to source models from the online store instead of meshing them locally |

## Keys, while the panel is open

`o` step inside · `c` copy the address · `n` new room · `x` close the room

Middle-clicking the bar glyph walks straight in.

## The helper

```
omarchy-thread new <name>            make a room you own
omarchy-thread rooms                 list your rooms
omarchy-thread validate [room|file]  check a world: conformance, presence tier, missing meshes
omarchy-thread open [room]           serve it and step inside
omarchy-thread invite [room]         the address to hand someone
omarchy-thread status                one line of JSON — what the bar reads
omarchy-thread publish <host> [room] stage the room for a domain you own
omarchy-thread verify <host>         check a live host the way a browser does
omarchy-thread stop                  close the room
omarchy-thread doctor                what's installed, what's reachable
```

`validate` refuses a room that names meshes which aren't beside it, and
`publish` won't stage one — a manifest missing its assets is still valid JSON,
so the failure is otherwise silent and permanent: the room simply loads with
holes in it, every time. (Conformance clause C8.)

`verify` is not Pixygon-specific and has no allegiance: point it at anybody's
domain and it will tell you whether they are conformant.

## Honest limits

- **v0.1.** The room template is one room. Editing a world means editing JSON
  (or using the Thread CLI's authoring tools) — a visual editor is being built,
  and is not here yet.
- **Your LAN, or your domain.** There is no tunnel and no hosted fallback: if
  the people you want to invite are not on your network, you need a domain (or
  a tunnel of your own, e.g. Tailscale or `cloudflared`). That is a deliberate
  omission — this plugin does not want to be a service you depend on.
- **Voice is declared, not implemented here.** The manifest advertises it; the
  browser does the talking.
- **The relay is a default, not a requirement.** Anyone can run one; the wire
  protocol is specified and the reference implementation is open.

## Licence

MIT. The Thread specification lives at
[Pixygon/thread-spec](https://github.com/Pixygon/thread-spec) and is
independent of this plugin — the format, the resolution rule and the presence
wire are the standard, not a product.
