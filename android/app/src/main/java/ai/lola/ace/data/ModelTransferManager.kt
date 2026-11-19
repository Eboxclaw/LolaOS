package ai.lola.ace.data

import android.content.Context
import android.os.Handler
import android.os.Looper
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

enum class ModelTransferStage {
    AVAILABLE,
    DOWNLOADING,
    VERIFYING,
    INSTALLED,
    PAUSED,
    CANCELLED,
    DELETING,
    FAILED,
}

data class ModelTransferSnapshot(
    val modelId: String,
    val stage: ModelTransferStage,
    val bytesDownloaded: Long = 0L,
    val totalBytes: Long? = null,
    val message: String? = null,
) {
    val progress: Float?
        get() = totalBytes?.takeIf { it > 0L }?.let {
            (bytesDownloaded.toFloat() / it.toFloat()).coerceIn(0f, 1f)
        }
}

class ModelTransferManager(context: Context) {
    private val appContext = context.applicationContext
    private val store = ModelStore(appContext)
    private val executor = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "lola-model-transfer").apply { isDaemon = true }
    }
    private val mainHandler = Handler(Looper.getMainLooper())
    private val active = ConcurrentHashMap<String, TransferTask>()
    private val deleting = ConcurrentHashMap.newKeySet<String>()
    private val listeners = CopyOnWriteArrayList<(ModelTransferSnapshot) -> Unit>()

    fun subscribe(listener: (ModelTransferSnapshot) -> Unit): AutoCloseable {
        listeners.add(listener)
        return AutoCloseable { listeners.remove(listener) }
    }

    fun snapshot(model: ModelArtifactSummary): ModelTransferSnapshot {
        active[model.id]?.let { return it.snapshot }
        if (deleting.contains(model.id)) {
            return ModelTransferSnapshot(model.id, ModelTransferStage.DELETING)
        }
        if (store.installedRecord(model) != null) {
            return ModelTransferSnapshot(model.id, ModelTransferStage.INSTALLED)
        }
        val partialBytes = runCatching { store.partialFile(model).length() }.getOrDefault(0L)
        return if (partialBytes > 0L) {
            ModelTransferSnapshot(
                modelId = model.id,
                stage = ModelTransferStage.PAUSED,
                bytesDownloaded = partialBytes,
                message = "Transferência interrompida; podes retomá-la.",
            )
        } else {
            ModelTransferSnapshot(model.id, ModelTransferStage.AVAILABLE)
        }
    }

    @Synchronized
    fun download(model: ModelArtifactSummary): Boolean {
        try {
            ModelDownloadPolicy.validateArtifact(model)
        } catch (error: Exception) {
            publish(
                ModelTransferSnapshot(
                    modelId = model.id,
                    stage = ModelTransferStage.FAILED,
                    message = error.message ?: "Metadados do modelo inválidos",
                ),
            )
            return false
        }

        if (deleting.contains(model.id)) return false

        if (store.installedRecord(model) != null) {
            publish(ModelTransferSnapshot(model.id, ModelTransferStage.INSTALLED))
            return false
        }

        val task = TransferTask(model.id)
        if (active.putIfAbsent(model.id, task) != null) return false

        val existingBytes = runCatching { store.partialFile(model).length() }.getOrDefault(0L)
        update(task,
            ModelTransferSnapshot(
                modelId = model.id,
                stage = ModelTransferStage.DOWNLOADING,
                bytesDownloaded = existingBytes,
                message = "A preparar a transferência…",
            ),
        )
        executor.execute { performDownload(model, task) }
        return true
    }

    fun cancel(modelId: String): Boolean {
        val task = active[modelId] ?: return false
        task.cancel()
        return true
    }

    @Synchronized
    fun delete(model: ModelArtifactSummary): Boolean {
        if (active.containsKey(model.id) || !deleting.add(model.id)) return false

        publish(ModelTransferSnapshot(model.id, ModelTransferStage.DELETING))
        executor.execute {
            try {
                store.deleteInstalled(model)
                publish(ModelTransferSnapshot(model.id, ModelTransferStage.AVAILABLE))
            } catch (error: Exception) {
                publish(
                    ModelTransferSnapshot(
                        modelId = model.id,
                        stage = ModelTransferStage.FAILED,
                        message = error.message ?: "Não foi possível remover o modelo",
                    ),
                )
            } finally {
                deleting.remove(model.id)
            }
        }
        return true
    }

    private fun performDownload(model: ModelArtifactSummary, task: TransferTask) {
        val partFile = store.partialFile(model)
        try {
            // Recover safely if the process stopped after atomic promotion but before registry commit.
            recoverPromotedFile(model, task)

            if (store.installedRecord(model) != null) {
                task.update(ModelTransferSnapshot(model.id, ModelTransferStage.INSTALLED))
                return
            }

            check(task.isCancelled()) { "Transferência cancelada" }
            val initialOffset = partFile.length()
            if (initialOffset > MAX_ARTIFACT_BYTES) {
                throw IOException("O ficheiro parcial excede o limite local de 8 GiB.")
            }

            val freeBytes = store.availableBytes()
            val minimumFreeBytes = if (initialOffset == 0L) {
                MINIMUM_FREE_BYTES
            } else {
                MINIMUM_RESUME_FREE_BYTES
            }
            if (freeBytes < minimumFreeBytes) {
                val requirement = if (initialOffset == 0L) "1,5 GiB" else "256 MiB"
                throw IOException("Espaço insuficiente. São necessários pelo menos $requirement livres.")
            }

            val connection = openFollowingRedirects(model.downloadUrl, initialOffset, task)
            task.connection = connection
            try {
                val responseCode = connection.responseCode
                val expectedTotal = when (responseCode) {
                    HttpURLConnection.HTTP_OK -> {
                        // A server that ignores Range returns the whole file; overwrite the old partial.
                        connection.contentLengthLong.takeIf { it >= 0L }
                    }
                    HttpURLConnection.HTTP_PARTIAL -> {
                        val range = ModelDownloadPolicy.parseContentRange(
                            connection.getHeaderField("Content-Range"),
                        ) ?: throw IOException("O servidor enviou um Content-Range inválido.")
                        if (range.start != initialOffset) {
                            throw IOException("O servidor retomou a transferência no byte errado.")
                        }
                        range.totalBytes
                    }
                    416 -> {
                        val total = ModelDownloadPolicy.parseUnsatisfiedRangeTotal(
                            connection.getHeaderField("Content-Range"),
                        )
                        if (total != null && total == initialOffset) {
                            total
                        } else {
                            throw IOException("O servidor recusou o pedido de retoma (HTTP 416).")
                        }
                    }
                    else -> throw IOException("Falha HTTP ${responseCode}: ${connection.responseMessage}")
                }

                if (expectedTotal != null && expectedTotal > MAX_ARTIFACT_BYTES) {
                    throw IOException("O artefacto excede o limite local de 8 GiB.")
                }

                if (responseCode != 416) {
                    val append = responseCode == HttpURLConnection.HTTP_PARTIAL
                    val startingBytes = if (append) initialOffset else 0L
                    var downloaded = startingBytes
                    val lastSnapshot = longArrayOf(0L)

                    connection.inputStream.use { input ->
                        FileOutputStream(partFile, append).use { output ->
                            val buffer = ByteArray(BUFFER_SIZE)
                            while (true) {
                                check(task.isCancelled()) { "Transferência cancelada" }
                                val count = input.read(buffer)
                                if (count < 0) break
                                if (count == 0) continue

                                downloaded += count.toLong()
                                if (downloaded > MAX_ARTIFACT_BYTES) {
                                    throw IOException("O artefacto excede o limite local de 8 GiB.")
                                }
                                if (expectedTotal != null && downloaded > expectedTotal) {
                                    throw IOException("O servidor enviou mais bytes do que anunciou.")
                                }

                                output.write(buffer, 0, count)
                                val now = System.currentTimeMillis()
                                if (now - lastSnapshot[0] >= PROGRESS_INTERVAL_MS) {
                                    lastSnapshot[0] = now
                                    update(task,
                                        ModelTransferSnapshot(
                                            modelId = model.id,
                                            stage = ModelTransferStage.DOWNLOADING,
                                            bytesDownloaded = downloaded,
                                            totalBytes = expectedTotal,
                                        ),
                                    )
                                }
                            }
                            output.flush()
                            output.fd.sync()
                        }
                    }

                    if (expectedTotal != null && partFile.length() != expectedTotal) {
                        throw IOException(
                            "Transferência incompleta: ${partFile.length()} de $expectedTotal bytes.",
                        )
                    }
                }
            } finally {
                connection.disconnect()
                task.connection = null
            }

            check(task.isCancelled()) { "Transferência cancelada" }
            update(task,
                ModelTransferSnapshot(
                    modelId = model.id,
                    stage = ModelTransferStage.VERIFYING,
                    bytesDownloaded = partFile.length(),
                    totalBytes = partFile.length(),
                    message = "A verificar SHA-256…",
                ),
            )

            val verified = hashFile(partFile, task)
            if (!verified.second.equals(model.sha256, ignoreCase = true)) {
                if (partFile.exists() && !partFile.delete()) {
                    throw IOException("O SHA-256 não corresponde ao catálogo e o ficheiro inválido não pôde ser removido.")
                }
                throw IOException("O SHA-256 do ficheiro não corresponde ao catálogo. A transferência foi descartada.")
            }

            check(task.isCancelled()) { "Transferência cancelada" }
            store.promoteVerifiedArtifact(
                model = model,
                partFile = partFile,
                sizeBytes = verified.first,
                sha256 = verified.second,
            )
            task.update(
                ModelTransferSnapshot(
                    modelId = model.id,
                    stage = ModelTransferStage.INSTALLED,
                    bytesDownloaded = verified.first,
                    totalBytes = verified.first,
                    message = "Modelo verificado e instalado.",
                ),
            )
        } catch (error: Exception) {
            val wasCancelled = task.isCancelled()
            task.update(
                ModelTransferSnapshot(
                    modelId = model.id,
                    stage = if (wasCancelled) ModelTransferStage.CANCELLED else ModelTransferStage.FAILED,
                    bytesDownloaded = runCatching { partFile.length() }.getOrDefault(0L),
                    message = if (wasCancelled) {
                        "Transferência cancelada. Os dados parciais foram mantidos para retoma."
                    } else {
                        error.message ?: "Falha na transferência do modelo"
                    },
                ),
            )
        } finally {
            task.connection?.disconnect()
            task.connection = null
            active.remove(model.id, task)
            publish(task.snapshot)
        }
    }

    private fun recoverPromotedFile(model: ModelArtifactSummary, task: TransferTask) {
        val destination = store.destinationFile(model)
        if (!destination.isFile || store.installedRecord(model) != null) return

        task.update(
            ModelTransferSnapshot(
                modelId = model.id,
                stage = ModelTransferStage.VERIFYING,
                bytesDownloaded = destination.length(),
                totalBytes = destination.length(),
                message = "A recuperar uma instalação interrompida…",
            ),
        )
        val verified = hashFile(destination, task)
        if (verified.second.equals(model.sha256, ignoreCase = true)) {
            store.registerInstalled(model, destination, verified.first, verified.second)
        } else {
            check(destination.delete()) { "Não foi possível remover um artefacto local inválido." }
        }
    }

    private fun openFollowingRedirects(
        source: String,
        offset: Long,
        task: TransferTask,
    ): HttpURLConnection {
        var current = URL(source)
        for (redirectCount in 0..MAX_REDIRECTS) {
            check(task.isCancelled()) { "Transferência cancelada" }
            check(current.protocol.equals("https", ignoreCase = true)) {
                "A transferência só aceita HTTPS."
            }
            val connection = (current.openConnection() as HttpURLConnection).apply {
                instanceFollowRedirects = false
                connectTimeout = CONNECT_TIMEOUT_MS
                readTimeout = READ_TIMEOUT_MS
                requestMethod = "GET"
                useCaches = false
                setRequestProperty("Accept-Encoding", "identity")
                setRequestProperty("User-Agent", "LOLA-ACE-Android/0.1")
                if (offset > 0L) setRequestProperty("Range", "bytes=$offset-")
            }
            task.connection = connection
            val code = connection.responseCode
            if (code in REDIRECT_CODES) {
                val location = connection.getHeaderField("Location")
                    ?: throw IOException("Redireccionamento sem destino.")
                connection.disconnect()
                if (redirectCount >= MAX_REDIRECTS) {
                    throw IOException("Demasiados redireccionamentos durante a transferência.")
                }
                current = URL(current, location)
            } else {
                return connection
            }
        }
        throw IOException("Não foi possível abrir a transferência.")
    }

    private fun hashFile(file: File, task: TransferTask): Pair<Long, String> {
        val digest = MessageDigest.getInstance("SHA-256")
        var totalBytes = 0L
        FileInputStream(file).use { input ->
            val buffer = ByteArray(BUFFER_SIZE)
            while (true) {
                check(task.isCancelled()) { "Transferência cancelada" }
                val count = input.read(buffer)
                if (count < 0) break
                if (count == 0) continue
                totalBytes += count.toLong()
                digest.update(buffer, 0, count)
            }
        }
        return totalBytes to digest.digest().joinToString("") { byte -> "%02x".format(byte.toInt() and 0xff) }
    }

    private fun update(task: TransferTask, snapshot: ModelTransferSnapshot) {
        task.update(snapshot)
        publish(snapshot)
    }

    private fun publish(snapshot: ModelTransferSnapshot) {
        mainHandler.post {
            listeners.forEach { listener ->
                runCatching { listener(snapshot) }
            }
        }
    }

    private class TransferTask(val modelId: String) {
        private val cancelled = AtomicBoolean(false)

        @Volatile
        var connection: HttpURLConnection? = null

        @Volatile
        var snapshot: ModelTransferSnapshot =
            ModelTransferSnapshot(modelId, ModelTransferStage.DOWNLOADING)

        fun isCancelled(): Boolean = cancelled.get()

        fun cancel() {
            cancelled.set(true)
            connection?.disconnect()
        }

        fun update(next: ModelTransferSnapshot) {
            snapshot = next
        }
    }

    companion object {
        private const val BUFFER_SIZE = 128 * 1024
        private const val PROGRESS_INTERVAL_MS = 250L
        private const val CONNECT_TIMEOUT_MS = 20_000
        private const val READ_TIMEOUT_MS = 60_000
        private const val MAX_REDIRECTS = 5
        private const val MAX_ARTIFACT_BYTES = 8L * 1024L * 1024L * 1024L
        private const val MINIMUM_FREE_BYTES = 1536L * 1024L * 1024L
        private const val MINIMUM_RESUME_FREE_BYTES = 256L * 1024L * 1024L
        private val REDIRECT_CODES = setOf(301, 302, 303, 307, 308)
    }
}

object ModelTransferManagerProvider {
    @Volatile
    private var instance: ModelTransferManager? = null

    fun get(context: Context): ModelTransferManager =
        instance ?: synchronized(this) {
            instance ?: ModelTransferManager(context.applicationContext).also { instance = it }
        }
}
