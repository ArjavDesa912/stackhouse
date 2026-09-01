package com.stackhouse

import retrofit2.Retrofit
import retrofit2.converter.gson.GsonConverterFactory
import retrofit2.http.*
import okhttp3.OkHttpClient
import okhttp3.Interceptor
import okhttp3.Request
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.Response as OkResponse
import okhttp3.RequestBody
import okhttp3.MultipartBody
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.RequestBody.Companion.toRequestBody
import com.google.gson.Gson
import com.google.gson.JsonObject
import com.google.gson.annotations.SerializedName
import kotlinx.coroutines.suspendCancellableCoroutine
import retrofit2.Call
import retrofit2.Callback
import retrofit2.Response
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.util.concurrent.TimeUnit

// ============================================================================
// Data Classes
// ============================================================================

data class User(
    @SerializedName("id") val id: Long,
    @SerializedName("email") val email: String,
    @SerializedName("created_at") val createdAt: String,
    @SerializedName("updated_at") val updatedAt: String,
    @SerializedName("metadata") val metadata: JsonObject? = null
)

data class AuthTokens(
    @SerializedName("access_token") val accessToken: String,
    @SerializedName("refresh_token") val refreshToken: String,
    @SerializedName("expires_in") val expiresIn: Long,
    @SerializedName("token_type") val tokenType: String,
    @SerializedName("user") val user: User
)

data class AuthSession(
    val user: User,
    val accessToken: String,
    val refreshToken: String,
    val expiresAt: Long
)

data class QueryOptions(
    val filters: Map<String, String>? = null,
    val orderBy: String? = null,
    val orderDir: String = "ASC",
    val limit: Int? = null,
    val offset: Int? = null
)

data class QueryResult(
    val success: Boolean,
    val data: List<Map<String, Any>>,
    val count: Int,
    val total: Int,
    val collection: String
)

data class PushResponse(
    val success: Boolean,
    val data: PushData,
    val message: String? = null
)

data class PushData(
    @SerializedName("id") val id: Long,
    @SerializedName("collection") val collection: String,
    @SerializedName("columns_added") val columnsAdded: List<String>
)

data class UpdateResponse(
    val success: Boolean,
    val affected: Int,
    val id: Long
)

data class DeleteResponse(
    val success: Boolean,
    val affected: Int,
    val id: Long
)

data class BulkOpResponse(
    val success: Boolean,
    val affected: Int
)

data class DropTableResponse(
    val success: Boolean,
    val message: String
)

data class BatchPushResponse(
    val success: Boolean,
    val data: BatchPushData
)

data class BatchPushData(
    @SerializedName("inserted") val inserted: Long,
    @SerializedName("collection") val collection: String,
    @SerializedName("columns_added") val columnsAdded: List<String>
)

// Vector Search Data Classes
data class VectorSearchRequest(
    val vector: List<Double>,
    @SerializedName("top_k") val topK: Int = 10,
    val metric: String = "cosine",
    val filters: JsonObject? = null,
    val column: String = "embedding"
)

data class VectorSearchResult(
    val id: Long,
    val similarity: Double,
    val data: Map<String, Any>
)

data class VectorSearchResponse(
    val success: Boolean,
    val data: List<VectorSearchResult>,
    val count: Int,
    val collection: String
)

data class VectorUpsertRequest(
    val embedding: List<Double>,
    val id: Long? = null,
    val data: JsonObject? = null,
    val column: String = "embedding"
)

data class VectorUpsertResponse(
    val success: Boolean,
    val data: VectorUpsertData,
    val message: String? = null
)

data class VectorUpsertData(
    val id: Long,
    val collection: String,
    val dimensions: Int
)

data class VectorBatchUpsertRequest(
    val records: List<VectorUpsertRequest>
)

data class VectorBatchUpsertResponse(
    val success: Boolean,
    val data: VectorBatchData,
    val message: String? = null
)

data class VectorBatchData(
    val ids: List<Long>,
    val collection: String,
    val count: Int
)

data class VectorInfoResponse(
    val success: Boolean,
    val data: List<VectorColumnInfo>
)

