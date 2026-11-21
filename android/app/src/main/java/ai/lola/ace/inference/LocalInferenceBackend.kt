package ai.lola.ace.inference

import java.io.File
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

data class InferenceChatMessage(
    val role: String,
    val content: String,
) {
    init {
        require(role in setOf("system", "user", "assistant")) {
            "Unsupported chat role: $role"
        }
        require(content.isNotBlank()) { "Chat message cannot be blank" }
    }
}

data class InferenceRunMetrics(
    val promptTokens: Int,
    val generatedTokens: Int,
    val prefillMillis: Long,
    val timeToFirstTokenMillis: Long,
    val decodeMillis: Long,
    val totalMillis: Long,
) {
    init {
        require(promptTokens >= 0)
        require(generatedTokens >= 0)
        require(prefillMillis >= 0L)
        require(timeToFirstTokenMillis >= 0L)
        require(decodeMillis >= 0L)
        require(totalMillis >= 0L)
    }

    val decodeTokensPerSecond: Double?
        get() = if (decodeMillis > 0L && generatedTokens > 0) {
            generatedTokens * 1000.0 / decodeMillis.toDouble()
        } else {
            null
        }
}

data class ReferenceSessionConfig(
    val contextTokens: Int = 2048,
    val threads: Int = 4,
    val temperature: Float = 0.1f,
    val topK: Int = 50,
    val repeatPenalty: Float = 1.1f,
    val seed: Long = 42L,
) {
    init {
        require(contextTokens in 512..8192)
        require(threads in 1..8)
        require(temperature > 0f && temperature <= 2f)
        require(topK > 0)
        require(repeatPenalty in 1f..2f)
    }
}

sealed interface LocalInferenceState {
    data object Unloaded : LocalInferenceState
    data object Loading : LocalInferenceState
    data class Loaded(val modelFileName: String) : LocalInferenceState
    data class Error(val message: String) : LocalInferenceState
}

/**
 * Android-side boundary consumed by the app. The implementation is replaceable:
 * this interface does not expose llama.cpp types, C pointers or backend-specific settings.
 */
interface LocalInferenceBackend {
    val state: StateFlow<LocalInferenceState>
    val latestMetrics: StateFlow<InferenceRunMetrics?>

    suspend fun loadModel(modelFile: File)
    suspend fun openSession(sessionId: String, config: ReferenceSessionConfig = ReferenceSessionConfig())

    /** Stream only assistant text; maxOutputTokens is a host-owned hard limit. */
    fun generate(
        sessionId: String,
        messages: List<InferenceChatMessage>,
        maxOutputTokens: Int,
    ): Flow<String>

    fun cancelGeneration(sessionId: String)
    suspend fun closeSession(sessionId: String)
    suspend fun unloadModel()
}
