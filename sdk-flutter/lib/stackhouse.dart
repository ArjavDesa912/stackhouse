library stackhouse_flutter;

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:http/http.dart' as http;

// ============================================================================
// Error Types
// ============================================================================

class StackhouseError implements Exception {
  final String message;
  final int? statusCode;
  final dynamic details;

  StackhouseError(this.message, {this.statusCode, this.details});

  @override
  String toString() => 'StackhouseError${statusCode != null ? "($statusCode)" : ""}: $message';

  factory StackhouseError.fromResponse(http.Response response) {
    int statusCode = response.statusCode;
    String message = 'Request failed';
    dynamic details;

    try {
      final data = jsonDecode(response.body);
      message = data['message'] ?? data['error'] ?? message;
      details = data;
    } catch (_) {
      message = response.reasonPhrase ?? message;
    }

    return StackhouseError(message, statusCode: statusCode, details: details);
  }
}

// ============================================================================
// Data Models
// ============================================================================

class User {
  final int id;
  final String email;
  final String createdAt;
  final String updatedAt;
  final Map<String, dynamic>? metadata;

  User({
    required this.id,
    required this.email,
    required this.createdAt,
    required this.updatedAt,
    this.metadata,
  });

  factory User.fromJson(Map<String, dynamic> json) {
    return User(
      id: json['id'] as int,
      email: json['email'] as String,
      createdAt: json['created_at'] as String,
      updatedAt: json['updated_at'] as String,
      metadata: json['metadata'] as Map<String, dynamic>?,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'email': email,
      'created_at': createdAt,
      'updated_at': updatedAt,
      if (metadata != null) 'metadata': metadata,
    };
  }
}

class AuthSession {
  final User user;
  final String accessToken;
  final String refreshToken;
  final int expiresAt;

  AuthSession({
    required this.user,
    required this.accessToken,
    required this.refreshToken,
    required this.expiresAt,
  });

  bool get isExpired => DateTime.now().millisecondsSinceEpoch >= expiresAt;

  factory AuthSession.fromJson(Map<String, dynamic> json) {
    return AuthSession(
      user: User.fromJson(json['user']),
      accessToken: json['access_token'] as String,
      refreshToken: json['refresh_token'] as String,
      expiresAt: json['expires_at'] as int,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'user': user.toJson(),
      'access_token': accessToken,
      'refresh_token': refreshToken,
      'expires_at': expiresAt,
    };
  }
}

class QueryResult {
  final bool success;
  final List<Map<String, dynamic>> data;
  final int count;
  final int total;
  final String collection;

  QueryResult({
    required this.success,
    required this.data,
    required this.count,
    required this.total,
    required this.collection,
  });

  factory QueryResult.fromJson(Map<String, dynamic> json) {
    return QueryResult(
      success: json['success'] as bool,
      data: (json['data'] as List).cast<Map<String, dynamic>>(),
      count: json['count'] as int,
      total: json['total'] as int,
      collection: json['collection'] as String,
    );
  }
}

class QueryOptions {
  final Map<String, String>? filters;
  final String? orderBy;
  final String orderDir;
  final int? limit;
  final int? offset;

  QueryOptions({
    this.filters,
    this.orderBy,
    this.orderDir = 'ASC',
    this.limit,
    this.offset,
  });
}

class PushData {
  final int id;
  final String collection;
  final List<String> columnsAdded;

  PushData({
    required this.id,
    required this.collection,
    required this.columnsAdded,
  });

  factory PushData.fromJson(Map<String, dynamic> json) {
    return PushData(
      id: json['id'] as int,
      collection: json['collection'] as String,
      columnsAdded: (json['columns_added'] as List).cast<String>(),
    );
  }
}

// ============================================================================
// Storage Models
// ============================================================================

class Bucket {
  final int id;
  final String name;
  final bool isPublic;
  final int? fileSizeLimit;
  final String? allowedMimeTypes;
  final String createdAt;

  Bucket({
    required this.id,
    required this.name,
    required this.isPublic,
    this.fileSizeLimit,
    this.allowedMimeTypes,
    required this.createdAt,
  });

