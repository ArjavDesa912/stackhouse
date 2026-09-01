import Foundation

// ============================================================================
// Error Types
// ============================================================================

public enum StackhouseError: Error, LocalizedError, Sendable {
    case networkError(String)
    case decodingError(String)
    case unauthorized(String)
    case notFound(String)
    case validationError(String)
    case serverError(Int, String)
    case unknown(String)

    public var errorDescription: String? {
        switch self {
        case .networkError(let msg): return "Network error: \(msg)"
        case .decodingError(let msg): return "Decoding error: \(msg)"
        case .unauthorized(let msg): return "Unauthorized: \(msg)"
        case .notFound(let msg): return "Not found: \(msg)"
        case .validationError(let msg): return "Validation error: \(msg)"
        case .serverError(let code, let msg): return "Server error \(code): \(msg)"
        case .unknown(let msg): return "Unknown error: \(msg)"
        }
    }

    public var statusCode: Int? {
        switch self {
        case .unauthorized: return 401
        case .notFound: return 404
        case .validationError: return 400
        case .serverError(let code, _): return code
        default: return nil
        }
    }
}

// ============================================================================
// Data Models
// ============================================================================

public struct User: Codable, Sendable {
    public let id: Int64
    public let email: String
    public let createdAt: String
    public let updatedAt: String
    public let metadata: [String: Any]?

    private enum CodingKeys: String, CodingKey {
        case id, email
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case metadata
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(Int64.self, forKey: .id)
        email = try container.decode(String.self, forKey: .email)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)
        if container.contains(.metadata) {
            let metadataContainer = try container.nestedContainer(keyedBy: GenericCodingKeys.self, forKey: .metadata)
            metadata = try? decodeAny(from: metadataContainer)
        } else {
            metadata = nil
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(email, forKey: .email)
        try container.encode(createdAt, forKey: .createdAt)
        try container.encode(updatedAt, forKey: .updatedAt)
    }
}

private struct GenericCodingKeys: CodingKey {
    var stringValue: String
    init?(stringValue: String) { self.stringValue = stringValue }
    var intValue: Int? { return nil }
    init?(intValue: Int) { return nil }
}

private func decodeAny(from container: KeyedDecodingContainer<GenericCodingKeys>) -> [String: Any] {
    var result: [String: Any] = [:]
    for key in container.allKeys {
        if let stringValue = try? container.decode(String.self, forKey: key) {
            result[key.stringValue] = stringValue
        } else if let intValue = try? container.decode(Int.self, forKey: key) {
            result[key.stringValue] = intValue
        } else if let doubleValue = try? container.decode(Double.self, forKey: key) {
            result[key.stringValue] = doubleValue
        } else if let boolValue = try? container.decode(Bool.self, forKey: key) {
            result[key.stringValue] = boolValue
        }
    }
    return result
}

public struct AuthSession: Codable, Sendable {
    public let user: User
    public let accessToken: String
    public let refreshToken: String
    public let expiresAt: TimeInterval

    private enum CodingKeys: String, CodingKey {
        case user
        case accessToken = "access_token"
        case refreshToken = "refresh_token"
        case expiresAt = "expires_at"
    }

    public var isExpired: Bool {
        return expiresAt <= Date().timeIntervalSince1970
    }
}

public struct AuthTokens: Codable, Sendable {
    public let accessToken: String
    public let refreshToken: String
    public let expiresIn: TimeInterval
    public let tokenType: String
    public let user: User

    private enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
        case refreshToken = "refresh_token"
        case expiresIn = "expires_in"
        case tokenType = "token_type"
        case user
    }
}

public struct QueryResult: Codable, Sendable {
    public let success: Bool
    public let data: [[String: Any]]
    public let count: Int
    public let total: Int
    public let collection: String

    private enum CodingKeys: String, CodingKey {
        case success, data, count, total, collection
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        success = try container.decode(Bool.self, forKey: .success)
        count = try container.decode(Int.self, forKey: .count)
        total = try container.decode(Int.self, forKey: .total)
        collection = try container.decode(String.self, forKey: .collection)
        let dataArray = try container.decode([[String: Any]].self, forKey: .data)
        data = dataArray
    }
}

