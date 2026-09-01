import Foundation

public struct ChatRequest: Codable {
    public let sessionId: String
    public let message: String
    
    enum CodingKeys: String, CodingKey {
        case sessionId = "session_id"
        case message
    }
}

public struct AgentTask: Codable {
    public let goal: String
    public let agentType: String?
    
    enum CodingKeys: String, CodingKey {
        case goal
        case agentType = "agent_type"
    }
}

public class StackhouseBrain {
    private let url: URL
    private let session = URLSession.shared
    
    public init(url: URL) {
        self.url = url.appendingPathComponent("v1/brain")
    }
    
    public func chat(request: ChatRequest) async throws -> [String: Any]? {
        var req = URLRequest(url: url.appendingPathComponent("chat"))
        req.httpMethod = "POST"
        req.addValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONEncoder().encode(request)
        
        let (data, _) = try await session.data(for: req)
        let json = try JSONSerialization.jsonObject(with: data, options: []) as? [String: Any]
        return json?["data"] as? [String: Any]
    }
}
