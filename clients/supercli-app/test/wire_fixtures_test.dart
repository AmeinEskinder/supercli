/// Wire-fixture drift guard: Dart half.
///
/// Every JSON file in `<repo>/protocol/fixtures/` (canonical examples from
/// the Swift XCTest expectations, see `protocol/fixtures/README.md`) must
/// decode through the wire DTOs in `lib/host_models.dart` without throwing.
/// Any Host-side wire change that Rust accepted but Dart chokes on breaks CI
/// here instead of the app at runtime.
///
/// NOTE: `session_summary.json` and `pending_approval.json` decode through
/// `lib/models.dart` because those names are already taken there. Both
/// `models.dart` decoders read the camelCase wire format (with backward
/// compat for the older snake_case spellings).
library;

import 'dart:convert';
import 'dart:io';

import 'package:supercli_app/host_models.dart';
import 'package:supercli_app/models.dart' as domain;
import 'package:test/test.dart';

/// Locates `<repo>/protocol/fixtures` whether the test runs from the package
/// root (`dart test`), a worktree root, or the test file's own directory.
Directory _fixturesDir() {
  Directory probe(String root) => Directory('$root/protocol/fixtures');

  var dir = Directory.current;
  for (var i = 0; i < 8; i++) {
    if (probe(dir.path).existsSync()) return probe(dir.path);
    dir = dir.parent;
  }
  var fdir = File(Platform.script.toFilePath()).parent;
  for (var i = 0; i < 8; i++) {
    if (probe(fdir.path).existsSync()) return probe(fdir.path);
    fdir = fdir.parent;
  }
  throw StateError(
    'protocol/fixtures not found (cwd=${Directory.current.path})',
  );
}

Map<String, dynamic> _load(String name) {
  final file = File('${_fixturesDir().path}/$name');
  return jsonDecode(file.readAsStringSync()) as Map<String, dynamic>;
}

