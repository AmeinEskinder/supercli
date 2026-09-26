# Vendored JavaScript

## jsqr-1.4.0.js

A QR code decoder for the pairing camera scanner, vendored from the
published npm package `jsqr@1.4.0` so the mobile launcher works offline.

- Upstream: https://github.com/cozmo/jsQR
- License: Apache License 2.0
- Copyright: (c) 2017 Cosmin Mihai Serbanescu

The file is byte-identical to the npm `dist/jsQR.js` build. It is loaded
once at startup via `document::eval` (see `QR_BRIDGE_INSTALL_JS` in
`src/main.rs`) and exposes the global `jsQR`. It is not modified.