  factory Bucket.fromJson(Map<String, dynamic> json) {
    return Bucket(
      id: json['id'] as int,
      name: json['name'] as String,
      isPublic: json['is_public'] as bool? ?? false,
      fileSizeLimit: json['file_size_limit'] as int?,
      allowedMimeTypes: json['allowed_mime_types'] as String?,
      createdAt: json['created_at'] as String,
    );
  }
}

class StorageObject {
  final int id;
  final String bucket;
  final String path;
  final int size;
  final String mimeType;
  final String createdAt;

  StorageObject({
    required this.id,
    required this.bucket,
    required this.path,
    required this.size,
    required this.mimeType,
    required this.createdAt,
  });

  factory StorageObject.fromJson(Map<String, dynamic> json) {
    return StorageObject(
      id: json['id'] as int,
      bucket: json['bucket'] as String,
      path: json['path'] as String,
      size: json['size'] as int,
      mimeType: json['mime_type'] as String,
      createdAt: json['created_at'] as String,
    );
  }
}

// ============================================================================
// RLS Models
// ============================================================================

class RlsPolicy {
  final String name;
  final String table;
  final String operation;
  final bool permissive;
  final String? usingExpression;
  final String? checkExpression;

  RlsPolicy({
    required this.name,
    required this.table,
    required this.operation,
    required this.permissive,
    this.usingExpression,
    this.checkExpression,
  });

  factory RlsPolicy.fromJson(Map<String, dynamic> json) {
    return RlsPolicy(
      name: json['name'] as String,
      table: json['table'] as String,
      operation: json['operation'] as String,
      permissive: json['permissive'] as bool? ?? true,
      usingExpression: json['using_expression'] as String?,
      checkExpression: json['check_expression'] as String?,
    );
  }
}

class RlsStatus {
  final String table;
  final bool enabled;
  final List<RlsPolicy> policies;

  RlsStatus({
    required this.table,
    required this.enabled,
    required this.policies,
  });

  factory RlsStatus.fromJson(Map<String, dynamic> json) {
    return RlsStatus(
      table: json['table'] as String,
      enabled: json['enabled'] as bool,
      policies: (json['policies'] as List?)
              ?.map((p) => RlsPolicy.fromJson(p as Map<String, dynamic>))
              .toList() ??
          [],
    );
  }
}

// ============================================================================
// Realtime Models
// ============================================================================

class RealtimeEvent {
  final String type;
  final String table;
  final Map<String, dynamic>? record;
  final Map<String, dynamic>? oldRecord;
  final String timestamp;

  RealtimeEvent({
    required this.type,
    required this.table,
    this.record,
    this.oldRecord,
    required this.timestamp,
  });

  factory RealtimeEvent.fromJson(Map<String, dynamic> json) {
    return RealtimeEvent(
      type: json['type'] as String,
      table: json['table'] as String,
      record: json['record'] as Map<String, dynamic>?,
      oldRecord: json['old_record'] as Map<String, dynamic>?,
      timestamp: json['timestamp'] as String? ?? '',
    );
  }
}

typedef RealtimeCallback = void Function(RealtimeEvent event);

// ============================================================================
// Vector Search Models
// ============================================================================

class VectorSearchResult {
  final int id;
  final double similarity;
  final Map<String, dynamic> data;

  VectorSearchResult({
    required this.id,
    required this.similarity,
    required this.data,
  });

  factory VectorSearchResult.fromJson(Map<String, dynamic> json) {
    return VectorSearchResult(
      id: json['id'] as int,
      similarity: (json['similarity'] as num).toDouble(),
      data: json['data'] as Map<String, dynamic>,
    );
  }
}

class VectorColumnInfo {
  final String table;
  final String column;
  final int dimensions;
  final String indexType;
  final int rowCount;

  VectorColumnInfo({
    required this.table,
    required this.column,
    required this.dimensions,
    required this.indexType,
    required this.rowCount,
  });