public struct QueryOptions: Sendable {
    public let filters: [String: String]?
    public let orderBy: String?
    public let orderDir: String
    public let limit: Int?
    public let offset: Int?

    public init(
        filters: [String: String]? = nil,
        orderBy: String? = nil,
        orderDir: String = "ASC",
        limit: Int? = nil,
        offset: Int? = nil
    ) {
        self.filters = filters
        self.orderBy = orderBy
        self.orderDir = orderDir
        self.limit = limit
        self.offset = offset
    }
}

public struct PushResponse: Codable, Sendable {
    public let success: Bool
    public let data: PushData
    public let message: String?
}

public struct PushData: Codable, Sendable {
    public let id: Int64
    public let collection: String
    public let columnsAdded: [String]

    private enum CodingKeys: String, CodingKey {
        case id, collection
        case columnsAdded = "columns_added"
    }
}

public struct UpdateResponse: Codable, Sendable {
    public let success: Bool
    public let affected: Int
    public let id: Int64
}

public struct DeleteResponse: Codable, Sendable {
    public let success: Bool
    public let affected: Int
    public let id: Int64
}

// ============================================================================
// Storage Models
// ============================================================================

public struct Bucket: Codable, Sendable {
    public let id: Int64
    public let name: String
    public let isPublic: Bool
    public let fileSizeLimit: Int64?
    public let allowedMimeTypes: String?
    public let createdAt: String

    private enum CodingKeys: String, CodingKey {
        case id, name
        case isPublic = "is_public"
        case fileSizeLimit = "file_size_limit"
        case allowedMimeTypes = "allowed_mime_types"
        case createdAt = "created_at"
    }
}

public struct StorageObject: Codable, Sendable {
    public let id: Int64
    public let bucket: String
    public let path: String
    public let size: Int64
    public let mimeType: String
    public let createdAt: String

    private enum CodingKeys: String, CodingKey {
        case id, bucket, path, size
        case mimeType = "mime_type"
        case createdAt = "created_at"
    }
}

// ============================================================================
// RLS Models
// ============================================================================

public struct RlsPolicy: Codable, Sendable {
    public let name: String
    public let table: String
    public let operation: String
    public let permissive: Bool
    public let usingExpression: String?
    public let checkExpression: String?

    private enum CodingKeys: String, CodingKey {
        case name, table, operation, permissive
        case usingExpression = "using_expression"
        case checkExpression = "check_expression"
    }
}

public struct RlsStatus: Codable, Sendable {
    public let table: String
    public let enabled: Bool
    public let policies: [RlsPolicy]
}

// ============================================================================
// Vector Search Models
// ============================================================================

public struct VectorSearchResult: Sendable {
    public let id: Int64
    public let similarity: Double
    public let data: [String: Any]
}

public struct VectorColumnInfo: Codable, Sendable {
    public let table: String
    public let column: String
    public let dimensions: Int
    public let indexType: String
    public let rowCount: Int64

    private enum CodingKeys: String, CodingKey {
        case table, column, dimensions
        case indexType = "index_type"
        case rowCount = "row_count"
    }
}

// ============================================================================
// Realtime Models
// ============================================================================

public struct RealtimeEvent: Sendable {
    public let type: String
    public let table: String
    public let record: [String: Any]?
    public let oldRecord: [String: Any]?
    public let timestamp: String
}

public typealias RealtimeCallback = @Sendable (RealtimeEvent) -> Void

// ============================================================================
// Main Client
// ============================================================================

