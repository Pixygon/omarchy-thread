#!/usr/bin/env bash
# Build and install the helper the plugin runs. Nothing here needs root, and
# nothing outside your home directory is touched.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
prefix="${PREFIX:-$HOME/.local}"
id="io.pixygon.thread"

command -v cargo >/dev/null || { echo "This needs Rust: pacman -S rust"; exit 1; }

echo "building the helper…"
cargo build --release --manifest-path "$here/helper/Cargo.toml"
install -Dm755 "$here/helper/target/release/omarchy-thread" "$prefix/bin/omarchy-thread"

echo "✓ installed $prefix/bin/omarchy-thread"
case ":$PATH:" in
  *":$prefix/bin:"*) ;;
  *) echo "  note: $prefix/bin is not on your PATH" ;;
esac

# Put the plugin where Omarchy looks. Done here rather than printed as an
# instruction, because "now run this one other command" is where an install
# stops being an install. Idempotent, and it never clobbers a link that points
# somewhere else — that would silently swap out someone's own checkout.
plugins="$HOME/.config/omarchy/plugins"
link="$plugins/$id"
if [ "$here" = "$link" ]; then
  :                                   # already living in the right place
elif [ ! -d "$plugins" ]; then
  echo
  echo "No $plugins here — not an Omarchy machine? To load the plugin there:"
  echo "  ln -s '$here' '$link'"
elif [ -L "$link" ] && [ "$(readlink -f "$link")" = "$(readlink -f "$here")" ]; then
  echo "✓ already linked into Omarchy"
elif [ -e "$link" ]; then
  echo "⚠ $link already exists and is not this directory — leaving it alone."
  echo "  Replace it yourself if you meant to: rm '$link' && ln -s '$here' '$link'"
else
  ln -s "$here" "$link" && echo "✓ linked into Omarchy: $link"
fi
echo "Then add the Thread widget to your bar."