  factory VectorColumnInfo.fromJson(Map<String, dynamic> json) {
    return VectorColumnInfo(
      table: json['table'] as String,
      column: json['column'] as String,
      dimensions: json['dimensions'] as int,
      indexType: json['index_type'] as String,
      rowCount: json['row_count'] as int,
    );
  }
}

// ============================================================================
// Main Client
// ============================================================================

class StackhouseClient {
  final String baseUrl;
  final String? apiKey;
  Map<String, String> _headers;
  AuthSession? _session;

  /// Storage client for file operations
  late final StorageClient storage;

  /// Realtime client for WebSocket subscriptions
  late final RealtimeClient realtime;

  StackhouseClient(this.baseUrl, {this.apiKey})
      : _headers = {
          'Content-Type': 'application/json',
          if (apiKey != null) 'X-API-Key': apiKey!,
        } {
    storage = StorageClient(baseUrl: baseUrl, headersGetter: () => _authHeaders);
    realtime = RealtimeClient(baseUrl: baseUrl, headersGetter: () => _authHeaders);
  }

  AuthSession? get session => _session;
  Map<String, String> get _authHeaders => {
    ..._headers,
    if (_session != null) 'Authorization': 'Bearer ${_session!.accessToken}',
  };

  // ============================================================================
  // Auth Methods
  // ============================================================================

  AuthSession? get currentSession => _session;
  User? get currentUser => _session?.user;
  bool get isAuthenticated => _session != null && !_session!.isExpired;

  Future<AuthSession> signup(
    String email,
    String password, {
    Map<String, dynamic>? metadata,
  }) async {
    final body = {
      'email': email,
      'password': password,
      if (metadata != null) 'metadata': metadata,
    };

    final response = await http.post(
      Uri.parse('$baseUrl/v1/auth/signup'),
      headers: _authHeaders,
      body: jsonEncode(body),
    );

    if (response.statusCode >= 400) {
      throw StackhouseError.fromResponse(response);
    }

    final result = jsonDecode(response.body);
    final tokens = result['data'];

    return _updateSession(tokens);
  }