public actor StackhouseClient {
    public let baseUrl: URL
    private var accessToken: String?
    private var refreshToken: String?
    private var apiKey: String?
    private var session: AuthSession?

    public init(url: String, apiKey: String? = nil) {
        self.baseUrl = URL(string: url)!
        self.apiKey = apiKey
    }

    // MARK: - HTTP Helpers

    internal func buildRequest(url: URL, method: String = "GET", body: Data? = nil) -> URLRequest {
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        if let token = accessToken {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }

        if let key = apiKey {
            request.setValue(key, forHTTPHeaderField: "X-API-Key")
        }

        if let body = body {
            request.httpBody = body
        }

        return request
    }

    internal func executeRequest(url: URL, method: String = "GET", body: Data? = nil) async throws -> Data {
        let request = buildRequest(url: url, method: method, body: body)

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse else {
            throw StackhouseError.networkError("Invalid response")
        }

        guard httpResponse.statusCode < 400 else {
            let errorMessage = String(data: data, encoding: .utf8) ?? HTTPURLResponse.localizedString(forStatusCode: httpResponse.statusCode)
            switch httpResponse.statusCode {
            case 401:
                throw StackhouseError.unauthorized(errorMessage)
            case 404:
                throw StackhouseError.notFound(errorMessage)
            case 400...499:
                throw StackhouseError.validationError(errorMessage)
            case 500...599:
                throw StackhouseError.serverError(httpResponse.statusCode, errorMessage)
            default:
                throw StackhouseError.unknown(errorMessage)
            }
        }

        return data
    }

    // MARK: - Auth

    public var currentSession: AuthSession? {
        return session
    }

    public var currentUser: User? {
        return session?.user
    }

    public var isAuthenticated: Bool {
        return session != nil && !(session?.isExpired ?? true)
    }

    public func signup(email: String, password: String, metadata: [String: Any]? = nil) async throws -> AuthSession {
        var bodyDict: [String: Any] = [
            "email": email,
            "password": password
        ]

        if let metadata = metadata {
            bodyDict["metadata"] = metadata
        }

        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = baseUrl.appendingPathComponent("v1/auth/signup")

        let data = try await executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response format")
        }

        let tokensData = try JSONSerialization.data(withJSONObject: dataDict)
        let tokens = try JSONDecoder().decode(AuthTokens.self, from: tokensData)

        return try updateSession(tokens)
    }

    public func login(email: String, password: String) async throws -> AuthSession {
        let bodyDict: [String: Any] = [
            "email": email,
            "password": password
        ]

        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = baseUrl.appendingPathComponent("v1/auth/login")

        let data = try await executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response format")
        }

        let tokensData = try JSONSerialization.data(withJSONObject: dataDict)
        let tokens = try JSONDecoder().decode(AuthTokens.self, from: tokensData)

        return try updateSession(tokens)
    }

    public func logout() async throws {
        guard let currentRefreshToken = refreshToken else {
            session = nil
            accessToken = nil
            refreshToken = nil
            return
        }

        let bodyDict: [String: Any] = [
            "refresh_token": currentRefreshToken
        ]

        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = baseUrl.appendingPathComponent("v1/auth/logout")

        do {
            _ = try await executeRequest(url: url, method: "POST", body: bodyData)
        } catch {
            print("[Stackhouse] Logout request failed: \(error)")
        }

        session = nil
        accessToken = nil
        refreshToken = nil
    }

    public func refreshAccessToken() async throws -> AuthSession {
        guard let currentRefreshToken = refreshToken else {
            throw StackhouseError.unauthorized("No session to refresh")
        }

        let bodyDict: [String: Any] = [
            "refresh_token": currentRefreshToken
        ]

        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = baseUrl.appendingPathComponent("v1/auth/refresh")

        let data = try await executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response format")
        }

        let tokensData = try JSONSerialization.data(withJSONObject: dataDict)
        let tokens = try JSONDecoder().decode(AuthTokens.self, from: tokensData)

        return try updateSession(tokens)
    }

    /// Get the current authenticated user from the server
    public func getUser() async throws -> User {
        let url = baseUrl.appendingPathComponent("v1/auth/me")
        let data = try await executeRequest(url: url)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response format")
        }

        let userData = try JSONSerialization.data(withJSONObject: dataDict)
        return try JSONDecoder().decode(User.self, from: userData)
    }

    /// Update user metadata
    public func updateUser(metadata: [String: Any]? = nil) async throws -> User {
        var bodyDict: [String: Any] = [:]
        if let metadata = metadata {
            bodyDict["metadata"] = metadata
        }

        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = baseUrl.appendingPathComponent("v1/auth/user")

        let data = try await executeRequest(url: url, method: "PUT", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response format")
        }

        let userData = try JSONSerialization.data(withJSONObject: dataDict)
        return try JSONDecoder().decode(User.self, from: userData)
    }

    /// Change the current user's password
    public func changePassword(currentPassword: String, newPassword: String) async throws {
        let bodyDict: [String: Any] = [
            "current_password": currentPassword,
            "new_password": newPassword
        ]

        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = baseUrl.appendingPathComponent("v1/auth/change-password")

        _ = try await executeRequest(url: url, method: "POST", body: bodyData)
    }

    public func setSession(_ authSession: AuthSession) throws {
        if authSession.isExpired {
            throw StackhouseError.validationError("Session has expired")
        }
        session = authSession
        accessToken = authSession.accessToken
        refreshToken = authSession.refreshToken
    }

    private func updateSession(_ tokens: AuthTokens) throws -> AuthSession {
        let expiresAt = Date().timeIntervalSince1970 + tokens.expiresIn
        let newSession = AuthSession(
            user: tokens.user,
            accessToken: tokens.accessToken,
            refreshToken: tokens.refreshToken,
            expiresAt: expiresAt
        )

        session = newSession
        accessToken = tokens.accessToken
        refreshToken = tokens.refreshToken

        return newSession
    }

    // MARK: - Query

    public func from(_ collection: String) -> QueryBuilder {
        return QueryBuilder(client: self, collection: collection)
    }

    // MARK: - Vector Search

    public func vectors(_ collection: String) -> VectorBuilder {
        return VectorBuilder(client: self, collection: collection)
    }

    // MARK: - Storage

    public nonisolated var storage: StorageClient {
        return StorageClient(client: self)
    }

    // MARK: - RLS

    public func rls(_ table: String) -> RlsClient {
        return RlsClient(client: self, table: table)
    }
}

