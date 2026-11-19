package ai.lola.ace.inference

object LocalInferenceBackendProvider {
    @Volatile
    private var instance: LocalInferenceBackend? = null

    fun get(): LocalInferenceBackend =
        instance ?: synchronized(this) {
            instance ?: LlamaCppReferenceBackend().also { instance = it }
        }
}
