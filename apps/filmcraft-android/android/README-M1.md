# FilmCraft on Android — M1 packaging notes

Provisional identity (NOT final): package `com.taras.filmcraft`, working label
"FilmCraft". The ArtCraft name/logos are not used anywhere in this shell.

## Layout

- `../Cargo.toml` — the `filmcraft-android` crate (`[lib] name = "main"`, cdylib) plus
  the cargo-apk2 `[package.metadata.android]` table (the manifest is generated from it).
- `AndroidManifest.xml` — the Gradle build's manifest (same values as the TOML table).
- `app/` — the Gradle shell (`build.gradle`, `settings.gradle`, `gradle.properties`,
  `app/build.gradle`, `app/proguard-rules.pro`, `app/src/main/java/.../MainActivity.java`,
  `app/src/main/res/...`).
- `MainActivity.java` — M1 minimal: extends `GameActivity`, nothing else. SAF pickers,
  text input and any other Java bridge are M2 work and land here as methods.

## Build the native library (works in this VM)

```sh
export ANDROID_HOME="$HOME/android-sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android35-clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
# ring's build script (via cc-rs) looks for the UNVERSIONED compiler name, which the
# NDK does not ship — point cc-rs at the versioned wrapper explicitly:
export CC_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android35-clang"
export CXX_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android35-clang++"
export AR_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
export CARGO_BUILD_JOBS=2   # 8 GB RAM VM; more jobs OOM the build
cd <filmcraft-clone>
cargo ndk -t arm64-v8a -o apps/filmcraft-android/jniLibs build -p filmcraft-android
# debug .so: apps/filmcraft-android/jniLibs/arm64-v8a/libmain.so
# release: add `--release` (lto=thin, codegen-units=1; ~6 min, .so ~45 MB)
```

16 KB page alignment (mandatory for targetSdk 35+): NDK r27c does not default to it,
so pass the linker flag via
`CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"`
and verify with `llvm-readelf -lW <so> | grep LOAD` (Align must be `0x4000`).
NDK r28+ defaults to 16 KB; consider upgrading when the CI workflow is written.

## APK via Gradle (CI / a machine with Maven access)

```sh
cd apps/filmcraft-android/android
# Gradle 9.3.1 + AGP 9.1.0, JDK 17+ (download the Gradle distribution first;
# the wrapper jar is intentionally not committed)
gradle assembleDebug    # or assembleRelease (debug-signed, installs with adb)
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n com.taras.filmcraft/.MainActivity
adb logcat | grep -Ei "filmcraft|rust|wgpu|vulkan|AndroidRuntime"
```

Known VM trap: a full Gradle build does NOT run in this sandbox VM — the runtime
blocks direct TCP from Java processes, so the Gradle client cannot talk to its
daemon and cannot fetch AGP/dependencies from Maven. That is an environment limit,
not a project failure; use cargo-apk2 locally and Gradle in CI.

## APK via cargo-apk2 (works in this VM)

cargo-apk2 (1.5.0) builds the APK without Gradle, but its javac classpath is
`android.jar` only — so `MainActivity extends GameActivity` cannot be compiled by
it: the `GameActivity` class lives in the `androidx.games:games-activity` AAR,
which cargo-apk2 does not fetch. Workaround (M1):