// ============================================================================
// Storage Client
// ============================================================================

public actor StorageClient {
    private let client: StackhouseClient

    init(client: StackhouseClient) {
        self.client = client
    }

    public func createBucket(name: String, isPublic: Bool = false) async throws -> Bucket {
        let bodyDict: [String: Any] = ["name": name, "public": isPublic]
        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = await client.baseUrl.appendingPathComponent("v1/storage/buckets")

        let data = try await client.executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        let bucketData = try JSONSerialization.data(withJSONObject: dataDict)
        return try JSONDecoder().decode(Bucket.self, from: bucketData)
    }

    public func listBuckets() async throws -> [Bucket] {
        let url = await client.baseUrl.appendingPathComponent("v1/storage/buckets")
        let data = try await client.executeRequest(url: url)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataArray = json["data"] as? [[String: Any]] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        return try dataArray.map { item in
            let itemData = try JSONSerialization.data(withJSONObject: item)
            return try JSONDecoder().decode(Bucket.self, from: itemData)
        }
    }

    public func getBucket(name: String) async throws -> Bucket {
        let url = await client.baseUrl.appendingPathComponent("v1/storage/buckets/\(name)")
        let data = try await client.executeRequest(url: url)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        let bucketData = try JSONSerialization.data(withJSONObject: dataDict)
        return try JSONDecoder().decode(Bucket.self, from: bucketData)
    }

    public func deleteBucket(name: String) async throws {
        let url = await client.baseUrl.appendingPathComponent("v1/storage/buckets/\(name)")
        _ = try await client.executeRequest(url: url, method: "DELETE")
    }

    public func uploadObject(bucket: String, path: String, fileData: Data, mimeType: String) async throws -> StorageObject {
        let url = await client.baseUrl.appendingPathComponent("v1/storage/object/\(bucket)/\(path)")

        let boundary = UUID().uuidString
        var bodyData = Data()

        bodyData.append("--\(boundary)\r\n".data(using: .utf8)!)
        bodyData.append("Content-Disposition: form-data; name=\"file\"; filename=\"\(path.components(separatedBy: "/").last ?? "file")\"\r\n".data(using: .utf8)!)
        bodyData.append("Content-Type: \(mimeType)\r\n\r\n".data(using: .utf8)!)
        bodyData.append(fileData)
        bodyData.append("\r\n--\(boundary)--\r\n".data(using: .utf8)!)

        var request = await client.buildRequest(url: url, method: "POST", body: bodyData)
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode < 400 else {
            throw StackhouseError.serverError(0, "Upload failed")
        }

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        let objData = try JSONSerialization.data(withJSONObject: dataDict)
        return try JSONDecoder().decode(StorageObject.self, from: objData)
    }

    public func downloadObject(bucket: String, path: String) async throws -> Data {
        let url = await client.baseUrl.appendingPathComponent("v1/storage/object/\(bucket)/\(path)")
        return try await client.executeRequest(url: url)
    }

    public func deleteObject(bucket: String, path: String) async throws {
        let url = await client.baseUrl.appendingPathComponent("v1/storage/object/\(bucket)/\(path)")
        _ = try await client.executeRequest(url: url, method: "DELETE")
    }

    public func listObjects(bucket: String, prefix: String? = nil, limit: Int = 100, offset: Int = 0) async throws -> [StorageObject] {
        var urlComponents = URLComponents(url: await client.baseUrl.appendingPathComponent("v1/storage/list/\(bucket)"), resolvingAgainstBaseURL: false)!
        var queryItems = [
            URLQueryItem(name: "limit", value: String(limit)),
            URLQueryItem(name: "offset", value: String(offset))
        ]
        if let prefix = prefix {
            queryItems.append(URLQueryItem(name: "prefix", value: prefix))
        }
        urlComponents.queryItems = queryItems

        guard let url = urlComponents.url else {
            throw StackhouseError.networkError("Invalid URL")
        }

        let data = try await client.executeRequest(url: url)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataArray = json["data"] as? [[String: Any]] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        return try dataArray.map { item in
            let itemData = try JSONSerialization.data(withJSONObject: item)
            return try JSONDecoder().decode(StorageObject.self, from: itemData)
        }
    }
}

