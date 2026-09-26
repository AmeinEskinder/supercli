/// HTTP client for the supercli Host.
///
/// Talks to the Host's local gateway (the same API the mobile app uses;
/// see supercli-serve mobile.rs). All calls are localhost-only by
/// construction: the base URL defaults to `http://127.0.0.1:port`.
library;

import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import 'host_models.dart';
import 'models.dart';

/// Thrown when the Host is unreachable or returns an error.
final class HostException implements Exception {
  const HostException(this.message, {this.statusCode});
  final String message;
  final int? statusCode;
  @override
  String toString() => 'HostException($statusCode): $message';
}

final class HostClient {
  HostClient({required this.baseUrl, http.Client? httpClient, this.token})
      : _http = httpClient ?? http.Client();

  final Uri baseUrl;
  final http.Client _http;

  /// Paired-device `Bearer` token. The real Host requires it on every
  /// /mobile route (over TLS); without it the Host answers 401.
  final String? token;

  /// Tracks answered approval ids for client-side idempotency. The Host
  /// also enforces idempotency server-side (answer is a no-op if already
  /// answered), but we never send twice.
  final Set<String> _answered = {};

  bool get isAnswered => _answered.isNotEmpty;

  Map<String, String> get _authHeaders =>
      token == null ? const {} : {'authorization': 'Bearer $token'};

