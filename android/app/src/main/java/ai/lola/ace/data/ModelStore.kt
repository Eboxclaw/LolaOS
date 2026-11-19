package ai.lola.ace.data

import android.content.Context
import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileNotFoundException

data class InstalledModelRecord(
    val modelId: String,
    val revision: String,
    val fileName: String,
    val sizeBytes: Long,
    val sha256: String,
    val installedAtEpochMillis: Long,
)

class ModelStore(context: Context) {
    private val appContext = context.applicationContext
    private val root = File(appContext.filesDir, "models")
    private val staging = File(root, ".staging")
    private val registry = AtomicFile(File(appContext.filesDir, "model-registry.json"))

    init {
        check(root.mkdirs() || root.isDirectory) { "Could not create the private model directory" }
        check(staging.mkdirs() || staging.isDirectory) { "Could not create the model staging directory" }
    }

    fun availableBytes(): Long = android.os.StatFs(appContext.filesDir.absolutePath).availableBytes

    fun partialFile(model: ModelArtifactSummary): File {
        validatePathSegments(model)
        return safeChild(staging, "${model.id}-${model.revision}.part")
    }

    fun destinationFile(model: ModelArtifactSummary): File {
        validatePathSegments(model)
        val modelDirectory = safeChild(root, model.id)
        val revisionDirectory = safeChild(modelDirectory, model.revision)
        return safeChild(revisionDirectory, model.fileName)
    }

    @Synchronized
    fun installedRecord(model: ModelArtifactSummary): InstalledModelRecord? {
        val record = readRegistry()[model.id] ?: return null
        if (record.revision != model.revision || record.sha256 != model.sha256) return null
        val file = destinationFile(model)
        return record.takeIf { file.isFile && file.length() == it.sizeBytes && it.sizeBytes > 0L }
    }

    @Synchronized
    fun registerInstalled(model: ModelArtifactSummary, file: File, sizeBytes: Long, sha256: String) {
        require(file.canonicalFile == destinationFile(model).canonicalFile) {
            "Only the model store's managed destination can be registered"
        }
        require(file.isFile && file.length() == sizeBytes && sizeBytes > 0L) {
            "Installed model size does not match the verified file"
        }
        require(sha256.equals(model.sha256, ignoreCase = true)) {
            "Installed model hash does not match the catalogue"
        }

        val records = readRegistry()
        records[model.id] = InstalledModelRecord(
            modelId = model.id,
            revision = model.revision,
            fileName = model.fileName,
            sizeBytes = sizeBytes,
            sha256 = sha256.lowercase(),
            installedAtEpochMillis = System.currentTimeMillis(),
        )
        writeRegistry(records)
    }

    @Synchronized
    fun promoteVerifiedArtifact(
        model: ModelArtifactSummary,
        partFile: File,
        sizeBytes: Long,
        sha256: String,
    ): File {
        require(partFile.canonicalFile == partialFile(model).canonicalFile) {
            "Only this model's staging file can be installed"
        }
        require(partFile.isFile && partFile.length() == sizeBytes && sizeBytes > 0L)
        require(sha256.equals(model.sha256, ignoreCase = true))

        val destination = destinationFile(model)
        val parent = destination.parentFile
            ?: throw IllegalStateException("Model destination has no parent directory")
        check(parent.mkdirs() || parent.isDirectory) {
            "Could not create the model revision directory"
        }

        // Staging and destination live under filesDir on the same filesystem.
        // If atomic promotion is unsupported, leave the verified staging file intact.
        java.nio.file.Files.move(
            partFile.toPath(),
            destination.toPath(),
            java.nio.file.StandardCopyOption.ATOMIC_MOVE,
            java.nio.file.StandardCopyOption.REPLACE_EXISTING,
        )
        registerInstalled(model, destination, sizeBytes, sha256)
        return destination
    }

