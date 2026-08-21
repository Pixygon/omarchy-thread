# Testing this on an Omarchy machine

Everything below has been verified on a Linux box that is **not** running
Omarchy — the room, the serving, the relay connection and the conformance pass
are all real.

The **QML has still never been rendered** — Quickshell needs a compositor — but
it is no longer unchecked. It was validated against the Omarchy source itself:

- `omarchy-plugin-validate` — the official checker, the same rules the shell
  applies at load time — **passes, exit 0**.
- Every component it uses resolves in `shell/Ui/`.
- Every property it sets exists on that component or a base of it, checked by
  reading the declarations rather than by memory.
- `fittedContentWidth`/`fittedContentHeight` exist and take the arity used;
  `closeRequested` and `textKey` are real signals on `PanelKeyCatcher`.
- The manifest is structurally identical to the shipped `omarchy.tailscale`
  one, and the panel's top-level shape matches it too.

That found one real bug, now fixed: four tooltips were nested `PanelToolTip`s
bound to `containsMouse`, which is public on `ToggleSwitch` and **not** on
`PanelActionButton` — so they were bound to nothing and would simply never have
appeared. `PanelActionButton` renders its own tooltip from `tooltipText`.

So: steps 1–3 should just work, and step 4 is still the real test — but the
failure modes left are the ones no static check can reach.

## 0. Install

```bash
git clone https://github.com/Pixygon/omarchy-thread && cd omarchy-thread
./install.sh
```

That builds the helper into `~/.local/bin` **and links the plugin into
`~/.config/omarchy/plugins/`** — no second command. It refuses to touch a link
that already points somewhere else.

```bash
omarchy-thread doctor
```

Expect: rooms `0`, a relay, your LAN address, and `browser` either pointing at
an Infinite binary or saying it is not installed. Both are fine.

## 1. A room of your own

```bash
omarchy-thread new "Saturday Test"
omarchy-thread rooms
```

A room is a directory in `~/Worlds`. Nothing is hidden; you can read it, copy
it to a USB stick, or delete it.

Worth checking, because it is the thing that has to be true:

```bash
thread validate ~/Worlds/saturday-test/world.json
thread lint     ~/Worlds/saturday-test/world.json
```

Both were clean here — 22 placements, a portal, and `presence.mode: relay`.

## 2. Serve it

```bash
omarchy-thread serve saturday-test &
omarchy-thread status
```

Look for **`"relay_connected": true`** and an `invite` like
`thread://192.168.1.x:7778`. Then, from anywhere that can see the machine:

```bash
thread-conformance --live 192.168.1.x:7778
```

Verified here: **`✓ CONFORMANT — walkable anywhere`**. If that passes, a
stranger's browser can walk into a world running on your laptop, which is the
entire point of the thing.

## 3. Walk into it

```bash
omarchy-thread open saturday-test
```

The plugin looks for `infinite` first and `infinite-wgpu` second — a release
tarball only contains the second, and the Arch package now provides both. If
`doctor` says the browser is not installed while it plainly is, that is a bug
in the detection and worth reporting.

## 4. The part that has never run: the bar widget

Add the **Thread** widget to your bar in Omarchy's plugin settings.

What should happen:

| | |
|---|---|
| Bar, no room open | a dim `◈` |
| Bar, room open, empty | `◈` brighter |
| Bar, someone inside | `◈ 1` |
| Click | the panel — room, address, who is inside |
| Middle-click | steps straight into the room |
| In the panel | `o` open · `c` copy the address · `n` new room · `x` close |

**If it fails, the useful thing to capture is the Quickshell log**, not a
screenshot — QML errors are legible and name the line:

```bash
journalctl --user -u quickshell -n 80 --no-pager      # or however your bar runs
qs log 2>&1 | tail -40
```

Send me the first error line. Every plausible failure is a component name I got
wrong from the plugin docs, or a property that does not exist on this version of
`Panel`/`BarIconButton` — both are one-line fixes once the message names them.

## 4b. While you are there: the competition screenshot

The plugin has no `preview.png`, deliberately — I cannot render the QML, and a
mocked-up one would be a picture of something that has never existed. The
moment step 4 works, that picture is ten seconds away and it is a real one:

```bash
# whole bar, so the widget is shown in context
grim -g "$(slurp)" preview.png     # or your screenshot tool of choice
```

Two shots are worth having: the bar with `◈ 1` in it, and the panel open with
somebody's name in the roster. The second is the one that explains the plugin
to a stranger in one glance.

## 5. Two people

The real proof, and the thing no single machine can test:

1. Serve a room here, `omarchy-thread invite` for the address.
2. Someone else opens it in Infinite.
3. The bar count goes to `1` and their name appears in the panel.

The relay is `wss://relay.pixygon.io` and it passes `thread-conformance
--relay`. Every world in the Pixygon estate now names a relay, so any of them
works as a fallback venue if your own room misbehaves.

## What is known-broken or unverified

- **The QML has never been rendered.** See above.
- **No macOS build of the browser**, so a Mac guest cannot join yet.
- `thread://` as a clickable protocol handler is installed by
  `scripts/install-thread-handler.sh`; that path is also untested on Omarchy.