  Future<AuthSession> login(String email, String password) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/auth/login'),
      headers: _authHeaders,
      body: jsonEncode({'email': email, 'password': password}),
    );

    if (response.statusCode >= 400) {
      throw StackhouseError.fromResponse(response);
    }

    final result = jsonDecode(response.body);
    final tokens = result['data'];

    return _updateSession(tokens);
  }

  Future<void> logout() async {
    final currentRefreshToken = _session?.refreshToken;

    if (currentRefreshToken != null) {
      try {
        await http.post(
          Uri.parse('$baseUrl/v1/auth/logout'),
          headers: _authHeaders,
          body: jsonEncode({'refresh_token': currentRefreshToken}),
        );
      } catch (e) {
        print('[Stackhouse] Logout request failed: $e');
      }
    }

    _session = null;
  }

  Future<AuthSession> refreshAccessToken() async {
    final currentRefreshToken = _session?.refreshToken;

    if (currentRefreshToken == null) {
      throw StackhouseError('No session to refresh', statusCode: 401);
    }

    final response = await http.post(
      Uri.parse('$baseUrl/v1/auth/refresh'),
      headers: _authHeaders,
      body: jsonEncode({'refresh_token': currentRefreshToken}),
    );

    if (response.statusCode >= 400) {
      _session = null;
      throw StackhouseError.fromResponse(response);
    }

    final result = jsonDecode(response.body);
    final tokens = result['data'];

    return _updateSession(tokens);
  }

  /// Get the current authenticated user from server
  Future<User> getUser() async {
    final response = await http.get(
      Uri.parse('$baseUrl/v1/auth/me'),
      headers: _authHeaders,
    );

    if (response.statusCode >= 400) {
      throw StackhouseError.fromResponse(response);
    }

    final result = jsonDecode(response.body);
    return User.fromJson(result['data'] as Map<String, dynamic>);
  }

  /// Update user metadata
  Future<User> updateUser({Map<String, dynamic>? metadata}) async {
    final response = await http.put(
      Uri.parse('$baseUrl/v1/auth/user'),
      headers: _authHeaders,
      body: jsonEncode({if (metadata != null) 'metadata': metadata}),
    );

    if (response.statusCode >= 400) {
      throw StackhouseError.fromResponse(response);
    }

    final result = jsonDecode(response.body);
    return User.fromJson(result['data'] as Map<String, dynamic>);
  }

  /// Change the current user's password
  Future<void> changePassword(String currentPassword, String newPassword) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/auth/change-password'),
      headers: _authHeaders,
      body: jsonEncode({
        'current_password': currentPassword,
        'new_password': newPassword,
      }),
    );

    if (response.statusCode >= 400) {
      throw StackhouseError.fromResponse(response);
    }
  }

  void setSession(AuthSession authSession) {
    if (authSession.isExpired) {
      throw StackhouseError('Session has expired', statusCode: 400);
    }
    _session = authSession;
  }

  AuthSession _updateSession(Map<String, dynamic> tokens) {
    final expiresIn = tokens['expires_in'] as int;
    final expiresAt = DateTime.now().millisecondsSinceEpoch + (expiresIn * 1000);

    final newSession = AuthSession(
      user: User.fromJson(tokens['user']),
      accessToken: tokens['access_token'] as String,
      refreshToken: tokens['refresh_token'] as String,
      expiresAt: expiresAt,
    );

    _session = newSession;
    return newSession;
  }

  // ============================================================================
  // Query Methods
  // ============================================================================

  QueryBuilder from(String collection) {
    return QueryBuilder(
      baseUrl: baseUrl,
      collection: collection,
      headers: _authHeaders,
    );
  }

  // ============================================================================
  // Vector Search Methods
  // ============================================================================

  /// Get a vector search builder for a collection
  VectorBuilder vectors(String collection) {
    return VectorBuilder(
      baseUrl: baseUrl,
      collection: collection,
      headers: _authHeaders,
    );
  }

  // ============================================================================
  // RLS Methods
  // ============================================================================

  /// Get an RLS client for a table
  RlsClient rls(String table) {
    return RlsClient(
      baseUrl: baseUrl,
      table: table,
      headers: _authHeaders,
    );
  }
}

// ============================================================================
// Storage Client
// ============================================================================

class StorageClient {
  final String baseUrl;
  final Map<String, String> Function() headersGetter;

  StorageClient({required this.baseUrl, required this.headersGetter});

  Map<String, String> get _headers => headersGetter();