    @Synchronized
    fun deleteInstalled(model: ModelArtifactSummary): Boolean {
        val records = readRegistry()
        val modelDirectory = safeChild(root, model.id)
        var removedAnything = false

        // A model ID may have more than one installed revision over its lifetime.
        if (modelDirectory.exists()) {
            deleteManagedPath(modelDirectory)
            removedAnything = true
        }

        // Remove resumable fragments for this model ID, including older catalogue revisions.
        staging.listFiles()
            ?.filter { file ->
                file.name.startsWith("${model.id}-") && file.name.endsWith(".part")
            }
            ?.forEach { partial ->
                deleteManagedPath(partial)
                removedAnything = true
            }

        if (records.remove(model.id) != null) {
            writeRegistry(records)
        }
        return removedAnything
    }

    private fun readRegistry(): MutableMap<String, InstalledModelRecord> {
        val file = try {
            registry.openRead()
        } catch (_: FileNotFoundException) {
            return linkedMapOf()
        }

        return file.bufferedReader(Charsets.UTF_8).use { reader ->
            val rootJson = JSONObject(reader.readText())
            val entries = rootJson.optJSONArray("models") ?: JSONArray()
            buildMap {
                for (index in 0 until entries.length()) {
                    val item = entries.getJSONObject(index)
                    val record = InstalledModelRecord(
                        modelId = item.getString("model_id"),
                        revision = item.getString("revision"),
                        fileName = item.getString("file_name"),
                        sizeBytes = item.getLong("size_bytes"),
                        sha256 = item.getString("sha256"),
                        installedAtEpochMillis = item.getLong("installed_at_epoch_ms"),
                    )
                    if (isSafeSegment(record.modelId) &&
                        isSafeSegment(record.revision) &&
                        isSafeFileName(record.fileName) &&
                        record.sizeBytes > 0L
                    ) {
                        put(record.modelId, record)
                    }
                }
            }.toMutableMap()
        }
    }

    private fun writeRegistry(records: Map<String, InstalledModelRecord>) {
        val rootJson = JSONObject()
        rootJson.put("schema_version", 1)
        val entries = JSONArray()
        records.values.forEach { record ->
            entries.put(
                JSONObject()
                    .put("model_id", record.modelId)
                    .put("revision", record.revision)
                    .put("file_name", record.fileName)
                    .put("size_bytes", record.sizeBytes)
                    .put("sha256", record.sha256)
                    .put("installed_at_epoch_ms", record.installedAtEpochMillis),
            )
        }
        rootJson.put("models", entries)

        val output = registry.startWrite()
        try {
            output.write(rootJson.toString().toByteArray(Charsets.UTF_8))
            output.fd.sync()
            registry.finishWrite(output)
        } catch (error: Exception) {
            registry.failWrite(output)
            throw error
        }
    }

    private fun validatePathSegments(model: ModelArtifactSummary) {
        require(isSafeSegment(model.id)) { "Invalid model ID for local storage" }
        require(isSafeSegment(model.revision)) { "Invalid model revision for local storage" }
        require(isSafeFileName(model.fileName)) { "Invalid model filename for local storage" }
    }

    private fun safeChild(parent: File, name: String): File {
        val child = File(parent, name)
        val canonicalParent = parent.canonicalFile
        val canonicalChild = child.canonicalFile
        require(canonicalChild.path.startsWith(canonicalParent.path + File.separator)) {
            "Model path escaped the app-private model directory"
        }
        return child
    }

    private fun deleteManagedPath(path: File) {
        val canonicalRoot = root.canonicalFile
        val canonicalPath = path.canonicalFile
        require(canonicalPath.path.startsWith(canonicalRoot.path + File.separator)) {
            "Refusing to delete a path outside the managed model store"
        }
        if (path.exists() && !path.deleteRecursively()) {
            throw IllegalStateException("Could not delete model data at ${path.name}")
        }
    }

    private fun isSafeSegment(value: String): Boolean =
        value.isNotBlank() &&
            value != "." &&
            value != ".." &&
            value.all { it.isLetterOrDigit() || it in "._-" }

    private fun isSafeFileName(value: String): Boolean =
        isSafeSegment(value) && '/' !in value && '\\' !in value
}