data class VectorColumnInfo(
    val table: String,
    val column: String,
    val dimensions: Int,
    @SerializedName("index_type") val indexType: String,
    @SerializedName("row_count") val rowCount: Long
)

// Storage Data Classes
data class Bucket(
    val id: Long,
    val name: String,
    @SerializedName("is_public") val isPublic: Boolean = false,
    @SerializedName("file_size_limit") val fileSizeLimit: Long? = null,
    @SerializedName("allowed_mime_types") val allowedMimeTypes: String? = null,
    @SerializedName("created_at") val createdAt: String
)

data class BucketResponse(val success: Boolean, val data: Bucket)
data class BucketsResponse(val success: Boolean, val data: List<Bucket>)

data class StorageObject(
    val id: Long,
    val bucket: String,
    val path: String,
    val size: Long,
    @SerializedName("mime_type") val mimeType: String,
    @SerializedName("created_at") val createdAt: String
)

data class StorageObjectResponse(val success: Boolean, val data: StorageObject)
data class StorageObjectsResponse(val success: Boolean, val data: List<StorageObject>)

// RLS Data Classes
data class RlsPolicy(
    val name: String,
    val table: String,
    val operation: String,
    val permissive: Boolean = true,
    @SerializedName("using_expression") val usingExpression: String? = null,
    @SerializedName("check_expression") val checkExpression: String? = null
)

data class RlsPoliciesResponse(val success: Boolean, val data: List<RlsPolicy>)

data class RlsStatus(
    val table: String,
    val enabled: Boolean,
    val policies: List<RlsPolicy>
)

data class RlsStatusResponse(val success: Boolean, val data: RlsStatus)

// Realtime Data Classes
data class RealtimeEvent(
    val type: String,
    val table: String,
    val record: Map<String, Any>? = null,
    @SerializedName("old_record") val oldRecord: Map<String, Any>? = null,
    val timestamp: String = ""
)

typealias RealtimeCallback = (RealtimeEvent) -> Unit

sealed class StackhouseError(message: String, open val statusCode: Int? = null) : Exception(message) {
    class NetworkError(message: String) : StackhouseError(message)
    class Unauthorized(message: String) : StackhouseError(message, 401)
    class NotFound(message: String) : StackhouseError(message, 404)
    class ServerError(message: String, statusCode: Int) : StackhouseError(message, statusCode)
    class ValidationError(message: String) : StackhouseError(message, 400)
    class Unknown(message: String) : StackhouseError(message)
}

// ============================================================================
// Retrofit API Interfaces
// ============================================================================

interface StackhouseApi {
    @GET("/v1/query/{collection}")
    suspend fun query(
        @Path("collection") collection: String,
        @QueryMap params: Map<String, String>? = null
    ): QueryResult

    @GET("/v1/query/{collection}/{id}")
    suspend fun getById(
        @Path("collection") collection: String,
        @Path("id") id: String
    ): JsonObject

    @POST("/v1/push/{collection}")
    suspend fun insert(
        @Path("collection") collection: String,
        @Body data: JsonObject
    ): PushResponse

    @POST("/v1/push/{collection}/batch")
    suspend fun insertBatch(
        @Path("collection") collection: String,
        @Body data: List<JsonObject>
    ): BatchPushResponse

    @POST("/v1/update/{collection}/{id}")
    suspend fun update(
        @Path("collection") collection: String,
        @Path("id") id: String,
        @Body data: JsonObject
    ): UpdateResponse

    @POST("/v1/delete/{collection}/{id}")
    suspend fun delete(
        @Path("collection") collection: String,
        @Path("id") id: String
    ): DeleteResponse

    // Bulk operations
    @POST("/v1/delete/{collection}")
    suspend fun bulkDelete(
        @Path("collection") collection: String,
        @Body body: JsonObject
    ): BulkOpResponse

    @POST("/v1/update/{collection}")
    suspend fun bulkUpdate(
        @Path("collection") collection: String,
        @Body body: JsonObject
    ): BulkOpResponse

    @DELETE("/v1/tables/{collection}")
    suspend fun dropTable(
        @Path("collection") collection: String
    ): DropTableResponse