  /// Create a new bucket
  Future<Bucket> createBucket(String name, {bool isPublic = false}) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/storage/buckets'),
      headers: _headers,
      body: jsonEncode({'name': name, 'public': isPublic}),
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return Bucket.fromJson(result['data'] as Map<String, dynamic>);
  }

  /// List all buckets
  Future<List<Bucket>> listBuckets() async {
    final response = await http.get(
      Uri.parse('$baseUrl/v1/storage/buckets'),
      headers: _headers,
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return (result['data'] as List)
        .map((b) => Bucket.fromJson(b as Map<String, dynamic>))
        .toList();
  }

  /// Get bucket info
  Future<Bucket> getBucket(String name) async {
    final response = await http.get(
      Uri.parse('$baseUrl/v1/storage/buckets/$name'),
      headers: _headers,
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return Bucket.fromJson(result['data'] as Map<String, dynamic>);
  }

  /// Delete a bucket
  Future<void> deleteBucket(String name) async {
    final response = await http.delete(
      Uri.parse('$baseUrl/v1/storage/buckets/$name'),
      headers: _headers,
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);
  }

  /// Upload a file to a bucket
  Future<StorageObject> uploadObject(
    String bucket,
    String path,
    List<int> data,
    String mimeType,
  ) async {
    final request = http.MultipartRequest(
      'POST',
      Uri.parse('$baseUrl/v1/storage/object/$bucket/$path'),
    );
    request.headers.addAll(_headers);
    request.headers.remove('Content-Type'); // Let multipart set it
    request.files.add(http.MultipartFile.fromBytes('file', data,
        filename: path.split('/').last,
        contentType: _parseMimeType(mimeType)));

    final streamedResponse = await request.send();
    final response = await http.Response.fromStream(streamedResponse);

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return StorageObject.fromJson(result['data'] as Map<String, dynamic>);
  }

  /// Download a file from a bucket
  Future<List<int>> downloadObject(String bucket, String path) async {
    final response = await http.get(
      Uri.parse('$baseUrl/v1/storage/object/$bucket/$path'),
      headers: _headers,
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    return response.bodyBytes;
  }

  /// Delete a file from a bucket
  Future<void> deleteObject(String bucket, String path) async {
    final response = await http.delete(
      Uri.parse('$baseUrl/v1/storage/object/$bucket/$path'),
      headers: _headers,
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);
  }

  /// List objects in a bucket
  Future<List<StorageObject>> listObjects(String bucket, {String? prefix, int limit = 100, int offset = 0}) async {
    var uri = Uri.parse('$baseUrl/v1/storage/list/$bucket');
    final params = <String, String>{
      'limit': limit.toString(),
      'offset': offset.toString(),
    };
    if (prefix != null) params['prefix'] = prefix;
    uri = uri.replace(queryParameters: params);

    final response = await http.get(uri, headers: _headers);

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return (result['data'] as List)
        .map((o) => StorageObject.fromJson(o as Map<String, dynamic>))
        .toList();
  }

  // Helper to parse mime type for multipart
  dynamic _parseMimeType(String mimeType) {
    // http package handles this via MediaType, but we don't want to import that
    return null; // Let the http library auto-detect
  }
}

// ============================================================================
// Realtime Client
// ============================================================================

class RealtimeClient {
  final String baseUrl;
  final Map<String, String> Function() headersGetter;

  WebSocket? _ws;
  bool _connected = false;
  int _reconnectAttempts = 0;
  final int maxReconnectAttempts = 10;
  final List<_Subscription> _subscriptions = [];

  RealtimeClient({required this.baseUrl, required this.headersGetter});

  bool get isConnected => _connected;

  /// Connect to the Stackhouse realtime WebSocket server
  Future<void> connect() async {
    final wsUrl = baseUrl
        .replaceFirst('http:', 'ws:')
        .replaceFirst('https:', 'wss:');

    _ws = await WebSocket.connect('$wsUrl/v1/realtime');
    _connected = true;
    _reconnectAttempts = 0;

    // Re-subscribe to all existing subscriptions
    for (final sub in _subscriptions) {
      _sendSubscribe(sub.table, sub.event);
    }

    _ws!.listen(
      (data) {
        try {
          final json = jsonDecode(data as String) as Map<String, dynamic>;
          final type = json['type'] as String?;
          if (type == 'INSERT' || type == 'UPDATE' || type == 'DELETE') {
            final event = RealtimeEvent.fromJson(json);
            for (final sub in _subscriptions) {
              if (sub.table == event.table &&
                  (sub.event == '*' || sub.event == event.type)) {
                sub.callback(event);
              }
            }
          }
        } catch (_) {}
      },
      onDone: () {
        _connected = false;
        _attemptReconnect();
      },
      onError: (_) {
        _connected = false;
      },
    );
  }

  /// Subscribe to changes on a table
  /// Returns an unsubscribe function.
  void Function() on(String table, String event, RealtimeCallback callback) {
    final sub = _Subscription(table: table, event: event, callback: callback);
    _subscriptions.add(sub);

    if (_connected && _ws != null) {
      _sendSubscribe(table, event);
    }

    return () {
      _subscriptions.remove(sub);
      if (_connected && _ws != null) {
        final hasOtherSubs = _subscriptions.any((s) => s.table == table);
        if (!hasOtherSubs) {
          _sendUnsubscribe(table);
        }
      }
    };
  }

  /// Disconnect from the realtime server
  void disconnect() {
    _subscriptions.clear();
    _ws?.close();
    _ws = null;
    _connected = false;
  }

  void _sendSubscribe(String table, String event) {
    _ws?.add(jsonEncode({'type': 'subscribe', 'table': table, 'event': event}));
  }

  void _sendUnsubscribe(String table) {
    _ws?.add(jsonEncode({'type': 'unsubscribe', 'table': table}));
  }

  void _attemptReconnect() {
    if (_reconnectAttempts >= maxReconnectAttempts) return;
    _reconnectAttempts++;

    final delay = Duration(milliseconds: 1000 * (1 << (_reconnectAttempts - 1)));
    Future.delayed(delay < const Duration(seconds: 30) ? delay : const Duration(seconds: 30), () {
      connect().catchError((_) {});
    });
  }
}

class _Subscription {
  final String table;
  final String event;
  final RealtimeCallback callback;
  _Subscription({required this.table, required this.event, required this.callback});
}

// ============================================================================
// RLS Client
// ============================================================================

class RlsClient {
  final String baseUrl;
  final String table;
  final Map<String, String> headers;

  RlsClient({required this.baseUrl, required this.table, required this.headers});

  /// Enable Row Level Security on the table
  Future<void> enable() async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/rls/$table/enable'),
      headers: headers,
    );
    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);
  }

  /// Disable Row Level Security on the table
  Future<void> disable() async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/rls/$table/disable'),
      headers: headers,
    );
    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);
  }

  /// Create an RLS policy
  Future<void> createPolicy({
    required String name,
    String operation = 'ALL',
    bool permissive = true,
    String? usingExpression,
    String? checkExpression,
  }) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/rls/$table/policies'),
      headers: headers,
      body: jsonEncode({
        'name': name,
        'operation': operation,
        'permissive': permissive,
        if (usingExpression != null) 'using_expression': usingExpression,
        if (checkExpression != null) 'check_expression': checkExpression,
      }),
    );
    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);
  }

  /// List all RLS policies on the table
  Future<List<RlsPolicy>> listPolicies() async {
    final response = await http.get(
      Uri.parse('$baseUrl/v1/rls/$table/policies'),
      headers: headers,
    );
    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return (result['data'] as List)
        .map((p) => RlsPolicy.fromJson(p as Map<String, dynamic>))
        .toList();
  }

  /// Drop an RLS policy by name
  Future<void> dropPolicy(String policyName) async {
    final response = await http.delete(
      Uri.parse('$baseUrl/v1/rls/$table/policies/$policyName'),
      headers: headers,
    );
    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);
  }

  /// Get RLS status for the table
  Future<RlsStatus> getStatus() async {
    final response = await http.get(
      Uri.parse('$baseUrl/v1/rls/$table/status'),
      headers: headers,
    );
    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return RlsStatus.fromJson(result['data'] as Map<String, dynamic>);
  }
}