// ============================================================================
// RLS Client
// ============================================================================

public actor RlsClient {
    private let client: StackhouseClient
    private let table: String

    init(client: StackhouseClient, table: String) {
        self.client = client
        self.table = table
    }

    public func enable() async throws {
        let url = await client.baseUrl.appendingPathComponent("v1/rls/\(table)/enable")
        _ = try await client.executeRequest(url: url, method: "POST")
    }

    public func disable() async throws {
        let url = await client.baseUrl.appendingPathComponent("v1/rls/\(table)/disable")
        _ = try await client.executeRequest(url: url, method: "POST")
    }

    public func createPolicy(
        name: String,
        operation: String = "ALL",
        permissive: Bool = true,
        usingExpression: String? = nil,
        checkExpression: String? = nil
    ) async throws {
        var bodyDict: [String: Any] = [
            "name": name,
            "operation": operation,
            "permissive": permissive
        ]
        if let using = usingExpression { bodyDict["using_expression"] = using }
        if let check = checkExpression { bodyDict["check_expression"] = check }

        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = await client.baseUrl.appendingPathComponent("v1/rls/\(table)/policies")

        _ = try await client.executeRequest(url: url, method: "POST", body: bodyData)
    }

    public func listPolicies() async throws -> [RlsPolicy] {
        let url = await client.baseUrl.appendingPathComponent("v1/rls/\(table)/policies")
        let data = try await client.executeRequest(url: url)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataArray = json["data"] as? [[String: Any]] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        return try dataArray.map { item in
            let itemData = try JSONSerialization.data(withJSONObject: item)
            return try JSONDecoder().decode(RlsPolicy.self, from: itemData)
        }
    }

    public func dropPolicy(name: String) async throws {
        let url = await client.baseUrl.appendingPathComponent("v1/rls/\(table)/policies/\(name)")
        _ = try await client.executeRequest(url: url, method: "DELETE")
    }

    public func getStatus() async throws -> RlsStatus {
        let url = await client.baseUrl.appendingPathComponent("v1/rls/\(table)/status")
        let data = try await client.executeRequest(url: url)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        let statusData = try JSONSerialization.data(withJSONObject: dataDict)
        return try JSONDecoder().decode(RlsStatus.self, from: statusData)
    }
}

