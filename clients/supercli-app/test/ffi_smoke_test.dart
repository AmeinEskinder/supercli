/// FFI smoke test: loads the REAL supercli-client-ffi cdylib and exercises
/// the C ABI surface end to end.
///
/// Requires the `SUPERCLI_FFI_LIB` environment variable pointing at the
/// built library (`cargo build -p supercli-client-ffi --release`). CI sets
/// this; the test is skipped with a clear message when the variable is
/// absent so `dart test` stays green on machines without a Rust toolchain.
library;

import 'dart:io';

import 'package:test/test.dart';

import 'package:supercli_app/native_client.dart';

void main() {
  final ffiLib = Platform.environment['SUPERCLI_FFI_LIB'];

  group('supercli-client-ffi smoke', () {
    test('library loads and ABI version matches', () {
      if (ffiLib == null || ffiLib.isEmpty) {
        markTestSkipped('SUPERCLI_FFI_LIB not set; build the cdylib first');
      }
      // Accessing .lib triggers the ABI check; mismatch throws StateError.
      expect(
        SupercliNativeBindings.abiVersion,
        SupercliNativeBindings.kExpectedAbiVersion,
      );
    });

    test('runtime catalog returns descriptors', () {
      if (ffiLib == null || ffiLib.isEmpty) {
        markTestSkipped('SUPERCLI_FFI_LIB not set');
      }
      final catalog = SupercliNative.runtimeCatalog();
      expect(catalog, isNotEmpty);
      final first = catalog.first;
      expect(first['id'], isA<String>());
      expect(first['slug'], isA<String>());
      expect(first['label'], isA<String>());
    });

    test('runtimeById roundtrip', () {
      if (ffiLib == null || ffiLib.isEmpty) {
        markTestSkipped('SUPERCLI_FFI_LIB not set');
      }
      final catalog = SupercliNative.runtimeCatalog();
      final firstId = catalog.first['id'] as String;
      final byId = SupercliNative.runtimeById(firstId);
      expect(byId, isNotNull);
      expect(byId!['id'], firstId);
      expect(SupercliNative.runtimeById('no-such-runtime'), isNull);
    });

    test('preset + registry + pool pure functions', () {
      if (ffiLib == null || ffiLib.isEmpty) {
        markTestSkipped('SUPERCLI_FFI_LIB not set');
      }
      // Backoff is monotonic in failures.
      final d1 = SupercliNative.poolBackoffDelayMs(1);
      final d3 = SupercliNative.poolBackoffDelayMs(3);
      expect(d1, greaterThan(0));
      expect(d3, greaterThanOrEqualTo(d1));

      final policy = SupercliNative.poolPolicy();
      expect(policy['poll_interval_ms'], isA<int>());

      final slug = SupercliNative.registrySlugify('My Workspace!');
      expect(slug, isNotEmpty);
      expect(slug.contains(' '), isFalse);

      expect(
        SupercliNative.presenceDisplayName(
          device: 'Alice (abc123)',
          ip: '10.0.0.1',
        ),
        contains('Alice'),
      );
    });

    test('presence parse of empty feed', () {
      if (ffiLib == null || ffiLib.isEmpty) {
        markTestSkipped('SUPERCLI_FFI_LIB not set');
      }
      final parsed = SupercliNative.presenceParse([], 'smoke');
      expect(parsed, isEmpty);
    });

    test('drop map rejects malformed JSON without crashing', () {
      if (ffiLib == null || ffiLib.isEmpty) {
        markTestSkipped('SUPERCLI_FFI_LIB not set');
      }
      expect(
        SupercliNative.dropMapAccepts('not json'.codeUnits, 0, 0, 0),
        isFalse,
      );
      expect(
        SupercliNative.pathDragMapPathAt('not json'.codeUnits, 0, 0, 0),
        isNull,
      );
    });
  });
}