// ============================================================================
// Vector Builder
// ============================================================================

class VectorBuilder {
  final String baseUrl;
  final String collection;
  final Map<String, String> headers;

  VectorBuilder({
    required this.baseUrl,
    required this.collection,
    required this.headers,
  });

  /// Perform a similarity search
  Future<List<VectorSearchResult>> search(
    List<double> queryVector, {
    int topK = 10,
    String metric = 'cosine',
    Map<String, dynamic>? filters,
    String column = 'embedding',
  }) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/vectors/$collection/search'),
      headers: headers,
      body: jsonEncode({
        'vector': queryVector,
        'top_k': topK,
        'metric': metric,
        if (filters != null) 'filters': filters,
        'column': column,
      }),
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return (result['data'] as List)
        .map((item) => VectorSearchResult.fromJson(item as Map<String, dynamic>))
        .toList();
  }

  /// Upsert a record with vector embedding
  Future<Map<String, dynamic>> upsert(
    List<double> embedding, {
    int? id,
    Map<String, dynamic>? data,
    String column = 'embedding',
  }) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/vectors/$collection/upsert'),
      headers: headers,
      body: jsonEncode({
        'embedding': embedding,
        if (id != null) 'id': id,
        if (data != null) 'data': data,
        'column': column,
      }),
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return result['data'] as Map<String, dynamic>;
  }

  /// Batch upsert records with vector embeddings
  Future<Map<String, dynamic>> batchUpsert(
    List<Map<String, dynamic>> records,
  ) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/vectors/$collection/batch'),
      headers: headers,
      body: jsonEncode({
        'records': records.map((r) {
          return {
            'embedding': r['embedding'],
            if (r['id'] != null) 'id': r['id'],
            if (r['data'] != null) 'data': r['data'],
            'column': r['column'] ?? 'embedding',
          };
        }).toList(),
      }),
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return result['data'] as Map<String, dynamic>;
  }

  /// Get vector column information
  Future<List<VectorColumnInfo>> info() async {
    final response = await http.get(
      Uri.parse('$baseUrl/v1/vectors/$collection/info'),
      headers: headers,
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return (result['data'] as List)
        .map((item) => VectorColumnInfo.fromJson(item as Map<String, dynamic>))
        .toList();
  }
}

