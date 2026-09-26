# supercli-native-bridge

Panic-contained C ABI over `supercli-core` for the Swift Mac app
(`clients/native`). The app links this static library to call backend logic
in-process (state reads/writes, session operations) instead of shelling out
for everything; every entry point catches Rust panics at the FFI boundary so
a backend bug can't take down the app.

Keep this crate a thin translation layer: logic belongs in `supercli-core`, and
anything another client needs must live there (one core, many
clients).
