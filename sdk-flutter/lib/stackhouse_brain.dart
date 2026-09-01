import 'dart:convert';
import 'package:http/http.dart' as http;

class ChatRequest {
  final String sessionId;
  final String message;

  ChatRequest({required this.sessionId, required this.message});

  Map<String, dynamic> toJson() => {
    'session_id': sessionId,
    'message': message,
  };
}

class AgentTask {
  final String goal;
  final String? agentType;

  AgentTask({required this.goal, this.agentType});

  Map<String, dynamic> toJson() => {
    'goal': goal,
    'agent_type': agentType,
  };
}

class StackhouseBrain {
  final String url;
  
  StackhouseBrain(this.url);

  Future<Map<String, dynamic>> chat(ChatRequest request) async {
    final response = await http.post(
      Uri.parse('$url/v1/brain/chat'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode(request.toJson()),
    );
    
    if (response.statusCode == 200) {
      return jsonDecode(response.body);
    } else {
      throw Exception('Failed to chat with AI Brain');
    }
  }

  Future<Map<String, dynamic>> submitTask(AgentTask task) async {
    final response = await http.post(
      Uri.parse('$url/v1/agent/task'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode(task.toJson()),
    );
    
    if (response.statusCode == 200) {
      return jsonDecode(response.body);
    } else {
      throw Exception('Failed to submit agent task');
    }
  }
}
