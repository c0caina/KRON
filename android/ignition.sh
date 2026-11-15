#!/usr/bin/env bash
# Prepare pinned, redistributable Android inference files. No runtime downloader.
set -euo pipefail
out=$(realpath -m "$1")
mkdir -p "$out/llama"
curl -fL --retry 3 https://github.com/ggml-org/llama.cpp/releases/download/b11500/llama-b11500-bin-android-arm64.tar.gz -o "$out/llama.tar.gz"
printf '%s  %s\n' b9655ef55090bed8ba46f8a20ac22279b1519f20570b500540fee979f36c8bd8 "$out/llama.tar.gz" | sha256sum -c -
tar -xzf "$out/llama.tar.gz" -C "$out/llama" --strip-components=1 --wildcards '*/llama-server' '*/*.so' '*/LICENSE'
rm "$out/llama.tar.gz"
if [ -n "${ANDROID_NDK_LATEST_HOME:-}" ]; then
  "$ANDROID_NDK_LATEST_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip" "$out/llama/llama-server" "$out/llama/"*.so
fi
base=https://huggingface.co/LiquidAI/LFM2.5-2.6B-GGUF/resolve/e7caca5d835a3901a8e0d63e94009429bafafdfc
curl -fL --retry 3 "$base/LFM2.5-2.6B-Q4_K_M.gguf" -o "$out/ignition.gguf"
printf '%s  %s\n' 02a8b7e17487d326e46d68ce0ba24211e1b80a14c4cd0597fa73c1cd697f52ed "$out/ignition.gguf" | sha256sum -c -
curl -fL --retry 3 "$base/LICENSE" -o "$out/LFM-LICENSE"
