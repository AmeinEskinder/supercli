/// Workspace registry: multiple isolated app instances on one machine.
///
/// Port of the legacy Swift workspace registry module
/// (`SupercliWorkspaceRegistry` type)
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/`).
///
/// Workspaces: multiple isolated instances of the app on one Mac. A workspace is
/// a separate running instance with its own SUPERCLI_HOME (state dir, pairing
/// identity/macID, UserDefaults suite) — the same mechanism dev-blank.sh
/// uses, productized. Each workspace pairs with the phone as its own "Mac".
///
/// The released registry lives in the REAL home (~/.supercli/profiles.json), never
/// the instance's supercliDir: every instance, whatever its SUPERCLI_HOME, must
/// see one shared registry. Workspace homes remain permanently under the legacy
/// ~/.supercli/profiles/<slug> path — permanence matters, because provider hook
/// configs (~/.claude/settings.json, …) bake absolute script paths into
/// whichever home installed hooks last.
///
/// Only the portable logic is here (record codec, slugify, path
/// normalization, list-order keys). Platform I/O (FileManager, UserDefaults,
/// ProcessInfo) is injected via the [WorkspaceRegistryIo] interface.
library;

import 'dart:convert';
import 'dart:io';

/// A workspace record: one isolated app instance.
final class SupercliWorkspaceRecord {
  const SupercliWorkspaceRecord({
    required this.id,
    required this.name,
    required this.home,
    required this.createdAt,
  });

  /// Absolute path of the workspace's SUPERCLI_HOME. Minted once at create;
  /// rename never moves it (hook configs may already point into it).
  final String id;
  final String name;
  final String home;
  final int createdAt;

  factory SupercliWorkspaceRecord.fromJson(Map<String, dynamic> json) =>
      SupercliWorkspaceRecord(
        id: json['id'] as String,
        name: json['name'] as String,
        home: json['home'] as String,
        createdAt: (json['createdAt'] as num).toInt(),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'name': name,
        'home': home,
        'createdAt': createdAt,
      };

  @override
  bool operator ==(Object other) =>
      other is SupercliWorkspaceRecord &&
      id == other.id &&
      name == other.name &&
      home == other.home &&
      createdAt == other.createdAt;

  @override
  int get hashCode => Object.hash(id, name, home, createdAt);
}

/// Platform I/O for the workspace registry. Injected for testability.
abstract interface class WorkspaceRegistryIo {
  /// Read the registry file bytes, or null if missing.
  List<int>? readRegistry();

  /// Write the registry file bytes atomically.
  void writeRegistry(List<int> data);

  /// Create a directory (including parents).
  void createDirectory(String path);

  /// True if a path exists.
  bool pathExists(String path);

  /// Delete a directory tree.
  void deleteDirectory(String path);

  /// Current time in milliseconds since epoch.
  int nowMs();

  /// Generate a random UUID (lowercased).
  String newUuid();
}

/// Workspace registry: load/save/create/rename/remove + slugify.
///
/// The on-disk spelling is a released compatibility contract. The collection
/// is encoded as `profiles` even though every source and UI surface calls
/// the product concept Workspaces now.
abstract final class SupercliWorkspaceRegistry {
  const SupercliWorkspaceRegistry._();

  /// Decode the registry JSON bytes to records.
  /// Throws [FormatException] on invalid JSON.
  static List<SupercliWorkspaceRecord> decodeRegistry(List<int> data) {
    final json = jsonDecode(utf8.decode(data)) as Map<String, dynamic>;
    final profiles = json['profiles'] as List<dynamic>;
    return profiles
        .map((e) =>
            SupercliWorkspaceRecord.fromJson(e as Map<String, dynamic>))
        .toList();
  }

  /// Encode records to the registry JSON bytes.
  static List<int> encodeRegistry(
      List<SupercliWorkspaceRecord> workspaces) {
    final json = {
      'version': 1,
      'profiles': workspaces.map((w) => w.toJson()).toList(),
    };
    return utf8.encode(jsonEncode(json));
  }

  /// Load the registry, returning empty on any failure.
  static List<SupercliWorkspaceRecord> load(WorkspaceRegistryIo io) {
    try {
      final data = io.readRegistry();
      if (data == null) return const [];
      return decodeRegistry(data);
    } catch (_) {
      return const [];
    }
  }

  /// Save the registry.
  static void save(WorkspaceRegistryIo io,
      List<SupercliWorkspaceRecord> workspaces) {
    io.writeRegistry(encodeRegistry(workspaces));
  }

  /// Create a workspace record. Returns the new record.
  /// Throws [SupercliWorkspaceError] on empty name.
  static SupercliWorkspaceRecord create(
      WorkspaceRegistryIo io, String registryDir, String name) {
    final trimmed = name.trim();
    if (trimmed.isEmpty) {
      throw const SupercliWorkspaceError('Give the workspace a name.');
    }
    // Re-read right before mutating: another instance may have edited the
    // registry (atomic last-writer-wins is the concurrency model).
    final workspaces = load(io).toList();
    final slug = uniqueSlug(io, registryDir, trimmed, workspaces);
    final home = '$registryDir/profiles/$slug';
    io.createDirectory(home);
    final record = SupercliWorkspaceRecord(
      id: io.newUuid().toLowerCase(),
      name: trimmed,
      home: home,
      createdAt: io.nowMs(),
    );
    workspaces.add(record);
    save(io, workspaces);
    return record;
  }

  /// Rename a workspace. No-op on empty name or unknown id.
  static void rename(
      WorkspaceRegistryIo io, String id, String name) {
    final trimmed = name.trim();
    if (trimmed.isEmpty) return;
    final workspaces = load(io).toList();
    final index = workspaces.indexWhere((w) => w.id == id);
    if (index < 0) return;
    workspaces[index] = SupercliWorkspaceRecord(
      id: workspaces[index].id,
      name: trimmed,
      home: workspaces[index].home,
      createdAt: workspaces[index].createdAt,
    );
    save(io, workspaces);
  }

  /// Forget a workspace. [deleteData] also removes its home dir — but only
  /// if the home is under the managed root (a hand-registered entry
  /// pointing elsewhere is not ours to delete).
  static void remove(
      WorkspaceRegistryIo io, String registryDir, String id,
      {required bool deleteData}) {
    final workspaces = load(io).toList();
    final index = workspaces.indexWhere((w) => w.id == id);
    if (index < 0) return;
    final record = workspaces.removeAt(index);
    save(io, workspaces);
    if (deleteData) {
      final normalized = normalizePath(record.home);
      final root = '${normalizePath('$registryDir/profiles')}/';
      if (normalized.startsWith(root)) {
        try {
          io.deleteDirectory(record.home);
        } catch (_) {
          // Best-effort.
        }
      }
    }
  }

  /// Convert a display name to a URL-safe slug.
  static String slugify(String name) {
    final buffer = StringBuffer();
    var lastWasDash = true; // suppress leading dashes
    for (final rune in name.toLowerCase().runes) {
      final char = String.fromCharCode(rune);
      final isAlphanumeric = RegExp(r'^[a-z0-9]$').hasMatch(char);
      if (isAlphanumeric) {
        buffer.write(char);
        lastWasDash = false;
      } else if (!lastWasDash) {
        buffer.write('-');
        lastWasDash = true;
      }
    }
    var slug = buffer.toString();
    while (slug.endsWith('-')) {
      slug = slug.substring(0, slug.length - 1);
    }
    return slug.isEmpty ? 'workspace' : slug;
  }

  /// Normalize a path: expand tilde, resolve symlinks, standardize.
  static String normalizePath(String path) {
    var expanded = path;
    if (expanded.startsWith('~/')) {
      final home = Platform.environment['HOME'] ?? '';
      expanded = '$home${expanded.substring(1)}';
    }
    // Note: full symlink resolution requires platform I/O; callers that
    // need it should use the IO interface. This handles the common case.
    return File(expanded).absolute.path;
  }

  static String uniqueSlug(
    WorkspaceRegistryIo io,
    String registryDir,
    String name,
    List<SupercliWorkspaceRecord> existing,
  ) {
    final base = slugify(name);
    final taken = existing.map((w) => normalizePath(w.home)).toSet();
    var candidate = base;
    var counter = 2;
    while (taken.contains(normalizePath('$registryDir/profiles/$candidate')) ||
        io.pathExists('$registryDir/profiles/$candidate')) {
      candidate = '$base-$counter';
      counter++;
    }
    return candidate;
  }
}

/// Error for workspace operations.
final class SupercliWorkspaceError implements Exception {
  const SupercliWorkspaceError(this.message);
  final String message;

  @override
  String toString() => 'SupercliWorkspaceError: $message';
}

/// Controller-local, user-chosen display order for the unified workspace
/// list (Settings ▸ Workspaces and the sidebar picker render the SAME
/// order). Keys are kind-prefixed so local homes and remote host ids can
/// never collide: `local:<normalized home>`, `host:<hostID>`, `ssh:<id>`.
/// Unknown keys keep their natural build order after the saved ones.
abstract final class WorkspaceListOrder {
  const WorkspaceListOrder._();

  static String localKey({required String home}) =>
      'local:${SupercliWorkspaceRegistry.normalizePath(home)}';

  static String pairedKey({required String hostId}) => 'host:$hostId';

  static String sshKey({required String id}) => 'ssh:$id';

  /// Stable sort by saved position; unsaved keys follow in build order.
  static List<T> apply<T>(
    List<T> rows,
    List<String> savedOrder,
    String Function(T) key,
  ) {
    final position = <String, int>{};
    for (var i = 0; i < savedOrder.length; i++) {
      position[savedOrder[i]] = i;
    }
    final indexed = rows.asMap().entries.toList();
    indexed.sort((a, b) {
      final pa = position[key(a.value)];
      final pb = position[key(b.value)];
      if (pa != null && pb != null) return pa.compareTo(pb);
      if (pa != null) return -1;
      if (pb != null) return 1;
      return a.key.compareTo(b.key);
    });
    return indexed.map((e) => e.value).toList();
  }
}
