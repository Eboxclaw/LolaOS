package ai.lola.ace.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import ai.lola.ace.inference.InferenceRunMetrics
import ai.lola.ace.inference.LocalInferenceState

@Composable
fun DiagnosticsScreen(
    modelCount: Int,
    catalogueAvailable: Boolean,
    inferenceState: LocalInferenceState,
    latestMetrics: InferenceRunMetrics?,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp, vertical = 18.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Text(
            text = "Diagnóstico",
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.SemiBold,
        )
        Text(
            text = "Estado real da implementação. Os valores não medidos são apresentados como indisponíveis.",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            style = MaterialTheme.typography.bodyMedium,
        )

        DiagnosticCard("CATÁLOGO", if (catalogueAvailable) "Carregado" else "Erro ao carregar")
        DiagnosticCard("MODELOS LISTADOS", modelCount.toString())
        DiagnosticCard(
            "MODELO RESIDENTE",
            (inferenceState as? LocalInferenceState.Loaded)?.modelFileName ?: "Nenhum",
        )
        DiagnosticCard(
            "BACKEND",
            when (inferenceState) {
                LocalInferenceState.Unloaded -> "Descarregado"
                LocalInferenceState.Loading -> "A carregar"
                is LocalInferenceState.Loaded -> "llama.cpp · sessão CPU local"
                is LocalInferenceState.Error -> "Erro: ${inferenceState.message}"
            },
        )
        DiagnosticCard("ABI DO APK", "arm64-v8a")
        DiagnosticCard("DISPOSITIVO", "Não confirmado por teste físico")
        DiagnosticCard(
            "TOKENS DE PROMPT / SAÍDA",
            latestMetrics?.let { "${it.promptTokens} / ${it.generatedTokens}" } ?: "Ainda sem geração",
        )
        DiagnosticCard(
            "TTFT",
            latestMetrics?.let { formatMillis(it.timeToFirstTokenMillis) } ?: "Ainda sem geração",
        )
        DiagnosticCard(
            "PREFILL",
            latestMetrics?.let { formatMillis(it.prefillMillis) } ?: "Ainda sem geração",
        )
        DiagnosticCard(
            "DECODE",
            latestMetrics?.decodeTokensPerSecond?.let { String.format("%.2f tok/s", it) }
                ?: "Ainda sem geração",
        )
        DiagnosticCard(
            "TEMPO TOTAL",
            latestMetrics?.let { formatMillis(it.totalMillis) } ?: "Ainda sem geração",
        )
        DiagnosticCard("PSS / TEMPERATURA", "Sem medições físicas")

        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Text(
                text = "O TTFT, prefill, decode e tempo total acima são medidos pelo relógio monotónico nativo durante cada geração. PSS, temperatura, bateria e frequência CPU exigem instrumentação física no Pixel.",
                modifier = Modifier.padding(16.dp),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
private fun DiagnosticCard(label: String, value: String) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Column(
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 13.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                text = label,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.labelSmall,
            )
            Text(
                text = value,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Medium,
            )
        }
    }
}

private fun formatMillis(value: Long): String = "${value} ms"