// ============================================================================
// Realtime Client (URLSessionWebSocketTask)
// ============================================================================

public final class RealtimeClient: @unchecked Sendable {
    private let client: StackhouseClient
    private var task: URLSessionWebSocketTask?
    private var connected = false
    private var reconnectAttempts = 0
    private let maxReconnectAttempts = 10
    private var subscriptions: [(table: String, event: String, callback: RealtimeCallback)] = []
    private let lock = NSLock()

    public var isConnected: Bool { connected }

    init(client: StackhouseClient) {
        self.client = client
    }

    public func connect() async throws {
        let baseUrl = await client.baseUrl
        let wsUrl = baseUrl.absoluteString
            .replacingOccurrences(of: "http:", with: "ws:")
            .replacingOccurrences(of: "https:", with: "wss:") + "/v1/realtime"

        guard let url = URL(string: wsUrl) else {
            throw StackhouseError.networkError("Invalid WebSocket URL")
        }

        let request = await client.buildRequest(url: url)
        task = URLSession.shared.webSocketTask(with: request)
        task?.resume()

        connected = true
        reconnectAttempts = 0

        // Re-subscribe to all existing subscriptions
        lock.lock()
        let currentSubs = subscriptions
        lock.unlock()
        for sub in currentSubs {
            sendSubscribe(table: sub.table, event: sub.event)
        }

        // Start listening
        listenForMessages()
    }

    public func on(table: String, event: String, callback: @escaping RealtimeCallback) -> () -> Void {
        let entry = (table: table, event: event, callback: callback)
        lock.lock()
        subscriptions.append(entry)
        lock.unlock()

        if connected {
            sendSubscribe(table: table, event: event)
        }

        return { [weak self] in
            guard let self = self else { return }
            self.lock.lock()
            self.subscriptions.removeAll { $0.table == table && $0.event == event }
            let hasOtherSubs = self.subscriptions.contains { $0.table == table }
            self.lock.unlock()

            if self.connected && !hasOtherSubs {
                self.sendUnsubscribe(table: table)
            }
        }
    }

    public func disconnect() {
        lock.lock()
        subscriptions.removeAll()
        lock.unlock()
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        connected = false
    }

    private func sendSubscribe(table: String, event: String) {
        let dict: [String: Any] = ["type": "subscribe", "table": table, "event": event]
        if let data = try? JSONSerialization.data(withJSONObject: dict),
           let str = String(data: data, encoding: .utf8) {
            task?.send(.string(str)) { _ in }
        }
    }

    private func sendUnsubscribe(table: String) {
        let dict: [String: Any] = ["type": "unsubscribe", "table": table]
        if let data = try? JSONSerialization.data(withJSONObject: dict),
           let str = String(data: data, encoding: .utf8) {
            task?.send(.string(str)) { _ in }
        }
    }

    private func listenForMessages() {
        task?.receive { [weak self] result in
            guard let self = self else { return }

            switch result {
            case .success(let message):
                switch message {
                case .string(let text):
                    self.handleMessage(text)
                default:
                    break
                }
                // Continue listening
                self.listenForMessages()

            case .failure:
                self.connected = false
                self.attemptReconnect()
            }
        }
    }

    private func handleMessage(_ text: String) {
        guard let data = text.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = json["type"] as? String,
              (type == "INSERT" || type == "UPDATE" || type == "DELETE") else {
            return
        }

        let event = RealtimeEvent(
            type: type,
            table: json["table"] as? String ?? "",
            record: json["record"] as? [String: Any],
            oldRecord: json["old_record"] as? [String: Any],
            timestamp: json["timestamp"] as? String ?? ""
        )

        lock.lock()
        let matchingSubs = subscriptions.filter {
            $0.table == event.table && ($0.event == "*" || $0.event == event.type)
        }
        lock.unlock()

        for sub in matchingSubs {
            sub.callback(event)
        }
    }