    // Auth
    @POST("/v1/auth/signup")
    suspend fun signup(@Body credentials: JsonObject): AuthTokens

    @POST("/v1/auth/login")
    suspend fun login(@Body credentials: JsonObject): AuthTokens

    @POST("/v1/auth/logout")
    suspend fun logout(@Body body: JsonObject)

    @POST("/v1/auth/refresh")
    suspend fun refresh(@Body body: JsonObject): AuthTokens

    @GET("/v1/auth/me")
    suspend fun getMe(): JsonObject

    @PUT("/v1/auth/user")
    suspend fun updateUser(@Body body: JsonObject): JsonObject

    @POST("/v1/auth/change-password")
    suspend fun changePassword(@Body body: JsonObject): JsonObject

    // Vector Search endpoints
    @POST("/v1/vectors/{collection}/search")
    suspend fun vectorSearch(
        @Path("collection") collection: String,
        @Body request: VectorSearchRequest
    ): VectorSearchResponse

    @POST("/v1/vectors/{collection}/upsert")
    suspend fun vectorUpsert(
        @Path("collection") collection: String,
        @Body request: VectorUpsertRequest
    ): VectorUpsertResponse

    @POST("/v1/vectors/{collection}/batch")
    suspend fun vectorBatchUpsert(
        @Path("collection") collection: String,
        @Body request: VectorBatchUpsertRequest
    ): VectorBatchUpsertResponse

    @GET("/v1/vectors/{collection}/info")
    suspend fun vectorInfo(
        @Path("collection") collection: String
    ): VectorInfoResponse

    // Storage endpoints
    @POST("/v1/storage/buckets")
    suspend fun createBucket(@Body body: JsonObject): BucketResponse

    @GET("/v1/storage/buckets")
    suspend fun listBuckets(): BucketsResponse

    @GET("/v1/storage/buckets/{name}")
    suspend fun getBucket(@Path("name") name: String): BucketResponse

    @DELETE("/v1/storage/buckets/{name}")
    suspend fun deleteBucket(@Path("name") name: String): JsonObject

    @DELETE("/v1/storage/object/{bucket}/{path}")
    suspend fun deleteObject(
        @Path("bucket") bucket: String,
        @Path("path", encoded = true) path: String
    ): JsonObject

    @GET("/v1/storage/list/{bucket}")
    suspend fun listObjects(
        @Path("bucket") bucket: String,
        @QueryMap params: Map<String, String>? = null
    ): StorageObjectsResponse

    // RLS endpoints
    @POST("/v1/rls/{table}/enable")
    suspend fun rlsEnable(@Path("table") table: String): JsonObject

    @POST("/v1/rls/{table}/disable")
    suspend fun rlsDisable(@Path("table") table: String): JsonObject

    @POST("/v1/rls/{table}/policies")
    suspend fun rlsCreatePolicy(
        @Path("table") table: String,
        @Body body: JsonObject
    ): JsonObject

    @GET("/v1/rls/{table}/policies")
    suspend fun rlsListPolicies(@Path("table") table: String): RlsPoliciesResponse

    @DELETE("/v1/rls/{table}/policies/{name}")
    suspend fun rlsDropPolicy(
        @Path("table") table: String,
        @Path("name") name: String
    ): JsonObject

    @GET("/v1/rls/{table}/status")
    suspend fun rlsStatus(@Path("table") table: String): RlsStatusResponse
}

// ============================================================================
// Main Client
// ============================================================================

class StackhouseClient(baseUrl: String, apiKey: String? = null) {
    private val api: StackhouseApi
    internal val client: OkHttpClient
    private val gson = Gson()
    internal val cleanUrl: String

    var session: AuthSession? = null
        private set

    /** Storage operations */
    val storage: StorageClient by lazy { StorageClient(api, client, cleanUrl) { session } }

    /** Realtime subscriptions */
    val realtime: RealtimeClient by lazy { RealtimeClient(client, cleanUrl) { session } }