  /// GET /mobile/bootstrap — the real Host's state snapshot. Carries
  /// `pendingApprovals` (from the ApprovalHub) and session info.
  Future<Map<String, dynamic>> bootstrap() async {
    final response = await _get('/mobile/bootstrap');
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  /// GET /mobile/sessions — list session summaries.
  ///
  /// NOTE: the real Host does not expose this route; sessions come from
  /// `GET /mobile/bootstrap`. Kept for the mock-era tests; prefer
  /// [sessionsFromBootstrap].
  Future<List<SessionSummary>> listSessions() async {
    final response = await _get('/mobile/sessions');
    final body = jsonDecode(response.body) as Map<String, dynamic>;
    final sessions = (body['sessions'] as List?) ?? const [];
    return sessions
        .map((s) => SessionSummary.fromJson(s as Map<String, dynamic>))
        .toList();
  }

  /// Sessions parsed from a bootstrap body. Tolerates absence.
  static List<SessionSummary> sessionsFromBootstrap(
      Map<String, dynamic> bootstrap) {
    final sessions = (bootstrap['sessions'] as List?) ?? const [];
    return sessions
        .map((s) => SessionSummary.fromJson(s as Map<String, dynamic>))
        .toList();
  }

  /// Pending approvals parsed from a bootstrap body (real Host wire format).
  static List<PendingApproval> approvalsFromBootstrap(
      Map<String, dynamic> bootstrap) {
    final approvals = (bootstrap['pendingApprovals'] as List?) ?? const [];
    return approvals
        .map((a) => PendingApproval.fromJson(a as Map<String, dynamic>))
        .toList();
  }

  /// GET /mobile/approvals — list pending approvals.
  ///
  /// NOTE: the real Host does not expose this route; approvals come from
  /// `GET /mobile/bootstrap` as `pendingApprovals`. Kept for the mock-era
  /// tests; prefer [approvalsFromBootstrap].
  Future<List<PendingApproval>> listApprovals() async {
    final response = await _get('/mobile/approvals');
    final body = jsonDecode(response.body) as Map<String, dynamic>;
    final approvals = (body['approvals'] as List?) ?? const [];
    return approvals
        .map((a) => PendingApproval.fromJson(a as Map<String, dynamic>))
        .toList();
  }

  /// POST /mobile/approvals/answer — answer an approval.
  ///
  /// Returns false if this id was already answered (client-side idempotency:
  /// we do not send twice). Throws [HostException] on transport errors.
  Future<bool> answerApproval(ApprovalAnswer answer) async {
    if (_answered.contains(answer.id)) {
      return false;
    }
    final response = await _post(
      '/mobile/approvals/answer',
      answer.toJson(),
    );
    if (response.statusCode >= 200 && response.statusCode < 300) {
      _answered.add(answer.id);
      return true;
    }
    throw HostException(
      'answer failed: ${response.body}',
      statusCode: response.statusCode,
    );
  }

  /// Long-poll for Host events. Returns the raw event map.
  ///
  /// The caller is responsible for looping; a timeout or error throws
  /// [HostException] and the caller should back off and retry.
  Future<Map<String, dynamic>> pollEvents({Duration timeout = const Duration(seconds: 30)}) async {
    final url = baseUrl.replace(
      path: '${baseUrl.path}/mobile/events/poll',
      queryParameters: {'timeout_ms': '${timeout.inMilliseconds}'},
    );
    final response = await _http
        .get(url, headers: _authHeaders)
        .timeout(timeout + const Duration(seconds: 5));
    if (response.statusCode != 200) {
      throw HostException('poll failed', statusCode: response.statusCode);
    }
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  /// POST `/mobile/sessions/<id>/messages` — send a prompt to a session.
  Future<void> sendMessage(String sessionId, String text) async {
    await _post('/mobile/sessions/$sessionId/messages', {'text': text});
  }

  /// POST `/mobile/workspace-settings` — persist workspace settings
  /// (`settings.workspace.set`). The [settings] map uses the Host's
  /// camelCase wire format (see `AppSettings.toHostJson`). Throws
  /// [HostException] on transport or validation errors.
  Future<void> settingsSet(Map<String, Object> settings) async {
    final response = await _post('/mobile/workspace-settings', settings);
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw HostException(
        'settings set failed: ${response.body}',
  // ------------------------------------------------------------------
  // Git routes (crates/supercli-core/src/host_git.rs).
  // ------------------------------------------------------------------

  /// GET /mobile/git/status — branch, ahead/behind, changed files.
  Future<GitStatus> gitStatus(String repoPath) async {
    final url = baseUrl.replace(
      path: '${baseUrl.path}/mobile/git/status',
      queryParameters: {'path': repoPath},
    );
    final response =
        await _http.get(url, headers: _authHeaders).timeout(const Duration(seconds: 15));
    _checkOk(response, 'git status');
    return GitStatus.fromJson(jsonDecode(response.body) as Map<String, dynamic>);
  }

  /// GET /mobile/git/diff — unified diff of one file against HEAD.
  Future<String> gitDiff(String repoPath, String file) async {
    final url = baseUrl.replace(
      path: '${baseUrl.path}/mobile/git/diff',
      queryParameters: {'path': repoPath, 'file': file},
    );
    final response =
        await _http.get(url, headers: _authHeaders).timeout(const Duration(seconds: 15));
    _checkOk(response, 'git diff');
    final body = jsonDecode(response.body) as Map<String, dynamic>;
    return (body['diff'] as String?) ?? '';
  }

  /// GET /mobile/git/history — recent commits.
  Future<List<GitHistoryCommit>> gitHistory(String repoPath, {int limit = 50}) async {
    final url = baseUrl.replace(
      path: '${baseUrl.path}/mobile/git/history',
      queryParameters: {'path': repoPath, 'limit': '$limit'},
    );
    final response =
        await _http.get(url, headers: _authHeaders).timeout(const Duration(seconds: 15));
    _checkOk(response, 'git history');
    final body = jsonDecode(response.body) as Map<String, dynamic>;
    final commits = (body['commits'] as List?) ?? const [];
    return commits
        .map((c) => GitHistoryCommit.fromJson(c as Map<String, dynamic>))
        .toList();
  }

  Future<void> _gitPost(String route, String repoPath, [Map<String, Object>? extra]) async {
    final response = await _post(route, {'path': repoPath, ...?extra});
    if (response.statusCode != 200) {
      throw HostException(
        '$route failed: ${response.body}',        statusCode: response.statusCode,
      );
    }
  }

  /// GET `/mobile/workspace-settings` — read the workspace settings
  /// (`settings.workspace.get`). Returns the raw wire map in the same
  /// camelCase shape `AppSettings.fromHostJson` parses. Throws
  /// [HostException] on transport errors.
  Future<Map<String, dynamic>> settingsGet() async {
    final response = await _get('/mobile/workspace-settings');
    final body = jsonDecode(response.body);
    if (body is Map<String, dynamic>) {
      return body;
    }
    if (body is Map) {
      return Map<String, dynamic>.from(body);
    }
    throw HostException('settings get returned unexpected body');
  /// POST /mobile/git/stage — `git add` the given repo-relative paths.
  Future<void> gitStage(String repoPath, List<String> files) =>
      _gitPost('/mobile/git/stage', repoPath, {'files': files});

  /// POST /mobile/git/unstage — `git restore --staged`.
  Future<void> gitUnstage(String repoPath, List<String> files) =>
      _gitPost('/mobile/git/unstage', repoPath, {'files': files});

  /// POST /mobile/git/commit.
  Future<void> gitCommit(String repoPath, String message) =>
      _gitPost('/mobile/git/commit', repoPath, {'message': message});

  /// POST /mobile/git/fetch — `git fetch --prune`.
  Future<void> gitFetch(String repoPath) => _gitPost('/mobile/git/fetch', repoPath);

  /// POST /mobile/git/pull — `git pull --ff-only`.
  Future<void> gitPull(String repoPath) => _gitPost('/mobile/git/pull', repoPath);

  /// POST /mobile/git/push.
  Future<void> gitPush(String repoPath) => _gitPost('/mobile/git/push', repoPath);

  // ------------------------------------------------------------------
  // Files routes.
  // ------------------------------------------------------------------

  /// GET /mobile/files/list — directory listing (dirs first, then files).
  Future<List<HostFileEntry>> filesList(String path) async {
    final url = baseUrl.replace(
      path: '${baseUrl.path}/mobile/files/list',
      queryParameters: {'path': path},
    );
    final response =
        await _http.get(url, headers: _authHeaders).timeout(const Duration(seconds: 10));
    _checkOk(response, 'files list');
    final body = jsonDecode(response.body) as Map<String, dynamic>;
    final entries = (body['entries'] as List?) ?? const [];
    return entries
        .map((e) => HostFileEntry.fromJson(e as Map<String, dynamic>))
        .toList();
  }

  /// GET /mobile/files/read — read a file page (base64 body).
  Future<String> filesRead(String path, {int offset = 0, int? limit}) async {
    final params = {'path': path, 'offset': '$offset'};
    if (limit != null) params['limit'] = '$limit';
    final url = baseUrl.replace(
      path: '${baseUrl.path}/mobile/files/read',
      queryParameters: params,
    );
    final response =
        await _http.get(url, headers: _authHeaders).timeout(const Duration(seconds: 10));
    _checkOk(response, 'files read');
    final body = jsonDecode(response.body) as Map<String, dynamic>;
    final b64 = (body['dataBase64'] as String?) ?? '';
    return utf8.decode(base64Decode(b64));
  }

  /// POST /mobile/files/write — atomically write a file (base64 content).
  Future<void> filesWrite(String path, String content) async {
    final response = await _post('/mobile/files/write', {
      'path': path,
      'contentBase64': base64Encode(utf8.encode(content)),
    });
    if (response.statusCode != 200) {
      throw HostException(
        'files write failed: ${response.body}',
        statusCode: response.statusCode,
      );
    }
  }

  // ------------------------------------------------------------------
  // Usage route.
  // ------------------------------------------------------------------

  /// GET /mobile/usage/stats — Host session counts + provider transcript presence.
  Future<UsageStats> usageStats() async {
    final response = await _get('/mobile/usage/stats');
    return UsageStats.fromJson(jsonDecode(response.body) as Map<String, dynamic>);
  }

  void _checkOk(http.Response response, String what) {
    if (response.statusCode != 200) {
      throw HostException('$what failed', statusCode: response.statusCode);
    }  }

  /// POST `/mobile/browser/takeover` — browser takeover over CDP.
  ///
  /// Pass `{'list': true}` to list tabs, or `{'target_id': id, 'frames': n,
  /// 'interval_ms': ms}` to capture screenshots. Returns the Host's JSON
  /// summary (targets list, or frame count/byte size/png_magic_ok).
  Future<Map<String, dynamic>> browserTakeover(Map<String, Object> args) async {
    final response = await _post('/mobile/browser/takeover', args);
    if (response.statusCode != 200) {
      throw HostException(
        'browser takeover failed: ${response.body}',
        statusCode: response.statusCode,
      );
    }
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  Future<http.Response> _get(String path) async {
    final url = baseUrl.replace(path: '${baseUrl.path}$path');
    final response =
        await _http.get(url, headers: _authHeaders).timeout(const Duration(seconds: 10));
    if (response.statusCode != 200) {
      throw HostException('GET $path failed', statusCode: response.statusCode);
    }
    return response;
  }

  Future<http.Response> _post(String path, Map<String, Object> body) async {
    final url = baseUrl.replace(path: '${baseUrl.path}$path');
    final response = await _http
        .post(
          url,
          headers: {'content-type': 'application/json', ..._authHeaders},
          body: jsonEncode(body),
        )
        .timeout(const Duration(seconds: 10));
    return response;
  }

  void close() => _http.close();
}
