package ai.lola.ace.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import ai.lola.ace.data.ModelArtifactSummary
import ai.lola.ace.data.ModelTransferManager
import ai.lola.ace.data.ModelTransferSnapshot
import ai.lola.ace.data.ModelTransferStage

@Composable
fun ModelsScreen(
    models: List<ModelArtifactSummary>,
    catalogueError: String?,
    transferManager: ModelTransferManager,
) {
    val snapshots = remember(transferManager) { mutableStateMapOf<String, ModelTransferSnapshot>() }

    DisposableEffect(transferManager, models) {
        models.forEach { model -> snapshots[model.id] = transferManager.snapshot(model) }
        val subscription = transferManager.subscribe { snapshot ->
            snapshots[snapshot.modelId] = snapshot
        }
        onDispose { subscription.close() }
    }

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
            text = "Transfere e gere modelos locais. A instalação só termina depois da verificação SHA-256.",
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
                    ModelCard(
                        model = model,
                        snapshot = snapshots[model.id] ?: transferManager.snapshot(model),
                        onDownload = { transferManager.download(model) },
                        onCancel = { transferManager.cancel(model.id) },
                        onDelete = { transferManager.delete(model) },
                    )
                }
            }
        }
    }
}

@Composable
private fun ModelCard(
    model: ModelArtifactSummary,
    snapshot: ModelTransferSnapshot,
    onDownload: () -> Unit,
    onCancel: () -> Unit,
    onDelete: () -> Unit,
) {
    val status = when (snapshot.stage) {
        ModelTransferStage.AVAILABLE -> "Disponível para transferência"
        ModelTransferStage.DOWNLOADING -> "A transferir"
        ModelTransferStage.VERIFYING -> "A verificar integridade"
        ModelTransferStage.INSTALLED -> "Instalado e verificado"
        ModelTransferStage.PAUSED -> "Transferência interrompida"
        ModelTransferStage.CANCELLED -> "Transferência cancelada"
        ModelTransferStage.DELETING -> "A remover"
        ModelTransferStage.FAILED -> "Falha"
    }

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
            MetadataValue("Estado", status)

            if (snapshot.stage == ModelTransferStage.DOWNLOADING ||
                snapshot.stage == ModelTransferStage.VERIFYING
            ) {
                val progress = snapshot.progress
                if (progress != null) {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(6.dp)
                            .clip(RoundedCornerShape(50))
                            .background(MaterialTheme.colorScheme.surface),
                    ) {
                        Box(
                            modifier = Modifier
                                .fillMaxHeight()
                                .fillMaxWidth(progress)
                                .background(MaterialTheme.colorScheme.primary),
                        )
                    }
                    Text(
                        text = "${formatBytes(snapshot.bytesDownloaded)} / ${formatBytes(snapshot.totalBytes ?: 0L)}",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodySmall,
                    )
                } else {
                    Text(
                        text = "${formatBytes(snapshot.bytesDownloaded)} transferidos",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }

            snapshot.message?.let { message ->
                Text(
                    text = message,
                    color = if (snapshot.stage == ModelTransferStage.FAILED) {
                        MaterialTheme.colorScheme.error
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
                    style = MaterialTheme.typography.bodySmall,
                )
            }

            MetadataValue("Repositório", model.sourceRepository)
            MetadataValue("Revisão", model.revision.take(12) + "…")
            MetadataValue("SHA-256", model.sha256.take(16) + "…")
            MetadataValue("Contexto anunciado", model.advertisedContext)
            MetadataValue("Licença", model.license)

            when (snapshot.stage) {
                ModelTransferStage.DOWNLOADING -> Button(
                    onClick = onCancel,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Cancelar transferência") }

                ModelTransferStage.VERIFYING,
                ModelTransferStage.DELETING -> Button(
                    onClick = {},
                    enabled = false,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(if (snapshot.stage == ModelTransferStage.VERIFYING) "A verificar…" else "A remover…") }

                ModelTransferStage.INSTALLED -> Button(
                    onClick = onDelete,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Apagar modelo") }

                else -> Button(
                    onClick = onDownload,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(
                        when (snapshot.stage) {
                            ModelTransferStage.PAUSED -> "Retomar transferência"
                            ModelTransferStage.CANCELLED -> "Retomar transferência"
                            ModelTransferStage.FAILED -> "Tentar novamente"
                            else -> "Descarregar modelo"
                        },
                    )
                }
            }
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

private fun formatBytes(bytes: Long): String {
    if (bytes < 1024L) return "$bytes B"
    val kilobytes = bytes / 1024.0
    if (kilobytes < 1024.0) return "%.1f KiB".format(kilobytes)
    val megabytes = kilobytes / 1024.0
    if (megabytes < 1024.0) return "%.1f MiB".format(megabytes)
    return "%.2f GiB".format(megabytes / 1024.0)
}
