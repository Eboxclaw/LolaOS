package ai.lola.ace.data

import android.content.Context
import org.json.JSONObject

data class ModelArtifactSummary(
    val id: String,
    val displayName: String,
    val family: String,
    val publisher: String,
    val sourceRepository: String,
    val revision: String,
    val fileName: String,
    val format: String,
    val quantization: String?,
    val downloadUrl: String,
    val sha256: String,
    val sizeLabel: String,
    val advertisedContext: String,
    val architecture: String,
    val license: String,
)

class ModelCatalogRepository(private val context: Context) {
    fun loadBundled(): List<ModelArtifactSummary> {
        val json = context.assets.open("catalog.v1.json")
            .bufferedReader()
            .use { it.readText() }
        val root = JSONObject(json)
        require(root.getInt("schema_version") == 1) {
            "Unsupported model catalogue schema"
        }

        val models = root.getJSONArray("models")
        return buildList(models.length()) {
            for (index in 0 until models.length()) {
                val model = models.getJSONObject(index)
                add(
                    ModelArtifactSummary(
                        id = model.getString("id"),
                        displayName = model.getString("display_name"),
                        family = model.getString("family"),
                        publisher = model.getString("publisher"),
                        sourceRepository = model.getString("source_repo"),
                        revision = model.getString("revision"),
                        fileName = model.getString("file_name"),
                        format = model.getString("format"),
                        quantization = model.optString("quantization").takeIf(String::isNotBlank),
                        downloadUrl = model.getString("download_url"),
                        sha256 = model.getString("sha256"),
                        sizeLabel = model.getString("size_label"),
                        advertisedContext = model.getString("advertised_context"),
                        architecture = model.getString("architecture"),
                        license = model.getString("license"),
                    ),
                )
            }
        }
    }
}
