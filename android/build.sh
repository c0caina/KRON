#!/usr/bin/env bash
# Build kron.apk from an aarch64-linux-android kron binary, without Gradle:
#   android/build.sh target/aarch64-linux-android/release/kron kron.apk ignition-files ignition-files
# Needs ANDROID_HOME with platforms;android-35 and build-tools;35.0.0, and a JDK.
# Signs with $KEYSTORE / $KEYSTORE_PASS if set, else with android/debug.keystore.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
bin=$(realpath "$1")
out=$(realpath -m "$2")
bt="$ANDROID_HOME/build-tools/35.0.0"
jar="$ANDROID_HOME/platforms/android-35/android.jar"
b="$here/build"
rm -rf "$b" && mkdir -p "$b/gen" "$b/classes" "$b/dex" "$b/apk/assets/runtime"

"$bt/aapt2" compile --dir "$here/res" -o "$b/res.zip"
"$bt/aapt2" link -I "$jar" --manifest "$here/AndroidManifest.xml" -o "$b/base.apk" "$b/res.zip" \
  --java "$b/gen" --min-sdk-version 28 --target-sdk-version 34 \
  --version-code "${VERSION_CODE:-1}" --version-name "${VERSION_NAME:-dev}"
javac -nowarn --release 11 -classpath "$jar" -d "$b/classes" \
  $(find "$here/src" "$b/gen" -name '*.java')
"$bt/d8" --lib "$jar" --min-api 28 --release --output "$b/dex" $(find "$b/classes" -name '*.class')

# Assets are first-run seeds, copied to editable files and executed with root.
cp "$b/base.apk" "$b/unsigned.apk"
cp "$b/dex/classes.dex" "$b/apk/"
cp -R "$here/../runtime/." "$b/apk/assets/runtime/"
cp "$bin" "$b/apk/assets/runtime/kron"
cp -R "${3:?pass the directory prepared by android/ignition.sh}/." "$b/apk/assets/runtime/"
tar -czf "$b/apk/assets/runtime/source.tar.gz" -C "$here/.." \
  Cargo.toml Cargo.lock rustfmt.toml src runtime android/src android/res \
  android/AndroidManifest.xml android/build.sh android/ignition.sh README.md AGENTS.md THIRD_PARTY_NOTICES.md
(cd "$b/apk" && zip -0qr "$b/unsigned.apk" classes.dex assets)
"$bt/zipalign" -p -f 4 "$b/unsigned.apk" "$b/aligned.apk"
"$bt/apksigner" sign --ks "${KEYSTORE:-$here/debug.keystore}" --ks-pass "pass:${KEYSTORE_PASS:-android}" \
  --out "$out" "$b/aligned.apk"
echo "built $out"
