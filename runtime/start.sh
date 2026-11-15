#!/system/bin/sh
umask 022
# Runs as root. This file, the binary, UI and prompt are all editable.
KRON_RUNTIME=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export KRON_RUNTIME
export KRON_CONFIG="${KRON_CONFIG:-$KRON_RUNTIME/../kron.json}"
export KRON_LISTEN="${KRON_LISTEN:-127.0.0.1:7790}"
printf '%s\n' "$$" > "$KRON_RUNTIME/pid"
cd "$KRON_RUNTIME/.." || exit 1
exec "$KRON_RUNTIME/kron"
