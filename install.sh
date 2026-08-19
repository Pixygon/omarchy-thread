#!/usr/bin/env bash
# Build and install the helper the plugin runs. Nothing here needs root, and
# nothing outside your home directory is touched.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
prefix="${PREFIX:-$HOME/.local}"

command -v cargo >/dev/null || { echo "This needs Rust: pacman -S rust"; exit 1; }

echo "building the helper…"
cargo build --release --manifest-path "$here/helper/Cargo.toml"
install -Dm755 "$here/helper/target/release/omarchy-thread" "$prefix/bin/omarchy-thread"

echo "✓ installed $prefix/bin/omarchy-thread"
case ":$PATH:" in
  *":$prefix/bin:"*) ;;
  *) echo "  note: $prefix/bin is not on your PATH" ;;
esac

if [ "$here" != "$HOME/.config/omarchy/plugins/io.pixygon.thread" ]; then
  echo
  echo "To load the plugin, put this directory where Omarchy looks:"
  echo "  ln -s '$here' ~/.config/omarchy/plugins/io.pixygon.thread"
fi
echo "Then add the Thread widget to your bar."
