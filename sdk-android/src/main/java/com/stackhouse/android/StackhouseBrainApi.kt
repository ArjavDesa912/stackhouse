package com.stackhouse.android

import retrofit2.http.POST
import retrofit2.http.GET
import retrofit2.http.Body
import retrofit2.http.Path
import retrofit2.Call

interface StackhouseBrainApi {
    @POST("/v1/brain/chat")
    fun chat(@Body request: ChatRequest): Call<ChatResponse>

    @POST("/v1/agent/task")
    fun submitTask(@Body task: AgentTask): Call<TaskResponse>
    
    @GET("/v1/connectors")
    fun listConnectors(): Call<ConnectorsResponse>
}

data class ChatRequest(val session_id: String, val message: String)
data class ChatResponse(val success: Boolean, val data: Map<String, Any>)
data class AgentTask(val goal: String, val agent_type: String?)
data class TaskResponse(val success: Boolean, val data: Map<String, Any>)
data class ConnectorsResponse(val success: Boolean, val data: List<Map<String, Any>>)