void main() {
  test('fixtures directory is complete', () {
    expect(
      _fixturesDir().listSync().whereType<File>().length,
      greaterThanOrEqualTo(36),
    );
  });

  group('session and project', () {
    test(
      'session_summary decodes (via models.dart)',
      () {
        final m = _load('session_summary.json');
        final s = domain.SessionSummary.fromJson(m);
        expect(s.id, 'session-1');
        expect(s.title, 'iOS remote PRD');
        // models.dart reads the camelCase wire fields.
        expect(s.updatedAt.millisecondsSinceEpoch, 1789996860000);
        expect(s.unreadCount, 1);
        expect(m['projectID'], 'project-1');
      },
    );

    test('session_capabilities decodes', () {
      final c = SessionCapabilities.fromJson(
        _load('session_capabilities.json'),
      );
      expect(c.restart, isTrue);
      expect(c.resumeAgent, isTrue);
      expect(c.archive, isFalse);
      expect(c.notifyWhenDone, isFalse);
    });

    test('project_summary decodes', () {
      final p = ProjectSummary.fromJson(_load('project_summary.json'));
      expect(p.id, 'group-research');
      expect(p.name, 'Research');
      expect(p.path, '/dev/supercli');
      expect(p.parentProjectID, 'project-supercli');
      // The Host and Swift both name group rows `isGroup`.
      expect(p.isGroup, isTrue);
    });
  });

  group('transcript', () {
    test('transcript_block decodes', () {
      final b = TranscriptBlock.fromJson(_load('transcript_block.json'));
      expect(b.kind, 'text');
      expect(b.text, 'hi');
    });

    test('transcript_entry decodes', () {
      final e = TranscriptEntry.fromJson(_load('transcript_entry.json'));
      expect(e.id, 'entry-1');
      expect(e.role, 'user');
      expect(e.blocks, hasLength(1));
      expect(e.blocks.first.text, 'hi');
    });

    test('transcript_snapshot decodes', () {
      final s = TranscriptSnapshot.fromJson(_load('transcript_snapshot.json'));
      expect(s.entries, hasLength(2));
      expect(s.entries.first.role, 'user');
      expect(s.entries.last.role, 'agent');
      expect(s.hasMore, isFalse);
    });
  });

  group('approvals', () {
    test(
      'pending_approval decodes (via models.dart)',
      () {
        final m = _load('pending_approval.json');
        final a = domain.PendingApproval.fromJson(m);
        expect(a.id, 'a1');
        // models.dart reads the title field; the wire DTO calls it title too.
        expect(a.summary, 'Allow write?');
        expect(a.detail, 'body');
      },
    );

    test('approval_answer_request decodes', () {
      final r = ApprovalAnswerRequest.fromJson(
        _load('approval_answer_request.json'),
      );
      expect(r.id, 'a1');
      expect(r.approved, isTrue);
    });
  });

  group('presets, bootstrap, devices, workspaces', () {
    test('preset_summary decodes', () {
      final p = PresetSummary.fromJson(_load('preset_summary.json'));
      expect(p.id, 'p1');
      expect(p.label, 'Claude');
      expect(p.command, 'claude');
      expect(p.projectID, 'proj-1');
      expect(p.enabled, isTrue);
      expect(p.quickLaunch, isFalse);
      expect(p.isDefault, isFalse);
      expect(p.tintColorHex, isNull);
    });

    test('bootstrap_snapshot decodes', () {
      final b = BootstrapSnapshot.fromJson(_load('bootstrap_snapshot.json'));
      expect(b.protocolVersion, 1);
      expect(b.macID, 'mac-1');
      expect(b.macName, 'Studio Mac');
      expect(b.presets, hasLength(1));
      expect(b.sessions, hasLength(1));
      expect(b.projects, hasLength(1));
      expect(b.pendingApprovals, hasLength(1));
      expect(b.capturedAtUnixMs, 42);
    });

    test('paired_device_summary decodes', () {
      final d = PairedDeviceSummary.fromJson(
        _load('paired_device_summary.json'),
      );
      expect(d.id, 'phone-1');
      expect(d.name, 'iPhone');
      expect(d.platform, 'iOS');
      expect(d.appVersion, '1.0');
      expect(d.relayAllowed, isNull);
    });

    test('workspace_summary decodes', () {
      final w = WorkspaceSummary.fromJson(_load('workspace_summary.json'));
      expect(w.id, 'local:/Users/t/.supercli');
      expect(w.name, 'Personal');
      expect(w.isCurrent, isTrue);
      expect(w.isRunning, isTrue);
      expect(w.effectiveKind, 'local');
    });
  });

  group('artifacts and session creation', () {
    test('artifact_upload_progress decodes', () {
      final u = ArtifactUploadProgress.fromJson(
        _load('artifact_upload_progress.json'),
      );
      expect(u.uploadID, 'upload-1');
      expect(u.sessionID, 'session-1');
      expect(u.fileName, 'upload.jpg');
      expect(u.mimeType, 'image/jpeg');
      expect(u.totalBytes, 300000);
      expect(u.receivedBytes, 262144);
      expect(u.chunkSize, 16384);
      expect(u.nextOffset, 262144);
      expect(u.complete, isFalse);
    });

    test('create_session_request decodes', () {
      final r = CreateSessionRequest.fromJson(
        _load('create_session_request.json'),
      );
      expect(r.projectID, 'project-1');
      expect(r.presetID, 'claude');
      expect(r.worktreePath, '/tmp/supercli-worktree');
      expect(r.worktreeBranch, 'feature/ios-remote');
      expect(r.initialText, 'Review the iOS pairing flow.');
      expect(r.initialTextSubmitMode, 'pasteAndSubmit');
    });

    test('create_session_response decodes', () {
      final r = CreateSessionResponse.fromJson(
        _load('create_session_response.json'),
      );
      expect(r.sessionID, 'session-new');
      expect(r.session, isNotNull);
      expect(r.session!['title'], 'opencode');
    });

    test('session_text_input decodes', () {
      final i = SessionTextInput.fromJson(_load('session_text_input.json'));
      expect(i.sessionID, 'session-1');
      expect(i.text, 'hello');
      expect(i.submitMode, 'pasteAndSubmit');
    });
  });

  group('terminal and viewport', () {
    test('terminal_write_request decodes', () {
      final r = TerminalWriteRequest.fromJson(
        _load('terminal_write_request.json'),
      );
      expect(r.sessionID, 'session-1');
      // Rust canonical form keeps the raw ESC byte (serde_json escapes it as
      // \u001b); the Swift test used the base64 of the same bytes.
      expect(r.data, '\x1B[A');
      expect(r.wid, 'write-123');
    });

    test('terminal_resize_request decodes', () {
      final r = TerminalResizeRequest.fromJson(
        _load('terminal_resize_request.json'),
      );
      expect(r.sessionID, 'session-1');
      expect(r.columns, 120);
      expect(r.rows, 42);
    });

    test('terminal_color decodes', () {
      final c = TerminalColor.fromJson(_load('terminal_color.json'));
      expect(c.kind, 'ansi');
      expect(c.index, 2);
      expect(c.red, isNull);
    });

    test('terminal_style decodes', () {
      final s = TerminalStyle.fromJson(_load('terminal_style.json'));
      expect(s.bold, isTrue);
      expect(s.italic, isFalse);
      expect(s.underline, isFalse);
    });

    test('terminal_cell decodes', () {
      final c = TerminalCell.fromJson(_load('terminal_cell.json'));
      expect(c.text, 'A');
      expect(c.foreground, isNotNull);
      expect(c.foreground!.kind, 'rgb');
      expect(c.background, isNotNull);
      expect(c.background!.kind, 'ansi');
      expect(c.background!.index, 4);
      expect(c.style.bold, isTrue);
    });

    test('terminal_cursor decodes', () {
      final c = TerminalCursor.fromJson(_load('terminal_cursor.json'));
      expect(c.row, 0);
      expect(c.column, 1);
      expect(c.shape, 'beam');
      expect(c.visible, isTrue);
    });

    test('viewport_frame decodes', () {
      final f = ViewportFrame.fromJson(_load('viewport_frame.json'));
      expect(f.sessionID, 'session-1');
      expect(f.sequence, 42);
      expect(f.rows, 1);
      expect(f.columns, 2);
      expect(f.cells, hasLength(2));
      expect(f.cells[0].text, 'A');
      expect(f.cells[1].text, '界');
      expect(f.cursor, isNotNull);
      expect(f.cursor!.shape, 'beam');
      expect(f.alternateScreen, isTrue);
      expect(f.capturedAtUnixMs, 1789996800000);
    });

    test('viewport_subscription decodes', () {
      final s = ViewportSubscription.fromJson(
        _load('viewport_subscription.json'),
      );
      expect(s.sessionID, 'session-1');
      expect(s.rows, 24);
      expect(s.columns, 80);
    });

    test('terminal_cell_run decodes', () {
      final r = TerminalCellRun.fromJson(_load('terminal_cell_run.json'));
      expect(r.row, 12);
      expect(r.column, 4);
      expect(r.cells, hasLength(2));
      expect(r.cells.map((c) => c.text).join(), 'OK');
    });

    test('viewport_patch decodes', () {
      final p = ViewportPatch.fromJson(_load('viewport_patch.json'));
      expect(p.sessionID, 'session-1');
      expect(p.sequence, 43);
      expect(p.baseSequence, 42);
      expect(p.changedRuns, hasLength(1));
      final run = p.changedRuns.first;
      expect(run.row, 12);
      expect(run.column, 4);
      expect(run.cells.map((c) => c.text).join(), 'OK');
      expect(run.cells.first.foreground!.index, 2);
      expect(p.cursor, isNotNull);
      expect(p.cursor!.shape, 'block');
      expect(p.capturedAtUnixMs, 1789996900000);
    });

    test('stream_event decodes', () {
      final e = StreamEvent.fromJson(_load('stream_event.json'));
      expect(e.protocolVersion, 1);
      expect(e.id, 'event-1');
      expect(e.kind, 'viewportFrame');
      expect(e.sessionID, 'session-1');
      expect(e.payload, isNotNull);
      expect(e.createdAtUnixMs, 1789996800000);
    });
  });

  group('session management', () {
    test('mark_read_request decodes', () {
      final r = MarkReadRequest.fromJson(_load('mark_read_request.json'));
      expect(r.sessionID, 'session-1');
    });

    test('session_action_request decodes', () {
      final r = SessionActionRequest.fromJson(
        _load('session_action_request.json'),
      );
      expect(r.sessionID, 'session-1');
      expect(r.action, 'remove');
    });

    test('session_organization_patch decodes', () {
      final p = SessionOrganizationPatch.fromJson(
        _load('session_organization_patch.json'),
      );
      expect(p.sessionID, 'session-1');
      expect(p.title, 'Renamed from phone');
      expect(p.pinned, isTrue);
      expect(p.projectID, isNull);
    });

    test('screenshot_request decodes', () {
      final r = ScreenshotRequest.fromJson(_load('screenshot_request.json'));
      expect(r.sessionID, 'session-1');
    });

    test('screenshot_request_response decodes', () {
      final r = ScreenshotRequestResponse.fromJson(
        _load('screenshot_request_response.json'),
      );
      expect(r.accepted, isTrue);
      expect(r.requestedAtUnixMs, 1789996800000);
    });
  });

  group('plugins, restart, push', () {
    test('plugin_update decodes', () {
      final u = PluginUpdate.fromJson(_load('plugin_update.json'));
      expect(u.id, 'plugin-1');
      expect(u.state, 'active');
      expect(u.installedVersion, '1.0.0');
      expect(u.latestVersion, '1.1.0');
      expect(u.updateAvailable, isTrue);
    });

    test('plugin_updates decodes', () {
      final u = PluginUpdates.fromJson(_load('plugin_updates.json'));
      expect(u.checking, isFalse);
      expect(u.items, hasLength(1));
      expect(u.items.first.id, 'plugin-1');
    });

    test('restart_session_request decodes', () {
      final r = RestartSessionRequest.fromJson(
        _load('restart_session_request.json'),
      );
      expect(r.sessionID, 'session-1');
    });

    test('push_token_registration decodes', () {
      final r = PushTokenRegistration.fromJson(
        _load('push_token_registration.json'),
      );
      expect(r.token, 'push-token');
      // The Swift test uses the lowercase platform id "ios".
      expect(r.platform, 'ios');
      expect(r.appVersion, '1.0');
    });
  });
}
