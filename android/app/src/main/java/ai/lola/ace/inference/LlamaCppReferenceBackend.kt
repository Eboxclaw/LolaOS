package ai.lola.ace.inference

import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.isActive
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Temporary CPU reference backend. The app depends only on [LocalInferenceBackend];
 * llama.cpp is isolated in this class and can be replaced by the LOLA Native Engine.
 *
 * The native implementation uses one model handle and one llama context per ACE session.
 * Chat history is re-evaluated from the supplied message list for this first correctness
 * baseline; incremental KV-cache reuse is deliberately a later optimization.
 */
class LlamaCppReferenceBackend : LocalInferenceBackend {
    private val resourceMutex = Mutex()
    private val sessionHandles = ConcurrentHashMap<String, Long>()
    private val activeGenerations = ConcurrentHashMap<String, Job>()
    private val mutableState = MutableStateFlow<LocalInferenceState>(LocalInferenceState.Unloaded)

    override val state: StateFlow<LocalInferenceState> = mutableState.asStateFlow()

    @Volatile
    private var modelHandle: Long = 0L

    @Volatile
    private var nativeLibraryLoaded = false

    private fun ensureNativeLibraryLoaded() {
        if (nativeLibraryLoaded) return
        synchronized(this) {
            if (!nativeLibraryLoaded) {
                System.loadLibrary("lola-reference-llama")
                nativeLibraryLoaded = true
            }
        }
    }

    override suspend fun loadModel(modelFile: File) {
        require(modelFile.isFile && modelFile.canRead() && modelFile.length() > 0L) {
            "O ficheiro GGUF instalado não existe ou não pode ser lido."
        }
        require(modelFile.extension.equals("gguf", ignoreCase = true)) {
            "O backend de referência aceita apenas GGUF."
        }

        resourceMutex.withLock {
            unloadModelLocked()
            mutableState.value = LocalInferenceState.Loading
            try {
                withContext(NonCancellable + Dispatchers.IO) {
                    ensureNativeLibraryLoaded()
                    nativeInit()
                    val newHandle = nativeLoadModel(modelFile.canonicalPath)
                    check(newHandle != 0L) {
                        "llama.cpp devolveu um identificador de modelo inválido."
                    }
                    // Commit the native handle before returning to a possibly-cancelled UI caller.
                    modelHandle = newHandle
                    mutableState.value = LocalInferenceState.Loaded(modelFile.name)
                }
            } catch (error: CancellationException) {
                mutableState.value = if (modelHandle != 0L) {
                    LocalInferenceState.Loaded(modelFile.name)
                } else {
                    LocalInferenceState.Unloaded
                }
                throw error
            } catch (error: LinkageError) {
                modelHandle = 0L
                mutableState.value = LocalInferenceState.Error(
                    error.message ?: "A biblioteca nativa não pôde ser carregada.",
                )
                throw error
            } catch (error: Exception) {
                modelHandle = 0L
                mutableState.value = LocalInferenceState.Error(
                    error.message ?: "Não foi possível carregar o modelo.",
                )
                throw error
            }
        }
    }

    override suspend fun openSession(
        sessionId: String,
        config: ReferenceSessionConfig,
    ) {
        require(sessionId.isNotBlank()) { "O identificador da sessão não pode estar vazio." }
        resourceMutex.withLock {
            check(modelHandle != 0L) { "Carrega primeiro um modelo GGUF." }
            if (sessionHandles.containsKey(sessionId)) return@withLock
            withContext(NonCancellable + Dispatchers.IO) {
                val handle = nativeCreateSession(
                    modelHandle,
                    config.contextTokens,
                    config.threads,
                    config.temperature,
                    config.topK,
                    config.repeatPenalty,
                    config.seed,
                )
                check(handle != 0L) { "llama.cpp não conseguiu criar a sessão." }
                // Register ownership before returning to a possibly-cancelled UI caller.
                sessionHandles[sessionId] = handle
            }
        }
    }

