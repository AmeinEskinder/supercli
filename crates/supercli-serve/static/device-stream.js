/**
 * device-stream.js — supercli web device streaming client.
 *
 * Speaks the unified wire format (docs/device.md §9.3) over WebSocket:
 *   0x01 description — JSON {device,platform,width_points,height_points,
 *                      width_pixels,height_pixels,density_dpi,orientation}
 *   0x02 keyframe    — H.264 IDR packet
 *   0x03 delta       — H.264 P-frame packet
 *   0x04 JPEG seed   — recovery frame
 *
 * Each wire frame: [1 byte type][4 bytes BE payload len][payload].
 * H.264 packets are fed to WebCodecs VideoDecoder and rendered to a canvas.
 * Pointer events on the canvas are converted to DEVICE-POINT coordinates
 * (not normalized 0-1) and POSTed to /api/devices/<id>/touch.
 *
 * In Node (unit tests) this module exports the pure decoder pieces.
 */
(function (root, factory) {
  if (typeof module !== "undefined" && module.exports) {
    module.exports = factory();
  } else {
    root.DeviceStream = factory();
  }
})(typeof self !== "undefined" ? self : this, function () {
  "use strict";

  const WIRE_DESCRIPTION = 0x01;
  const WIRE_KEYFRAME = 0x02;
  const WIRE_DELTA = 0x03;
  const WIRE_JPEG_SEED = 0x04;

  /**
   * Incremental wire-format decoder. Feed ArrayBuffers/Uint8Arrays; the
   * callback receives {type, payload: Uint8Array} for each complete frame.
   */
  function WireDecoder(onFrame) {
    this.buf = new Uint8Array(0);
    this.onFrame = onFrame;
  }

  WireDecoder.prototype.push = function (chunk) {
    const incoming = chunk instanceof Uint8Array ? chunk : new Uint8Array(chunk);
    const merged = new Uint8Array(this.buf.length + incoming.length);
    merged.set(this.buf, 0);
    merged.set(incoming, this.buf.length);
    this.buf = merged;
    let offset = 0;
    while (this.buf.length - offset >= 5) {
      const type = this.buf[offset];
      const len =
        (this.buf[offset + 1] << 24) |
        (this.buf[offset + 2] << 16) |
        (this.buf[offset + 3] << 8) |
        this.buf[offset + 4];
      if (len < 0 || len > 64 * 1024 * 1024) {
        throw new Error("wire frame length out of range: " + len);
      }
      if (this.buf.length - offset < 5 + len) break; // incomplete
      const payload = this.buf.slice(offset + 5, offset + 5 + len);
      offset += 5 + len;
      this.onFrame({ type: type, payload: payload });
    }
    this.buf = this.buf.slice(offset);
  };

  WireDecoder.WIRE_DESCRIPTION = WIRE_DESCRIPTION;
  WireDecoder.WIRE_KEYFRAME = WIRE_KEYFRAME;
  WireDecoder.WIRE_DELTA = WIRE_DELTA;
  WireDecoder.WIRE_JPEG_SEED = WIRE_JPEG_SEED;

  function encodeWireFrame(type, payload) {
    const out = new Uint8Array(5 + payload.length);
    out[0] = type;
    out[1] = (payload.length >>> 24) & 0xff;
    out[2] = (payload.length >>> 16) & 0xff;
    out[3] = (payload.length >>> 8) & 0xff;
    out[4] = payload.length & 0xff;
    out.set(payload, 5);
    return out;
  }

  /**
   * DeviceStream — one live device view.
   *
   * opts: {
   *   deviceId, canvas, baseUrl (default location.origin),
   *   onDescription(meta), onError(err), onClose(),
   *   lowBandwidth: bool (farm tiles: request reduced fps/bitrate)
   * }
   */
  function DeviceStream(opts) {
    this.deviceId = opts.deviceId;
    this.canvas = opts.canvas;
    this.baseUrl = (opts.baseUrl || (typeof location !== "undefined" ? location.origin : "")).replace(/\/$/, "");
    this.onDescription = opts.onDescription || function () {};
    this.onError = opts.onError || function () {};
    this.onClose = opts.onClose || function () {};
    this.lowBandwidth = !!opts.lowBandwidth;
    this.decoder = null;
    this.ctx = this.canvas.getContext("2d");
    this.decoderQueue = [];
    this.decoding = false;
    this.frameCount = 0;
    this.lastFpsSample = Date.now();
    this.fps = 0;
    this.running = false;
    this.ws = null;
    this.wire = new WireDecoder(this._onWireFrame.bind(this));
  }

  DeviceStream.prototype._wsUrl = function (path) {
    const base = this.baseUrl.replace(/^http/, "ws");
    const q = this.lowBandwidth ? "?fps=15&bitrate=2000000" : "";
    return base + path + q;
  };

  DeviceStream.prototype.start = function () {
    if (this.running) return;
    this.running = true;
    const url = this._wsUrl("/api/devices/stream/" + encodeURIComponent(this.deviceId));
    const ws = new WebSocket(url);
    ws.binaryType = "arraybuffer";
    this.ws = ws;
    const self = this;
    ws.onopen = function () {
      self._initVideoDecoder();
    };
    ws.onmessage = function (ev) {
      try {
        self.wire.push(ev.data);
      } catch (err) {
        self.onError(err);
      }
    };
    ws.onerror = function (ev) {
      self.onError(new Error("websocket error"));
    };
    ws.onclose = function () {
      self.running = false;
      self.onClose();
    };
    this._bindPointerInput();
  };

  DeviceStream.prototype.stop = function () {
    this.running = false;
    if (this.ws) {
      try { this.ws.close(); } catch (e) { /* ignore */ }
      this.ws = null;
    }
    if (this.decoder) {
      try { this.decoder.close(); } catch (e) { /* ignore */ }
      this.decoder = null;
    }
  };

  DeviceStream.prototype._initVideoDecoder = function () {
    if (typeof VideoDecoder === "undefined") {
      this.onError(new Error("WebCodecs VideoDecoder not available in this browser"));
      return;
    }
    const self = this;
    try {
      this.decoder = new VideoDecoder({
        output: function (frame) {
          self._renderFrame(frame);
        },
        error: function (err) {
          self.onError(err);
        },
      });
    } catch (err) {
      this.onError(err);
    }
  };

  DeviceStream.prototype._onWireFrame = function (frame) {
    if (frame.type === WIRE_DESCRIPTION) {
      let meta = {};
      try {
        meta = JSON.parse(new TextDecoder().decode(frame.payload));
      } catch (e) { /* keep {} */ }
      if (meta.width && meta.height) {
        this.canvas.width = meta.width;
        this.canvas.height = meta.height;
      }
      // Save device-point dimensions for touch coordinate conversion.
      // The 0x01 description carries width_points/height_points (device
      // points, not pixels, not normalized). Touch input MUST be in these
      // units (see toDevicePoint below).
      if (meta.width_points && meta.height_points) {
        this.widthPoints = meta.width_points;
        this.heightPoints = meta.height_points;
      }
      this.onDescription(meta);
      return;
    }
    if (frame.type === WIRE_JPEG_SEED) {
      // Recovery frame: draw directly, then expect a fresh keyframe.
      this._renderJpeg(frame.payload);
      return;
    }
    if (frame.type !== WIRE_KEYFRAME && frame.type !== WIRE_DELTA) {
      return; // unknown type: ignore
    }
    if (!this.decoder) return;
    const isKey = frame.type === WIRE_KEYFRAME;
    // H.264 in Annex B or AVCC? The server sends length-prefixed NALUs
    // (AVCC). WebCodecs wants "avc" description on first keyframe.
    const chunk = new EncodedVideoChunk({
      type: isKey ? "key" : "delta",
      timestamp: (this.frameCount * 1e6) / 60,
      data: frame.payload,
    });
    this.frameCount++;
    const self = this;
    this.decoder.decode(chunk);
    // Track fps over 1s windows.
    const now = Date.now();
    if (now - this.lastFpsSample >= 1000) {
      this.fps = Math.round((this.frameCount * 1000) / (now - this.lastFpsSample));
      this.frameCount = 0;
      this.lastFpsSample = now;
      if (this.onFps) this.onFps(this.fps);
    }
  };

  DeviceStream.prototype.configureDecoder = function (meta) {
    // Called with the description metadata once known; configures the codec.
    if (!this.decoder || !meta || !meta.width || !meta.height) return;
    try {
      this.decoder.configure({
        codec: "avc1.640028", // H.264 High profile, level 4.0
        codedWidth: meta.width,
        codedHeight: meta.height,
      });
      this._decoderConfigured = true;
    } catch (err) {
      this.onError(err);
    }
  };

  DeviceStream.prototype._renderFrame = function (frame) {
    try {
      const w = frame.displayWidth || this.canvas.width;
      const h = frame.displayHeight || this.canvas.height;
      if (this.canvas.width !== w || this.canvas.height !== h) {
        this.canvas.width = w;
        this.canvas.height = h;
      }
      this.ctx.drawImage(frame, 0, 0, w, h);
    } finally {
      frame.close();
    }
  };

  DeviceStream.prototype._renderJpeg = function (bytes) {
    const self = this;
    const blob = new Blob([bytes], { type: "image/jpeg" });
    const url = URL.createObjectURL(blob);
    const img = new Image();
    img.onload = function () {
      self.canvas.width = img.width;
      self.canvas.height = img.height;
      self.ctx.drawImage(img, 0, 0);
      URL.revokeObjectURL(url);
    };
    img.src = url;
  };

  // --- Pointer input → DEVICE-POINT coordinates -------------------------
  // NOT normalized 0-1. Device points are the units from the 0x01
  // description's width_points/height_points. The canvas may be scaled
  // by CSS; we map the pointer position to the device's point space.

  DeviceStream.prototype._bindPointerInput = function () {
    const canvas = this.canvas;
    const self = this;
    let activePointer = null;

    function toDevicePoint(ev) {
      const rect = canvas.getBoundingClientRect();
      const nx = (ev.clientX - rect.left) / rect.width;
      const ny = (ev.clientY - rect.top) / rect.height;
      // Clamp normalized to [0,1], then scale to device points.
      const cx = Math.min(1, Math.max(0, nx));
      const cy = Math.min(1, Math.max(0, ny));
      const wp = self.widthPoints || 0;
      const hp = self.heightPoints || 0;
      return {
        x: cx * wp,
        y: cy * hp,
      };
    }

    function postTouch(x, y, action) {
      fetch(
        self.baseUrl + "/api/devices/" + encodeURIComponent(self.deviceId) + "/touch",
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ x: x, y: y, action: action }),
        }
      ).catch(function (err) {
        self.onError(err);
      });
    }

    canvas.style.touchAction = "none";
    canvas.addEventListener("pointerdown", function (ev) {
      if (activePointer !== null) return; // single-touch for now; pinch via buttons
      activePointer = ev.pointerId;
      canvas.setPointerCapture(ev.pointerId);
      const p = toDevicePoint(ev);
      postTouch(p.x, p.y, "down");
      ev.preventDefault();
    });
    canvas.addEventListener("pointermove", function (ev) {
      if (ev.pointerId !== activePointer) return;
      const p = toDevicePoint(ev);
      postTouch(p.x, p.y, "move");
      ev.preventDefault();
    });
    function up(ev) {
      if (ev.pointerId !== activePointer) return;
      activePointer = null;
      const p = toDevicePoint(ev);
      postTouch(p.x, p.y, "up");
      ev.preventDefault();
    }
    canvas.addEventListener("pointerup", up);
    canvas.addEventListener("pointercancel", up);
  };

  DeviceStream.prototype.sendKey = function (keycode) {
    const self = this;
    return fetch(
      this.baseUrl + "/api/devices/" + encodeURIComponent(this.deviceId) + "/key",
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ keycode: keycode }),
      }
    ).catch(function (err) {
      self.onError(err);
    });
  };

  DeviceStream.prototype.sendText = function (text) {
    const self = this;
    return fetch(
      this.baseUrl + "/api/devices/" + encodeURIComponent(this.deviceId) + "/text",
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ text: text }),
      }
    ).catch(function (err) {
      self.onError(err);
    });
  };

  /**
   * LogStream — one text WS frame per log line.
   */
  function LogStream(opts) {
    this.deviceId = opts.deviceId;
    this.baseUrl = (opts.baseUrl || (typeof location !== "undefined" ? location.origin : "")).replace(/\/$/, "");
    this.onLine = opts.onLine || function () {};
    this.onClose = opts.onClose || function () {};
    this.ws = null;
    this.maxLines = opts.maxLines || 500;
  }

  LogStream.prototype.start = function () {
    const url = this.baseUrl.replace(/^http/, "ws") +
      "/api/devices/logs/" + encodeURIComponent(this.deviceId);
    const ws = new WebSocket(url);
    this.ws = ws;
    const self = this;
    ws.onmessage = function (ev) {
      self.onLine(String(ev.data));
    };
    ws.onclose = function () {
      self.onClose();
    };
  };

  LogStream.prototype.stop = function () {
    if (this.ws) {
      try { this.ws.close(); } catch (e) { /* ignore */ }
      this.ws = null;
    }
  };

  return {
    WireDecoder: WireDecoder,
    encodeWireFrame: encodeWireFrame,
    DeviceStream: DeviceStream,
    LogStream: LogStream,
    WIRE_DESCRIPTION: WIRE_DESCRIPTION,
    WIRE_KEYFRAME: WIRE_KEYFRAME,
    WIRE_DELTA: WIRE_DELTA,
    WIRE_JPEG_SEED: WIRE_JPEG_SEED,
  };
});
