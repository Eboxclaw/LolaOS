package ai.lola.ace.data

internal data class ContentRange(
    val start: Long,
    val endInclusive: Long,
    val totalBytes: Long?,
)

internal object ModelDownloadPolicy {
    private val revisionPattern = Regex("^[a-f0-9]{40}$")
    private val sha256Pattern = Regex("^[a-f0-9]{64}$")
    private val modelIdPattern = Regex("^[a-z0-9][a-z0-9._-]*$")
    private val repositoryPattern = Regex("^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$")
    private val fileNamePattern = Regex("^[A-Za-z0-9._-]+\\.(gguf|onnx)$")

    fun validateArtifact(model: ModelArtifactSummary) {
        require(modelIdPattern.matches(model.id)) { "Invalid model ID" }
        require(repositoryPattern.matches(model.sourceRepository)) { "Invalid model repository" }
        require(revisionPattern.matches(model.revision)) { "Model revision must be an immutable commit SHA" }
        require(fileNamePattern.matches(model.fileName)) { "Invalid model artefact filename" }
        require(sha256Pattern.matches(model.sha256.lowercase())) { "Invalid SHA-256 in model catalogue" }

        val expectedUrl =
            "https://huggingface.co/${model.sourceRepository}/resolve/${model.revision}/${model.fileName}"
        require(model.downloadUrl == expectedUrl) {
            "Download URL does not match the pinned publisher, revision and filename"
        }
        require(model.format.equals("gguf", ignoreCase = true) && model.fileName.endsWith(".gguf")) {
            "This first Android transfer implementation only accepts GGUF artefacts"
        }
    }

    fun parseContentRange(value: String?): ContentRange? {
        if (value == null) return null
        val match = Regex("^bytes (\\d+)-(\\d+)/(\\d+|\\*)$", RegexOption.IGNORE_CASE)
            .matchEntire(value.trim()) ?: return null
        val start = match.groupValues[1].toLongOrNull() ?: return null
        val end = match.groupValues[2].toLongOrNull() ?: return null
        val totalText = match.groupValues[3]
        val total = if (totalText == "*") null else totalText.toLongOrNull() ?: return null
        if (end < start || (total != null && (total <= end || total <= 0L))) return null
        return ContentRange(start, end, total)
    }

    fun parseUnsatisfiedRangeTotal(value: String?): Long? {
        if (value == null) return null
        val match = Regex("^bytes \\*/(\\d+)$", RegexOption.IGNORE_CASE)
            .matchEntire(value.trim()) ?: return null
        return match.groupValues[1].toLongOrNull()
    }
}