// ============================================================================
// Query Builder
// ============================================================================

class QueryBuilder {
  final String baseUrl;
  final String collection;
  final Map<String, String> headers;

  QueryBuilder({
    required this.baseUrl,
    required this.collection,
    required this.headers,
  });

  Future<QueryResult> select([QueryOptions? options]) async {
    var uri = Uri.parse('$baseUrl/v1/query/$collection');

    if (options != null) {
      final queryParams = <String, String>{};

      options.filters?.forEach((key, value) {
        queryParams[key] = value;
      });

      if (options.orderBy != null) {
        queryParams['order_by'] = options.orderBy!;
        queryParams['order_dir'] = options.orderDir;
      }

      if (options.limit != null) {
        queryParams['limit'] = options.limit.toString();
      }

      if (options.offset != null) {
        queryParams['offset'] = options.offset.toString();
      }

      if (queryParams.isNotEmpty) {
        uri = uri.replace(queryParameters: queryParams);
      }
    }

    final response = await http.get(uri, headers: headers);

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return QueryResult.fromJson(result);
  }

  Future<Map<String, dynamic>> getById(String id) async {
    final response = await http.get(
      Uri.parse('$baseUrl/v1/query/$collection/$id'),
      headers: headers,
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return result['data'] as Map<String, dynamic>;
  }

  Future<PushData> insert(Map<String, dynamic> data) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/push/$collection'),
      headers: headers,
      body: jsonEncode(data),
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return PushData.fromJson(result['data']);
  }

  Future<Map<String, dynamic>> insertBatch(List<Map<String, dynamic>> data) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/push/$collection/batch'),
      headers: headers,
      body: jsonEncode(data),
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return result['data'] as Map<String, dynamic>;
  }

  Future<int> update(String id, Map<String, dynamic> data) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/update/$collection/$id'),
      headers: headers,
      body: jsonEncode(data),
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return result['affected'] as int;
  }

  Future<int> delete(String id) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/delete/$collection/$id'),
      headers: headers,
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return result['affected'] as int;
  }

  /// Bulk delete with filters
  Future<int> bulkDelete([Map<String, dynamic>? filters]) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/delete/$collection'),
      headers: headers,
      body: jsonEncode({'filters': filters ?? {}}),
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return result['affected'] as int;
  }

  /// Bulk update with filters
  Future<int> bulkUpdate(Map<String, dynamic> data, {Map<String, dynamic>? filters}) async {
    final response = await http.post(
      Uri.parse('$baseUrl/v1/update/$collection'),
      headers: headers,
      body: jsonEncode({
        'data': data,
        'filters': filters ?? {},
      }),
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);

    final result = jsonDecode(response.body);
    return result['affected'] as int;
  }

  /// Drop the entire table
  Future<void> dropTable() async {
    final response = await http.delete(
      Uri.parse('$baseUrl/v1/tables/$collection'),
      headers: headers,
    );

    if (response.statusCode >= 400) throw StackhouseError.fromResponse(response);
  }
}
