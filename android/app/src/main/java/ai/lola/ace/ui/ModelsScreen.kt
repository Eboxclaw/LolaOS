package ai.lola.ace.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import ai.lola.ace.data.ModelArtifactSummary

@Composable
fun ModelsScreen(
    models: List<ModelArtifactSummary>,
    catalogueError: String?,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 20.dp, vertical = 18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            text = "Biblioteca de modelos",
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.SemiBold,
        )
        Text(
            text = "Artefactos conhecidos pela LOLA ACE. Um ficheiro só fica disponível para execução depois de descarregado, verificado, instalado e aceite por um backend.",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            style = MaterialTheme.typography.bodyMedium,
        )

        if (catalogueError != null) {
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer)) {
                Text(
                    text = "Erro ao abrir o catálogo: $catalogueError",
                    modifier = Modifier.padding(16.dp),
                    color = MaterialTheme.colorScheme.onErrorContainer,
                )
            }
        } else if (models.isEmpty()) {
            Text("Ainda não existem modelos no catálogo.")
        } else {
            LazyColumn(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                items(models, key = { it.id }) { model ->
                    ModelCard(model)
                }
            }
        }
    }
}

@Composable
private fun ModelCard(model: ModelArtifactSummary) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(9.dp),
        ) {
            Text(
                text = model.displayName,
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = "${model.publisher} · ${model.family} · ${model.format.uppercase()}",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
            Row {
                MetadataValue("Quantização", model.quantization ?: "—")
                Spacer(Modifier.width(24.dp))
                MetadataValue("Tamanho", model.sizeLabel)
            }
            Spacer(Modifier.height(2.dp))
            MetadataValue("Estado", "Só catálogo · não instalado")
            MetadataValue("Repositório", model.sourceRepository)
            MetadataValue("Revisão", model.revision.take(12) + "…")
            MetadataValue("SHA-256", model.sha256.take(16) + "…")
            MetadataValue("Contexto anunciado", model.advertisedContext)
            MetadataValue("Licença", model.license)
            Text(
                text = "A transferência e a instalação ainda não estão ligadas nesta versão.",
                color = MaterialTheme.colorScheme.primary,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
private fun MetadataValue(label: String, value: String) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(
            text = label.uppercase(),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            style = MaterialTheme.typography.labelSmall,
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
        )
    }
}