    private func attemptReconnect() {
        guard reconnectAttempts < maxReconnectAttempts else { return }
        reconnectAttempts += 1
        let delay = min(Double(1 << (reconnectAttempts - 1)), 30.0)

        DispatchQueue.global().asyncAfter(deadline: .now() + delay) { [weak self] in
            Task {
                try? await self?.connect()
            }
        }
    }
}

// ============================================================================
// Vector Builder
// ============================================================================

public actor VectorBuilder {
    public let client: StackhouseClient
    public let collection: String

    public init(client: StackhouseClient, collection: String) {
        self.client = client
        self.collection = collection
    }

    public func search(
        queryVector: [Double],
        topK: Int = 10,
        metric: String = "cosine",
        filters: [String: Any]? = nil,
        column: String = "embedding"
    ) async throws -> [VectorSearchResult] {
        var bodyDict: [String: Any] = [
            "vector": queryVector,
            "top_k": topK,
            "metric": metric,
            "column": column
        ]

        if let filters = filters {
            bodyDict["filters"] = filters
        }

        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = await client.baseUrl.appendingPathComponent("v1/vectors/\(collection)/search")

        let data = try await client.executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let resultsArray = json["data"] as? [[String: Any]] else {
            throw StackhouseError.decodingError("Invalid vector search response")
        }

        return resultsArray.map { item in
            VectorSearchResult(
                id: item["id"] as? Int64 ?? 0,
                similarity: item["similarity"] as? Double ?? 0.0,
                data: item["data"] as? [String: Any] ?? [:]
            )
        }
    }

    public func upsert(
        embedding: [Double],
        id: Int64? = nil,
        data: [String: Any]? = nil,
        column: String = "embedding"
    ) async throws -> (id: Int64, collection: String, dimensions: Int) {
        var bodyDict: [String: Any] = [
            "embedding": embedding,
            "column": column
        ]

        if let id = id { bodyDict["id"] = id }
        if let data = data { bodyDict["data"] = data }

        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = await client.baseUrl.appendingPathComponent("v1/vectors/\(collection)/upsert")

        let responseData = try await client.executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              let dataDict = json["data"] as? [String: Any],
              let recordId = dataDict["id"] as? Int64,
              let collectionName = dataDict["collection"] as? String,
              let dimensions = dataDict["dimensions"] as? Int else {
            throw StackhouseError.decodingError("Invalid vector upsert response")
        }

        return (recordId, collectionName, dimensions)
    }

    public func info() async throws -> [VectorColumnInfo] {
        let url = await client.baseUrl.appendingPathComponent("v1/vectors/\(collection)/info")

        let data = try await client.executeRequest(url: url)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let resultsArray = json["data"] as? [[String: Any]] else {
            throw StackhouseError.decodingError("Invalid vector info response")
        }

        return try resultsArray.map { item in
            let itemData = try JSONSerialization.data(withJSONObject: item)
            return try JSONDecoder().decode(VectorColumnInfo.self, from: itemData)
        }
    }
}

// ============================================================================
// Query Builder
// ============================================================================