    init {
        cleanUrl = baseUrl.removeSuffix("/")

        client = OkHttpClient.Builder()
            .connectTimeout(30, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .writeTimeout(30, TimeUnit.SECONDS)
            .addInterceptor(Interceptor { chain ->
                val builder = chain.request().newBuilder()
                session?.let {
                    builder.addHeader("Authorization", "Bearer ${it.accessToken}")
                }
                apiKey?.let {
                    builder.addHeader("X-API-Key", it)
                }
                chain.proceed(builder.build())
            })
            .build()

        val retrofit = Retrofit.Builder()
            .baseUrl(cleanUrl)
            .client(client)
            .addConverterFactory(GsonConverterFactory.create(gson))
            .build()

        api = retrofit.create(StackhouseApi::class.java)
    }

    // ============================================================================
    // Auth Methods
    // ============================================================================

    suspend fun signup(email: String, password: String, metadata: JsonObject? = null): AuthSession {
        val body = JsonObject().apply {
            addProperty("email", email)
            addProperty("password", password)
            metadata?.let { add("metadata", it) }
        }

        return try {
            val tokens = api.signup(body)
            updateSession(tokens)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun login(email: String, password: String): AuthSession {
        val body = JsonObject().apply {
            addProperty("email", email)
            addProperty("password", password)
        }

        return try {
            val tokens = api.login(body)
            updateSession(tokens)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun logout() {
        session?.let { currentSession ->
            try {
                api.logout(JsonObject().apply {
                    addProperty("refresh_token", currentSession.refreshToken)
                })
            } catch (e: Exception) {
                println("[Stackhouse] Logout request failed: ${e.message}")
            }
        }
        session = null
    }

    suspend fun refreshAccessToken(): AuthSession {
        val currentSession = session
            ?: throw StackhouseError.Unauthorized("No session to refresh")

        val body = JsonObject().apply {
            addProperty("refresh_token", currentSession.refreshToken)
        }

        return try {
            val tokens = api.refresh(body)
            updateSession(tokens)
        } catch (e: Exception) {
            session = null
            throw handleError(e)
        }
    }

    /** Get the current authenticated user from the server */
    suspend fun getUser(): User {
        return try {
            val response = api.getMe()
            val data = response.getAsJsonObject("data")
            gson.fromJson(data, User::class.java)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    /** Update user metadata */
    suspend fun updateUser(metadata: JsonObject? = null): User {
        val body = JsonObject().apply {
            metadata?.let { add("metadata", it) }
        }
        return try {
            val response = api.updateUser(body)
            val data = response.getAsJsonObject("data")
            gson.fromJson(data, User::class.java)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    /** Change password */
    suspend fun changePassword(currentPassword: String, newPassword: String) {
        val body = JsonObject().apply {
            addProperty("current_password", currentPassword)
            addProperty("new_password", newPassword)
        }
        try {
            api.changePassword(body)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    fun setSession(authSession: AuthSession) {
        if (authSession.expiresAt <= System.currentTimeMillis()) {
            throw StackhouseError.ValidationError("Session has expired")
        }
        session = authSession
    }

    private fun updateSession(tokens: AuthTokens): AuthSession {
        val expiresAt = System.currentTimeMillis() + (tokens.expiresIn * 1000)
        val newSession = AuthSession(
            user = tokens.user,
            accessToken = tokens.accessToken,
            refreshToken = tokens.refreshToken,
            expiresAt = expiresAt
        )
        session = newSession
        return newSession
    }

    // ============================================================================
    // Query Methods
    // ============================================================================

    fun from(collection: String): QueryBuilder {
        return QueryBuilder(api, collection)
    }

    // ============================================================================
    // Vector Search Methods
    // ============================================================================

    fun vectors(collection: String): VectorBuilder {
        return VectorBuilder(api, collection)
    }

    // ============================================================================
    // RLS Methods
    // ============================================================================

    /** Get an RLS client for a table */
    fun rls(table: String): RlsClient {
        return RlsClient(api, table)
    }

    // ============================================================================
    // Error Handling
    // ============================================================================

    internal fun handleError(e: Exception): StackhouseError {
        return when (e) {
            is retrofit2.HttpException -> {
                val statusCode = e.code()
                val message = e.message() ?: "HTTP error"
                when (statusCode) {
                    401 -> StackhouseError.Unauthorized(message)
                    404 -> StackhouseError.NotFound(message)
                    in 400..499 -> StackhouseError.ValidationError(message)
                    in 500..599 -> StackhouseError.ServerError(message, statusCode)
                    else -> StackhouseError.Unknown(message)
                }
            }
            is java.net.UnknownHostException, is java.net.SocketTimeoutException -> {
                StackhouseError.NetworkError(e.message ?: "Network error")
            }
            else -> StackhouseError.Unknown(e.message ?: "Unknown error")
        }
    }
}

// ============================================================================
// Query Builder
// ============================================================================

class QueryBuilder(private val api: StackhouseApi, private val collection: String) {

    suspend fun select(options: QueryOptions? = null): QueryResult {
        val params = mutableMapOf<String, String>()

        options?.let { opts ->
            opts.filters?.forEach { (key, value) ->
                params[key] = value
            }
            opts.orderBy?.let {
                params["order_by"] = it
                params["order_dir"] = opts.orderDir
            }
            opts.limit?.let {
                params["limit"] = it.toString()
            }
            opts.offset?.let {
                params["offset"] = it.toString()
            }
        }

        return try {
            api.query(collection, params.ifEmpty { null })
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun getById(id: String): Map<String, Any> {
        return try {
            val response = api.getById(collection, id)
            val dataObj = response.getAsJsonObject("data")
            convertJsonToMap(dataObj)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun insert(data: Map<String, Any>): PushData {
        val jsonObject = convertMapToJson(data)
        return try {
            val response = api.insert(collection, jsonObject)
            response.data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun insertBatch(data: List<Map<String, Any>>): BatchPushData {
        val jsonArray = data.map { convertMapToJson(it) }
        return try {
            val response = api.insertBatch(collection, jsonArray)
            response.data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun update(id: String, data: Map<String, Any>): Int {
        val jsonObject = convertMapToJson(data)
        return try {
            val response = api.update(collection, id, jsonObject)
            response.affected
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun delete(id: String): Int {
        return try {
            val response = api.delete(collection, id)
            response.affected
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    /** Bulk delete with optional filters */
    suspend fun bulkDelete(filters: Map<String, Any>? = null): Int {
        val body = JsonObject().apply {
            add("filters", convertMapToJson(filters ?: emptyMap()))
        }
        return try {
            val response = api.bulkDelete(collection, body)
            response.affected
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    /** Bulk update with data and optional filters */
    suspend fun bulkUpdate(data: Map<String, Any>, filters: Map<String, Any>? = null): Int {
        val body = JsonObject().apply {
            add("data", convertMapToJson(data))
            add("filters", convertMapToJson(filters ?: emptyMap()))
        }
        return try {
            val response = api.bulkUpdate(collection, body)
            response.affected
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    /** Drop the entire table */
    suspend fun dropTable(): String {
        return try {
            val response = api.dropTable(collection)
            response.message
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    private fun handleError(e: Exception): StackhouseError {
        return when (e) {
            is retrofit2.HttpException -> {
                val statusCode = e.code()
                val message = e.message() ?: "HTTP error"
                when (statusCode) {
                    401 -> StackhouseError.Unauthorized(message)
                    404 -> StackhouseError.NotFound(message)
                    in 400..499 -> StackhouseError.ValidationError(message)
                    in 500..599 -> StackhouseError.ServerError(message, statusCode)
                    else -> StackhouseError.Unknown(message)
                }
            }
            is java.net.UnknownHostException, is java.net.SocketTimeoutException -> {
                StackhouseError.NetworkError(e.message ?: "Network error")
            }
            else -> StackhouseError.Unknown(e.message ?: "Unknown error")
        }
    }

    private fun convertMapToJson(map: Map<String, Any>): JsonObject {
        val jsonObject = JsonObject()
        map.forEach { (key, value) ->
            when (value) {
                is String -> jsonObject.addProperty(key, value)
                is Number -> jsonObject.addProperty(key, value)
                is Boolean -> jsonObject.addProperty(key, value)
                is Map<*, *> -> {
                    @Suppress("UNCHECKED_CAST")
                    jsonObject.add(key, convertMapToJson(value as Map<String, Any>))
                }
                is List<*> -> {
                    jsonObject.add(key, convertListToJsonArray(value))
                }
                else -> jsonObject.addProperty(key, value.toString())
            }
        }
        return jsonObject
    }

    private fun convertListToJsonArray(list: List<*>): com.google.gson.JsonArray {
        val jsonArray = com.google.gson.JsonArray()
        list.forEach { item ->
            when (item) {
                is String -> jsonArray.add(item)
                is Number -> jsonArray.add(item)
                is Boolean -> jsonArray.add(item)
                is Map<*, *> -> {
                    @Suppress("UNCHECKED_CAST")
                    jsonArray.add(convertMapToJson(item as Map<String, Any>))
                }
                is List<*> -> jsonArray.add(convertListToJsonArray(item))
                else -> jsonArray.add(item.toString())
            }
        }
        return jsonArray
    }

    private fun convertJsonToMap(jsonObj: JsonObject?): Map<String, Any> {
        if (jsonObj == null) return emptyMap()
        val map = mutableMapOf<String, Any>()
        jsonObj.entrySet().forEach { entry ->
            val value = when (val element = entry.value) {
                is com.google.gson.JsonPrimitive -> {
                    when {
                        element.isBoolean -> element.asBoolean
                        element.isNumber -> element.asNumber
                        element.isString -> element.asString
                        else -> element.toString()
                    }
                }
                is com.google.gson.JsonObject -> convertJsonToMap(element)
                is com.google.gson.JsonArray -> convertJsonToArray(element)
                else -> element.toString()
            }
            map[entry.key] = value
        }
        return map
    }

    private fun convertJsonToArray(jsonArray: com.google.gson.JsonArray): List<Any> {
        val list = mutableListOf<Any>()
        jsonArray.forEach { item ->
            when (item) {
                is com.google.gson.JsonPrimitive -> {
                    when {
                        item.isBoolean -> list.add(item.asBoolean)
                        item.isNumber -> list.add(item.asNumber)
                        item.isString -> list.add(item.asString)
                        else -> list.add(item.toString())
                    }
                }
                is com.google.gson.JsonObject -> list.add(convertJsonToMap(item))
                is com.google.gson.JsonArray -> list.add(convertJsonToArray(item))
                else -> list.add(item.toString())
            }
        }
        return list
    }
}

// ============================================================================
// Vector Builder
// ============================================================================

class VectorBuilder(private val api: StackhouseApi, private val collection: String) {

    suspend fun search(
        queryVector: List<Double>,
        topK: Int = 10,
        metric: String = "cosine",
        filters: JsonObject? = null,
        column: String = "embedding"
    ): List<VectorSearchResult> {
        val request = VectorSearchRequest(
            vector = queryVector,
            topK = topK,
            metric = metric,
            filters = filters,
            column = column
        )
        return try {
            val response = api.vectorSearch(collection, request)
            response.data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun upsert(
        embedding: List<Double>,
        id: Long? = null,
        data: JsonObject? = null,
        column: String = "embedding"
    ): VectorUpsertData {
        val request = VectorUpsertRequest(
            embedding = embedding,
            id = id,
            data = data,
            column = column
        )
        return try {
            val response = api.vectorUpsert(collection, request)
            response.data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun batchUpsert(records: List<VectorUpsertRequest>): VectorBatchData {
        val request = VectorBatchUpsertRequest(records = records)
        return try {
            val response = api.vectorBatchUpsert(collection, request)
            response.data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun info(): List<VectorColumnInfo> {
        return try {
            val response = api.vectorInfo(collection)
            response.data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    private fun handleError(e: Exception): StackhouseError {
        return when (e) {
            is retrofit2.HttpException -> {
                val statusCode = e.code()
                val message = e.message() ?: "HTTP error"
                when (statusCode) {
                    401 -> StackhouseError.Unauthorized(message)
                    404 -> StackhouseError.NotFound(message)
                    in 400..499 -> StackhouseError.ValidationError(message)
                    in 500..599 -> StackhouseError.ServerError(message, statusCode)
                    else -> StackhouseError.Unknown(message)
                }
            }
            is java.net.UnknownHostException, is java.net.SocketTimeoutException -> {
                StackhouseError.NetworkError(e.message ?: "Network error")
            }
            else -> StackhouseError.Unknown(e.message ?: "Unknown error")
        }
    }
}

// ============================================================================
// Storage Client
// ============================================================================

class StorageClient(
    private val api: StackhouseApi,
    private val client: OkHttpClient,
    private val baseUrl: String,
    private val sessionProvider: () -> AuthSession?
) {
    suspend fun createBucket(name: String, isPublic: Boolean = false): Bucket {
        val body = JsonObject().apply {
            addProperty("name", name)
            addProperty("public", isPublic)
        }
        return try {
            api.createBucket(body).data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun listBuckets(): List<Bucket> {
        return try {
            api.listBuckets().data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun getBucket(name: String): Bucket {
        return try {
            api.getBucket(name).data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun deleteBucket(name: String) {
        try {
            api.deleteBucket(name)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    /** Upload a file (uses OkHttp directly for multipart) */
    suspend fun uploadObject(bucket: String, path: String, data: ByteArray, mimeType: String): StorageObject {
        return withContext(Dispatchers.IO) {
            val requestBody = MultipartBody.Builder()
                .setType(MultipartBody.FORM)
                .addFormDataPart(
                    "file",
                    path.substringAfterLast("/"),
                    data.toRequestBody(mimeType.toMediaTypeOrNull())
                )
                .build()

            val request = Request.Builder()
                .url("$baseUrl/v1/storage/object/$bucket/$path")
                .post(requestBody)
                .apply {
                    sessionProvider()?.let {
                        addHeader("Authorization", "Bearer ${it.accessToken}")
                    }
                }
                .build()

            val response = client.newCall(request).execute()
            if (!response.isSuccessful) {
                throw StackhouseError.ServerError("Upload failed: ${response.code}", response.code)
            }

            val result = Gson().fromJson(
                response.body?.string() ?: "{}",
                StorageObjectResponse::class.java
            )
            result.data
        }
    }

    /** Download a file */
    suspend fun downloadObject(bucket: String, path: String): ByteArray {
        return withContext(Dispatchers.IO) {
            val request = Request.Builder()
                .url("$baseUrl/v1/storage/object/$bucket/$path")
                .get()
                .apply {
                    sessionProvider()?.let {
                        addHeader("Authorization", "Bearer ${it.accessToken}")
                    }
                }
                .build()

            val response = client.newCall(request).execute()
            if (!response.isSuccessful) {
                throw StackhouseError.ServerError("Download failed: ${response.code}", response.code)
            }
            response.body?.bytes() ?: ByteArray(0)
        }
    }

    suspend fun deleteObject(bucket: String, path: String) {
        try {
            api.deleteObject(bucket, path)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun listObjects(bucket: String, prefix: String? = null, limit: Int = 100, offset: Int = 0): List<StorageObject> {
        val params = mutableMapOf(
            "limit" to limit.toString(),
            "offset" to offset.toString()
        )
        prefix?.let { params["prefix"] = it }

        return try {
            api.listObjects(bucket, params).data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    private fun handleError(e: Exception): StackhouseError {
        return when (e) {
            is StackhouseError -> e
            is retrofit2.HttpException -> StackhouseError.ServerError(e.message() ?: "HTTP error", e.code())
            else -> StackhouseError.Unknown(e.message ?: "Unknown error")
        }
    }
}

// ============================================================================
// RLS Client
// ============================================================================

class RlsClient(private val api: StackhouseApi, private val table: String) {

    suspend fun enable() {
        try {
            api.rlsEnable(table)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun disable() {
        try {
            api.rlsDisable(table)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun createPolicy(
        name: String,
        operation: String = "ALL",
        permissive: Boolean = true,
        usingExpression: String? = null,
        checkExpression: String? = null
    ) {
        val body = JsonObject().apply {
            addProperty("name", name)
            addProperty("operation", operation)
            addProperty("permissive", permissive)
            usingExpression?.let { addProperty("using_expression", it) }
            checkExpression?.let { addProperty("check_expression", it) }
        }
        try {
            api.rlsCreatePolicy(table, body)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun listPolicies(): List<RlsPolicy> {
        return try {
            api.rlsListPolicies(table).data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun dropPolicy(policyName: String) {
        try {
            api.rlsDropPolicy(table, policyName)
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    suspend fun getStatus(): RlsStatus {
        return try {
            api.rlsStatus(table).data
        } catch (e: Exception) {
            throw handleError(e)
        }
    }

    private fun handleError(e: Exception): StackhouseError {
        return when (e) {
            is StackhouseError -> e
            is retrofit2.HttpException -> StackhouseError.ServerError(e.message() ?: "HTTP error", e.code())
            else -> StackhouseError.Unknown(e.message ?: "Unknown error")
        }
    }
}

// ============================================================================
// Realtime Client (OkHttp WebSocket)
// ============================================================================

class RealtimeClient(
    private val client: OkHttpClient,
    private val baseUrl: String,
    private val sessionProvider: () -> AuthSession?
) {
    private var ws: WebSocket? = null
    private var connected = false
    private var reconnectAttempts = 0
    private val maxReconnectAttempts = 10
    private val subscriptions = mutableListOf<Subscription>()
    private val gson = Gson()

    val isConnected: Boolean get() = connected

    private data class Subscription(
        val table: String,
        val event: String,
        val callback: RealtimeCallback
    )

    /** Connect to the Stackhouse realtime WebSocket server */
    fun connect() {
        val wsUrl = baseUrl
            .replace("http:", "ws:")
            .replace("https:", "wss:") + "/v1/realtime"

        val request = Request.Builder()
            .url(wsUrl)
            .apply {
                sessionProvider()?.let {
                    addHeader("Authorization", "Bearer ${it.accessToken}")
                }
            }
            .build()

        ws = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: OkResponse) {
                connected = true
                reconnectAttempts = 0
                // Re-subscribe to all existing subscriptions
                for (sub in subscriptions) {
                    sendSubscribe(sub.table, sub.event)
                }
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                try {
                    val json = gson.fromJson(text, JsonObject::class.java)
                    val type = json.get("type")?.asString ?: return

                    if (type == "INSERT" || type == "UPDATE" || type == "DELETE") {
                        val event = gson.fromJson(json, RealtimeEvent::class.java)
                        for (sub in subscriptions) {
                            if (sub.table == event.table &&
                                (sub.event == "*" || sub.event == event.type)) {
                                sub.callback(event)
                            }
                        }
                    }
                } catch (_: Exception) {}
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                connected = false
                attemptReconnect()
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: OkResponse?) {
                connected = false
                attemptReconnect()
            }
        })
    }

    /** Subscribe to changes on a table. Returns unsubscribe lambda. */
    fun on(table: String, event: String, callback: RealtimeCallback): () -> Unit {
        val sub = Subscription(table, event, callback)
        subscriptions.add(sub)

        if (connected) {
            sendSubscribe(table, event)
        }

        return {
            subscriptions.remove(sub)
            if (connected) {
                val hasOtherSubs = subscriptions.any { it.table == table }
                if (!hasOtherSubs) {
                    sendUnsubscribe(table)
                }
            }
        }
    }

    /** Disconnect from the realtime server */
    fun disconnect() {
        subscriptions.clear()
        ws?.close(1000, "Client disconnect")
        ws = null
        connected = false
    }

    private fun sendSubscribe(table: String, event: String) {
        ws?.send(gson.toJson(mapOf("type" to "subscribe", "table" to table, "event" to event)))
    }

    private fun sendUnsubscribe(table: String) {
        ws?.send(gson.toJson(mapOf("type" to "unsubscribe", "table" to table)))
    }

    private fun attemptReconnect() {
        if (reconnectAttempts >= maxReconnectAttempts) return
        reconnectAttempts++
        val delay = minOf(1000L * (1L shl (reconnectAttempts - 1)), 30000L)

        Thread {
            Thread.sleep(delay)
            connect()
        }.start()
    }
}