    override fun generate(
        sessionId: String,
        messages: List<InferenceChatMessage>,
        maxOutputTokens: Int,
    ) = callbackFlow {
        require(messages.isNotEmpty()) { "A conversa não contém mensagens." }
        require(maxOutputTokens in 1..1024) { "O limite de saída tem de estar entre 1 e 1024 tokens." }

        val handle = sessionHandles[sessionId]
        if (handle == null) {
            close(IllegalStateException("A sessão $sessionId não está aberta."))
            return@callbackFlow
        }

        val producer = this
        val cancelled = AtomicBoolean(false)

        val sink = object : NativeTokenSink {
            override fun onToken(token: String) {
                if (token.isNotEmpty()) producer.trySend(token)
            }

            override fun isCancelled(): Boolean =
                cancelled.get() || !producer.isActive
        }

        val roles = messages.map { it.role }.toTypedArray()
        val contents = messages.map { it.content }.toTypedArray()

        val generationJob = launch(Dispatchers.IO, start = CoroutineStart.LAZY) {
            try {
                val result = nativeGenerate(
                    handle,
                    roles,
                    contents,
                    maxOutputTokens,
                    sink,
                )
                if (result < 0) {
                    close(IllegalStateException("A geração local terminou com erro ($result)."))
                } else {
                    close()
                }
            } catch (error: Throwable) {
                close(error)
            }
        }

        if (activeGenerations.putIfAbsent(sessionId, generationJob) != null) {
            generationJob.cancel()
            close(IllegalStateException("A sessão $sessionId já está a gerar texto."))
            return@callbackFlow
        }

        generationJob.invokeOnCompletion {
            activeGenerations.remove(sessionId, generationJob)
        }
        // Reset only after this generation owns the session; a rejected second request
        // must never clear a cancellation already sent to the active generation.
        nativeResetCancellation(handle)
        generationJob.start()

        awaitClose {
            cancelled.set(true)
            nativeCancel(handle)
            generationJob.cancel()
        }
    }.buffer(Channel.UNLIMITED)

    override fun cancelGeneration(sessionId: String) {
        sessionHandles[sessionId]?.let(::nativeCancel)
        activeGenerations[sessionId]?.cancel()
    }

    override suspend fun closeSession(sessionId: String) {
        resourceMutex.withLock {
            val handle = sessionHandles[sessionId] ?: return
            nativeCancel(handle)
            activeGenerations[sessionId]?.cancelAndJoin()
            activeGenerations.remove(sessionId)
            withContext(NonCancellable + Dispatchers.IO) {
                nativeCloseSession(handle)
                sessionHandles.remove(sessionId, handle)
            }
        }
    }

    override suspend fun unloadModel() {
        resourceMutex.withLock {
            unloadModelLocked()
        }
    }

    private suspend fun unloadModelLocked() {
        val sessions = sessionHandles.toMap()
        for ((sessionId, handle) in sessions) {
            nativeCancel(handle)
            activeGenerations[sessionId]?.cancelAndJoin()
            activeGenerations.remove(sessionId)
            withContext(NonCancellable + Dispatchers.IO) {
                nativeCloseSession(handle)
                sessionHandles.remove(sessionId, handle)
            }
        }

        val handle = modelHandle
        if (handle != 0L) {
            withContext(NonCancellable + Dispatchers.IO) {
                nativeUnloadModel(handle)
                if (modelHandle == handle) modelHandle = 0L
                mutableState.value = LocalInferenceState.Unloaded
            }
        }
        if (modelHandle == 0L) mutableState.value = LocalInferenceState.Unloaded
    }

    private interface NativeTokenSink {
        fun onToken(token: String)
        fun isCancelled(): Boolean
    }

    private external fun nativeInit()
    private external fun nativeLoadModel(modelPath: String): Long
    private external fun nativeCreateSession(
        modelHandle: Long,
        contextTokens: Int,
        threads: Int,
        temperature: Float,
        topK: Int,
        repeatPenalty: Float,
        seed: Long,
    ): Long
    private external fun nativeGenerate(
        sessionHandle: Long,
        roles: Array<String>,
        contents: Array<String>,
        maxOutputTokens: Int,
        sink: NativeTokenSink,
    ): Int
    private external fun nativeResetCancellation(sessionHandle: Long)
    private external fun nativeCancel(sessionHandle: Long)
    private external fun nativeCloseSession(sessionHandle: Long)
    private external fun nativeUnloadModel(modelHandle: Long)
}