1. Download the AARs (curl works in this VM):
   `https://maven.google.com/androidx/games/games-activity/4.4.0/games-activity-4.4.0.aar`
   and its compile-time dependency
   `https://maven.google.com/androidx/annotation/annotation/1.9.1/annotation-1.9.1.aar`
   (verify the versions against the AAR's POM; annotation is needed because
   `GameActivity`'s public API uses its annotations).
2. Extract each AAR's `classes.jar` and unzip the classes into one directory.
3. Compile the activity against them:
   `javac -cp <android.jar>:<games-classes>:<annotation-classes> -d <classes-dir> android/app/src/main/java/com/taras/filmcraft/MainActivity.java`
   where `<android.jar>` is `$ANDROID_HOME/platforms/android-35/android.jar` and
   `<classes-dir>` is `<clone>/target/<profile>/apk/classes`
   (cargo-apk2's stable classes directory: `<target-dir>/<profile>/apk/classes`).
4. Run cargo-apk2 WITHOUT `java_sources` declared (it is intentionally absent from
   `Cargo.toml`, so cargo-apk2 skips its own javac step) — it dexes whatever is
   already in `classes_dir`:
   `cargo apk2 build -p filmcraft-android --target aarch64-linux-android`
   (debug; add `--release` for the release APK, which needs a keystore — see
   `cargo apk2 build --help` and `CARGO_APK_RELEASE_KEYSTORE`).
5. Verify: `aapt2 dump badging <apk>` (package, versionCode, targetSdk, label),
   `apksigner verify --print-certs <apk>`, `unzip -l <apk>` (lib/arm64-v8a/libmain.so),
   `zipalign -c -P 16 4 <apk>`.

### GameActivity needs ~12 AARs, not 2 — use NativeActivity for the local APK

Step 1–3 above turned out to be a dead end for a local build: `GameActivity
extends androidx.appcompat.app.AppCompatActivity`, so its compile-time tree is
appcompat 1.7.0 + core + annotation + fragment + lifecycle + … (~12 AARs), and at
runtime it needs an `Theme.AppCompat` theme, i.e. the appcompat *resources* merged
into the APK — a job for Gradle, not a manual pre-seed. The local M1 APK therefore
uses **NativeActivity** (`android.app.NativeActivity`, eframe feature
`android-native-activity`, no Java at all). The two backends are mutually
exclusive in `android-activity` (`compile_error!` if both features are on), so the
flip is temporary: flip the eframe feature and the manifest activity in
`Cargo.toml`, build the APK, flip both back to GameActivity. The `.so` differs
between the two variants only in the winit backend glue; engine and UI are
identical.

### `accesskit` does not compile with NativeActivity

`filmcraft-ui-egui` enabled eframe's `accesskit` feature unconditionally, and the
eframe fork refuses `accesskit` + `android-native-activity` on Android
(`compile_error!` in eframe's `lib.rs`, citing the `accesskit_winit` Android
limitation). Fix (in `crates/ui-egui/Cargo.toml`, the one upstream edit M1 needed):
split the eframe dependency by target — non-Android keeps `accesskit`, Android
drops it (keeps `clipboard`, which egui-winit cfg-gates out on Android anyway).
The egui `accesskit` types the UI code calls (`accesskit_node_builder`,
`has_accesskit_node`, `egui::accesskit::Role`) are unconditional in egui itself,
so they still compile; they just build a tree nobody consumes on Android.

### cargo-apk2 overrides the linker and rustflags — use global `RUSTFLAGS`

ndk-build2 (cargo-apk2's backend) sets `CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER`
to the NDK's plain `clang` with `--target=aarch64-linux-android<minSdk>` and
builds `CARGO_ENCODED_RUSTFLAGS` from the **global** `RUSTFLAGS` env var only —
the target-specific `CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS` is ignored.
Consequence found the hard way: the first local APK's `.so` had 4 KB LOAD
alignment (`0x1000`) despite the target-specific flag, which is non-compliant for
targetSdk 35 on 16 KB-page devices. For `cargo apk2` runs, export the flag
globally instead (it also reaches host rustc invocations, where GNU ld accepts it
harmlessly):

```sh
export RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
unset CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS
cargo apk2 build -p filmcraft-android --target aarch64-linux-android
```

Verify the built `.so` afterwards: `llvm-readelf -lW <so>` must show `0x4000` on
every LOAD segment. (`cargo ndk` does NOT do this override — the target-specific
var works there, and the GameActivity `.so` built with it is already `0x4000`.)
`cargo apk2` also needs `JAVA_HOME` set (it fails fast without it) and writes the
APK to `<clone>/target/debug/apk/main.apk` (note: the *host* target dir), signed
with an auto-generated debug keystore at `~/.android/debug.keystore`.

## Phone test (the M1 gate — run by Diane, not by this VM)

1. `adb install -r <apk>` (uninstall any older `com.taras.filmcraft` first if the
   signature changed).
2. Open the app. Expected: the FilmCraft UI renders (menu bar, Project panel,
   timeline); logcat shows `filmcraft-android: android_main entered`,
   `filmcraft-android: demo project opened`, and no `AndroidRuntime` crash.
3. If the screen stays black or the app exits at once, capture
   `adb logcat -d | grep -Ei "filmcraft|rust|wgpu|vulkan|AndroidRuntime"` and the
   first ~50 lines after the launch.
4. "Open Demo Project" is already executed at startup in M1, so a loaded demo
   (timeline clips + monitors) is the pass criterion — no file picking needed.
5. Known M1 gaps (by design, M2/M3): no file import/save pickers, no keyboard,
   desktop-sized layout on a phone screen, no audio, software decode only.
