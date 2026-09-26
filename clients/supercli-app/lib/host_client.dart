/// HTTP client for the supercli Host.
///
/// Talks to the Host's local gateway (the same API the mobile app uses;
/// see supercli-serve mobile.rs). All calls are localhost-only by
/// construction: the base URL defaults to `http://127.0.0.1:port`.
library;

import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

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