public actor QueryBuilder {
    public let client: StackhouseClient
    public let collection: String

    public init(client: StackhouseClient, collection: String) {
        self.client = client
        self.collection = collection
    }

    public func select(options: QueryOptions? = nil) async throws -> QueryResult {
        var urlComponents = URLComponents(url: await client.baseUrl.appendingPathComponent("v1/query/\(collection)"), resolvingAgainstBaseURL: false)!

        var queryItems: [URLQueryItem] = []

        options?.filters?.forEach { key, value in
            queryItems.append(URLQueryItem(name: key, value: value))
        }

        if let orderBy = options?.orderBy {
            queryItems.append(URLQueryItem(name: "order_by", value: orderBy))
            queryItems.append(URLQueryItem(name: "order_dir", value: options?.orderDir ?? "ASC"))
        }

        if let limit = options?.limit {
            queryItems.append(URLQueryItem(name: "limit", value: String(limit)))
        }

        if let offset = options?.offset {
            queryItems.append(URLQueryItem(name: "offset", value: String(offset)))
        }

        if !queryItems.isEmpty {
            urlComponents.queryItems = queryItems
        }

        guard let url = urlComponents.url else {
            throw StackhouseError.networkError("Invalid URL")
        }

        let data = try await client.executeRequest(url: url)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        let resultData = try JSONSerialization.data(withJSONObject: json)
        return try JSONDecoder().decode(QueryResult.self, from: resultData)
    }

    public func getById(id: String) async throws -> [String: Any] {
        let url = await client.baseUrl.appendingPathComponent("v1/query/\(collection)/\(id)")

        let data = try await client.executeRequest(url: url)

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        return dataDict
    }

    public func insert(_ data: [String: Any]) async throws -> PushData {
        let url = await client.baseUrl.appendingPathComponent("v1/push/\(collection)")

        let bodyData = try JSONSerialization.data(withJSONObject: data)
        let responseData = try await client.executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: responseData) as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        let resultData = try JSONSerialization.data(withJSONObject: json)
        let response = try JSONDecoder().decode(PushResponse.self, from: resultData)

        return response.data
    }

    public func insertBatch(_ data: [[String: Any]]) async throws -> (inserted: Int, collection: String, columnsAdded: [String]) {
        let url = await client.baseUrl.appendingPathComponent("v1/push/\(collection)/batch")

        let bodyData = try JSONSerialization.data(withJSONObject: data)
        let responseData = try await client.executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              let dataDict = json["data"] as? [String: Any] else {
            throw StackhouseError.decodingError("Invalid response")
        }

        guard let inserted = dataDict["inserted"] as? Int,
              let collection = dataDict["collection"] as? String,
              let columnsAdded = dataDict["columns_added"] as? [String] else {
            throw StackhouseError.decodingError("Invalid response format")
        }

        return (inserted, collection, columnsAdded)
    }

    public func update(id: String, _ data: [String: Any]) async throws -> Int {
        let url = await client.baseUrl.appendingPathComponent("v1/update/\(collection)/\(id)")

        let bodyData = try JSONSerialization.data(withJSONObject: data)
        let responseData = try await client.executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              let affected = json["affected"] as? Int else {
            throw StackhouseError.decodingError("Invalid response format")
        }

        return affected
    }

    public func delete(id: String) async throws -> Int {
        let url = await client.baseUrl.appendingPathComponent("v1/delete/\(collection)/\(id)")

        let responseData = try await client.executeRequest(url: url, method: "POST")

        guard let json = try JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              let affected = json["affected"] as? Int else {
            throw StackhouseError.decodingError("Invalid response format")
        }

        return affected
    }

    /// Bulk delete with optional filters
    public func bulkDelete(filters: [String: Any]? = nil) async throws -> Int {
        let bodyDict: [String: Any] = ["filters": filters ?? [:]]
        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = await client.baseUrl.appendingPathComponent("v1/delete/\(collection)")

        let responseData = try await client.executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              let affected = json["affected"] as? Int else {
            throw StackhouseError.decodingError("Invalid response format")
        }

        return affected
    }

    /// Bulk update with data and optional filters
    public func bulkUpdate(_ data: [String: Any], filters: [String: Any]? = nil) async throws -> Int {
        let bodyDict: [String: Any] = ["data": data, "filters": filters ?? [:]]
        let bodyData = try JSONSerialization.data(withJSONObject: bodyDict)
        let url = await client.baseUrl.appendingPathComponent("v1/update/\(collection)")

        let responseData = try await client.executeRequest(url: url, method: "POST", body: bodyData)

        guard let json = try JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              let affected = json["affected"] as? Int else {
            throw StackhouseError.decodingError("Invalid response format")
        }

        return affected
    }

    /// Drop the entire table
    public func dropTable() async throws {
        let url = await client.baseUrl.appendingPathComponent("v1/tables/\(collection)")
        _ = try await client.executeRequest(url: url, method: "DELETE")
    }
}
