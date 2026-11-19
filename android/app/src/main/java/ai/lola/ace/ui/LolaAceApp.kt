package ai.lola.ace.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.sp
import ai.lola.ace.data.ModelArtifactSummary
import ai.lola.ace.data.ModelCatalogRepository
import ai.lola.ace.data.ModelTransferManager
import ai.lola.ace.data.ModelTransferManagerProvider

private enum class AceDestination(val label: String, val symbol: String) {
    CHAT("Conversas", "◉"),
    MODELS("Modelos", "▤"),
    DIAGNOSTICS("Diagnóstico", "⌁"),
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LolaAceApp() {
    val context = LocalContext.current
    val transferManager = remember(context) { ModelTransferManagerProvider.get(context) }
    val catalogResult = remember(context) {
        runCatching { ModelCatalogRepository(context).loadBundled() }
    }
    var destination by rememberSaveable { mutableStateOf(AceDestination.CHAT.name) }
    val selectedDestination = AceDestination.valueOf(destination)

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text("LOLA ACE", letterSpacing = 2.sp)
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = androidx.compose.material3.MaterialTheme.colorScheme.background,
                ),
            )
        },
        bottomBar = {
            NavigationBar {
                AceDestination.entries.forEach { item ->
                    NavigationBarItem(
                        selected = selectedDestination == item,
                        onClick = { destination = item.name },
                        icon = { Text(item.symbol) },
                        label = { Text(item.label) },
                    )
                }
            }
        },
    ) { innerPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding),
        ) {
            when (selectedDestination) {
                AceDestination.CHAT -> ChatScreen(
                    model = catalogResult.getOrNull()?.firstOrNull(),
                    catalogueAvailable = catalogResult.isSuccess,
                    transferManager = transferManager,
                    onOpenModels = { destination = AceDestination.MODELS.name },
                )
                AceDestination.MODELS -> ModelsScreen(
                    models = catalogResult.getOrNull().orEmpty(),
                    catalogueError = catalogResult.exceptionOrNull()?.message,
                    transferManager = transferManager,
                )
                AceDestination.DIAGNOSTICS -> DiagnosticsScreen(
                    modelCount = catalogResult.getOrNull()?.size ?: 0,
                    catalogueAvailable = catalogResult.isSuccess,
                )
            }
        }
    }
}

internal fun displayModelLabel(model: ModelArtifactSummary?): String =
    model?.displayName ?: "Nenhum modelo catalogado"
