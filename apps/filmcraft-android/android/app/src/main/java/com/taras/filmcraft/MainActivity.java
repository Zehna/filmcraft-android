package com.taras.filmcraft;

/**
 * The Android entry point (M1 minimal shell).
 *
 * <p>{@code android-activity} launches this class because the manifest names it and the native
 * library is loaded through {@code android.app.lib_name=main}. The Rust side
 * ({@code android_main} in {@code src/lib.rs}) does everything else.
 *
 * <p>M1 deliberately adds nothing here: the Storage Access Framework pickers (import/save),
 * text input and any other Java-side bridge are M2 work and will land as methods on this
 * class, following the pattern of the reference experiment (see
 * {@code android/README-M1.md}).
 */
public class MainActivity extends com.google.androidgamesdk.GameActivity {
}
