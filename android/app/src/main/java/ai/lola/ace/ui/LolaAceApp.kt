package ai.lola.ace.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import ai.lola.ace.R
import ai.lola.ace.data.ModelArtifactSummary
import ai.lola.ace.data.ModelCatalogRepository
import ai.lola.ace.data.ModelTransferManager
import ai.lola.ace.data.ModelTransferManagerProvider
import ai.lola.ace.inference.LocalInferenceBackendProvider

private enum class AceDestination(val labelRes: Int, val iconRes: Int) {
    CHAT(R.string.nav_chats, R.drawable.ic_chat),
    MODELS(R.string.nav_library, R.drawable.ic_library),
    DIAGNOSTICS(R.string.nav_status, R.drawable.ic_status),
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LolaAceApp() {
    val context = LocalContext.current
    val transferManager = remember(context) { ModelTransferManagerProvider.get(context) }
    val inferenceBackend = remember { LocalInferenceBackendProvider.get() }
    val inferenceState by inferenceBackend.state.collectAsState()
    val latestMetrics by inferenceBackend.latestMetrics.collectAsState()
    val catalogResult = remember(context) {
        runCatching { ModelCatalogRepository(context).loadBundled() }
    }
    var destination by rememberSaveable { mutableStateOf(AceDestination.CHAT.name) }
    val selectedDestination = AceDestination.valueOf(destination)
    var chatHomeOpen by rememberSaveable { mutableStateOf(false) }
    var pendingOpenConversationId by rememberSaveable { mutableStateOf<Long?>(null) }
    var requestNewConversation by rememberSaveable { mutableStateOf(false) }
    var activeConversationId by rememberSaveable { mutableStateOf<Long?>(null) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(stringResource(R.string.app_name))
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
                        icon = {
                            Icon(
                                painter = painterResource(item.iconRes),
                                contentDescription = null,
                            )
                        },
                        label = { Text(stringResource(item.labelRes)) },
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
                AceDestination.CHAT -> if (chatHomeOpen) {
                    ConversationsHomeScreen(
                        currentConversationId = activeConversationId,
                        onOpenConversation = { id ->
                            pendingOpenConversationId = id
                            chatHomeOpen = false
                        },
                        onNewConversation = {
                            requestNewConversation = true
                            chatHomeOpen = false
                        },
                    )
                } else {
                    ChatScreen(
                        model = catalogResult.getOrNull()?.firstOrNull(),
                        catalogueAvailable = catalogResult.isSuccess,
                        transferManager = transferManager,
                        backend = inferenceBackend,
                        onOpenModels = { destination = AceDestination.MODELS.name },
                        onOpenHome = { chatHomeOpen = true },
                        pendingOpenConversationId = pendingOpenConversationId,
                        onPendingOpenConsumed = { pendingOpenConversationId = null },
                        requestNewConversation = requestNewConversation,
                        onNewConversationRequestConsumed = { requestNewConversation = false },
                        onActiveConversationChange = { activeConversationId = it },
                    )
                }
                AceDestination.MODELS -> ModelsScreen(
                    models = catalogResult.getOrNull().orEmpty(),
                    catalogueError = catalogResult.exceptionOrNull()?.message,
                    transferManager = transferManager,
                )
                AceDestination.DIAGNOSTICS -> DiagnosticsScreen(
                    modelCount = catalogResult.getOrNull()?.size ?: 0,
                    catalogueAvailable = catalogResult.isSuccess,
                    inferenceState = inferenceState,
                    latestMetrics = latestMetrics,
                )
            }
        }
    }
}

internal fun displayModelLabel(model: ModelArtifactSummary?): String =
    model?.displayName ?: "Nenhum modelo catalogado"
