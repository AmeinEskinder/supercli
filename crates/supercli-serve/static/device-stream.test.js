/**
 * device-stream.test.js — unit tests for the wire-format decoder in
 * device-stream.js. Run: node device-stream.test.js
 *
 * Uses synthetic frames only; no browser, no devices.
 */
"use strict";

const assert = require("node:assert/strict");
const { WireDecoder, encodeWireFrame } = require("./device-stream.js");

let passed = 0;
function test(name, fn) {
  try {
    fn();
    passed++;
    console.log("ok - " + name);
  } catch (err) {
    console.error("FAIL - " + name + ": " + err.message);
    process.exitCode = 1;
  }
}

test("decodes a single description frame", () => {
  const frames = [];
  const d = new WireDecoder((f) => frames.push(f));
  const payload = Buffer.from(JSON.stringify({ width: 1080, height: 2400 }));
  d.push(encodeWireFrame(0x01, payload));
  assert.equal(frames.length, 1);
  assert.equal(frames[0].type, 0x01);
  assert.deepEqual(Buffer.from(frames[0].payload), payload);
});

test("decodes all four frame types", () => {
  const frames = [];
  const d = new WireDecoder((f) => frames.push(f));
  for (const t of [0x01, 0x02, 0x03, 0x04]) {
    d.push(encodeWireFrame(t, new Uint8Array([t, t + 1])));
  }
  assert.deepEqual(frames.map((f) => f.type), [0x01, 0x02, 0x03, 0x04]);
});

test("handles frames split across pushes (partial delivery)", () => {
  const frames = [];
  const d = new WireDecoder((f) => frames.push(f));
  const full = encodeWireFrame(0x02, new Uint8Array([1, 2, 3, 4, 5]));
  d.push(full.slice(0, 2));
  assert.equal(frames.length, 0);
  d.push(full.slice(2, 6));
  assert.equal(frames.length, 0);
  d.push(full.slice(6));
  assert.equal(frames.length, 1);
  assert.equal(frames[0].type, 0x02);
});

test("handles multiple frames in one push", () => {
  const frames = [];
  const d = new WireDecoder((f) => frames.push(f));
  const a = encodeWireFrame(0x02, new Uint8Array([9]));
  const b = encodeWireFrame(0x03, new Uint8Array([8, 7]));
  const merged = new Uint8Array(a.length + b.length);
  merged.set(a, 0);
  merged.set(b, a.length);
  d.push(merged);
  assert.equal(frames.length, 2);
  assert.equal(frames[0].type, 0x02);
  assert.equal(frames[1].type, 0x03);
});

test("handles empty payload", () => {
  const frames = [];
  const d = new WireDecoder((f) => frames.push(f));
  d.push(encodeWireFrame(0x03, new Uint8Array(0)));
  assert.equal(frames.length, 1);
  assert.equal(frames[0].payload.length, 0);
});

test("handles large payload (64KB H.264 packet)", () => {
  const frames = [];
  const d = new WireDecoder((f) => frames.push(f));
  const payload = new Uint8Array(65536).fill(0xab);
  d.push(encodeWireFrame(0x02, payload));
  assert.equal(frames.length, 1);
  assert.equal(frames[0].payload.length, 65536);
});

test("rejects absurd frame length", () => {
  const d = new WireDecoder(() => {});
  const bad = new Uint8Array([0x02, 0xff, 0xff, 0xff, 0xff]);
  assert.throws(() => d.push(bad), /out of range/);
});

test("frame type constants match docs/device.md §9.3", () => {
  assert.equal(WireDecoder.WIRE_DESCRIPTION, 0x01);
  assert.equal(WireDecoder.WIRE_KEYFRAME, 0x02);
  assert.equal(WireDecoder.WIRE_DELTA, 0x03);
  assert.equal(WireDecoder.WIRE_JPEG_SEED, 0x04);
});

test("roundtrip preserves binary payload bytes", () => {
  const frames = [];
  const d = new WireDecoder((f) => frames.push(f));
  const payload = new Uint8Array(256);
  for (let i = 0; i < 256; i++) payload[i] = i;
  d.push(encodeWireFrame(0x02, payload));
  assert.deepEqual(Array.from(frames[0].payload), Array.from(payload));
});

console.log(`\n${passed} tests passed`);
