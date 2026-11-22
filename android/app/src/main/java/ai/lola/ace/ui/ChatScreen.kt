package ai.lola.ace.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import ai.lola.ace.R
import ai.lola.ace.data.ChatRepository
import ai.lola.ace.data.ChatRepositoryProvider
import ai.lola.ace.data.ModelArtifactSummary
import ai.lola.ace.data.ModelTransferManager
import ai.lola.ace.data.StoredConversation
import ai.lola.ace.inference.InferenceChatMessage
import ai.lola.ace.inference.LocalInferenceBackend
import ai.lola.ace.inference.LocalInferenceState
import ai.lola.ace.inference.ReferenceSessionConfig
import ai.lola.ace.ui.theme.LolaSpacing
import ai.lola.ace.ui.theme.LolaStatus
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val CHAT_SESSION_ID = "primary-chat"
private const val MAX_OUTPUT_TOKENS = 256

private data class ChatLine(
    val role: String,
    val text: String,
)

@Composable
fun ChatScreen(
    model: ModelArtifactSummary?,
    catalogueAvailable: Boolean,
    transferManager: ModelTransferManager,
    backend: LocalInferenceBackend,
    onOpenModels: () -> Unit,
) {
    val context = LocalContext.current
    val repository = remember(context) { ChatRepositoryProvider.get(context) }
    val runtimeState by backend.state.collectAsState()
    val latestMetrics by backend.latestMetrics.collectAsState()
    val scope = rememberCoroutineScope()
    val chatLines = remember { mutableStateListOf<ChatLine>() }
    val listState = rememberLazyListState()

    var transferVersion by remember { mutableIntStateOf(0) }
    var input by rememberSaveable { mutableStateOf("") }
    var sessionReady by remember { mutableStateOf(false) }
    var generating by remember { mutableStateOf(false) }
    var localError by remember { mutableStateOf<String?>(null) }
    var conversations by remember { mutableStateOf<List<StoredConversation>>(emptyList()) }
    var currentConversationId by rememberSaveable { mutableStateOf<Long?>(null) }
    var showHistory by remember { mutableStateOf(false) }
    var showDeleteConfirmation by remember { mutableStateOf(false) }
    var historyLoading by remember { mutableStateOf(true) }
    var conversationLoading by remember { mutableStateOf(false) }

    DisposableEffect(transferManager) {
        val subscription = transferManager.subscribe { transferVersion++ }
        onDispose { subscription.close() }
    }

    val installedFile = remember(model, transferVersion) {
        model?.let(transferManager::installedModelFile)
    }

    suspend fun refreshConversations() {
        conversations = withContext(Dispatchers.IO) {
            repository.listConversations()
        }
    }

    LaunchedEffect(repository) {
        historyLoading = true
        try {
            val initialConversationId = withContext(Dispatchers.IO) {
                repository.latestConversationId() ?: repository.createConversation()
            }
            refreshConversations()
            currentConversationId = initialConversationId
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            localError = error.message ?: "Não foi possível abrir o histórico local."
        } finally {
            historyLoading = false
        }
    }

    LaunchedEffect(currentConversationId) {
        val conversationId = currentConversationId ?: return@LaunchedEffect
        conversationLoading = true
        try {
            val messages = withContext(Dispatchers.IO) {
                repository.loadMessages(conversationId)
            }
            chatLines.clear()
            chatLines.addAll(
                messages
                    .filter { it.role == "user" || it.role == "assistant" }
                    .map { ChatLine(role = it.role, text = it.content) },
            )
            localError = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            localError = error.message ?: "Não foi possível carregar esta conversa."
        } finally {
            conversationLoading = false
        }
    }

    suspend fun ensureChatSession() {
        val selectedModel = model ?: return
        try {
            backend.openSession(
                CHAT_SESSION_ID,
                ReferenceSessionConfig(
                    contextTokens = 2048,
                    threads = 4,
                    temperature = 0.1f,
                    topK = 50,
                    repeatPenalty = 1.1f,
                ),
            )
            sessionReady = true
            transferManager.setModelInUse(selectedModel.id, true)
            localError = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            sessionReady = false
            localError = error.message ?: "Não foi possível abrir a sessão local."
        }
    }

    LaunchedEffect(runtimeState, model?.id) {
        if (runtimeState is LocalInferenceState.Loaded && model != null) {
            ensureChatSession()
        } else {
            sessionReady = false
            if (runtimeState is LocalInferenceState.Unloaded ||
                runtimeState is LocalInferenceState.Error
            ) {
                model?.let { transferManager.setModelInUse(it.id, false) }
            }
        }
    }

    LaunchedEffect(chatLines.size) {
        if (chatLines.isNotEmpty()) {
            listState.animateScrollToItem(chatLines.lastIndex)
        }
    }

    fun createNewConversation() {
        if (generating) return
        conversationLoading = true
        scope.launch {
            try {
                val newId = withContext(Dispatchers.IO) {
                    repository.createConversation()
                }
                currentConversationId = newId
                refreshConversations()
                showHistory = false
                localError = null
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                localError = error.message ?: "Não foi possível criar uma conversa."
                conversationLoading = false
            }
        }
    }

    fun selectConversation(conversationId: Long) {
        if (generating || currentConversationId == conversationId) {
            showHistory = false
            return
        }
        conversationLoading = true
        currentConversationId = conversationId
        showHistory = false
        localError = null
    }

    fun deleteCurrentConversation() {
        val deletingId = currentConversationId ?: return
        if (generating || conversationLoading) return

        showDeleteConfirmation = false
        conversationLoading = true
        scope.launch {
            try {
                val nextConversationId = withContext(Dispatchers.IO) {
                    repository.deleteConversation(deletingId)
                    repository.latestConversationId() ?: repository.createConversation()
                }
                currentConversationId = nextConversationId
                refreshConversations()
                localError = null
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                conversationLoading = false
                localError = error.message ?: "Não foi possível apagar esta conversa."
            }
        }
    }

    fun loadModel() {
        val file = installedFile ?: return
        scope.launch {
            localError = null
            // Protect the app-private GGUF from deletion while llama.cpp may mmap/read it.
            model?.let { transferManager.setModelInUse(it.id, true) }
            try {
                backend.loadModel(file)
            } catch (error: CancellationException) {
                throw error
            } catch (error: LinkageError) {
                model?.let { transferManager.setModelInUse(it.id, false) }
                localError = error.message ?: "A biblioteca nativa não foi carregada."
            } catch (error: Exception) {
                model?.let { transferManager.setModelInUse(it.id, false) }
                localError = error.message ?: "Falha ao carregar o modelo."
            }
        }
    }

    fun unloadModel() {
        scope.launch {
            try {
                backend.unloadModel()
                model?.let { transferManager.setModelInUse(it.id, false) }
                sessionReady = false
                generating = false
                // Keep the local transcript visible when the model is unloaded.
                localError = null
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                localError = error.message ?: "Não foi possível libertar o modelo."
            }
        }
    }

    fun sendMessage() {
        val text = input.trim()
        val conversationId = currentConversationId
        if (text.isEmpty() || !sessionReady || generating || conversationLoading || conversationId == null) return

        val history = buildList {
            add(
                InferenceChatMessage(
                    role = "system",
                    content = "És a LOLA, uma assistente local, útil e rigorosa. Responde em português europeu quando o utilizador escreve em português.",
                ),
            )
            chatLines.filter { it.text.isNotBlank() }.forEach { line ->
                add(InferenceChatMessage(line.role, line.text))
            }
            add(InferenceChatMessage("user", text))
        }

        input = ""
        chatLines.add(ChatLine(role = "user", text = text))
        chatLines.add(ChatLine(role = "assistant", text = ""))
        generating = true
        localError = null

        scope.launch {
            val response = StringBuilder()
            var assistantPersisted = false
            try {
                withContext(NonCancellable + Dispatchers.IO) {
                    repository.appendMessage(conversationId, "user", text)
                }
                refreshConversations()

                backend.generate(
                    sessionId = CHAT_SESSION_ID,
                    messages = history,
                    maxOutputTokens = MAX_OUTPUT_TOKENS,
                ).collect { delta ->
                    response.append(delta)
                    val last = chatLines.lastIndex
                    if (last >= 0 && chatLines[last].role == "assistant") {
                        chatLines[last] = chatLines[last].copy(text = response.toString())
                    }
                }

                if (response.isNotBlank()) {
                    withContext(NonCancellable + Dispatchers.IO) {
                        repository.appendMessage(conversationId, "assistant", response.toString())
                    }
                    assistantPersisted = true
                }
            } catch (error: CancellationException) {
                // Persist any partial response in finally, then preserve structured cancellation.
                throw error
            } catch (error: Exception) {
                localError = error.message ?: "Erro durante a geração local."
            } finally {
                if (!assistantPersisted && response.isNotBlank()) {
                    runCatching {
                        withContext(NonCancellable + Dispatchers.IO) {
                            repository.appendMessage(conversationId, "assistant", response.toString())
                        }
                    }.onFailure { error ->
                        localError = error.message ?: "Não foi possível guardar a resposta parcial."
                    }
                } else if (response.isBlank()) {
                    // Do not leave a permanent typing placeholder when the backend returns no text.
                    val last = chatLines.lastOrNull()
                    if (last?.role == "assistant" && last.text.isBlank()) {
                        chatLines.removeAt(chatLines.lastIndex)
                    }
                }
                runCatching { refreshConversations() }
                generating = false
            }
        }
    }

    val currentConversationTitle = conversations
        .firstOrNull { it.id == currentConversationId }
        ?.title
        ?: ChatRepository.DEFAULT_CONVERSATION_TITLE

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            TextButton(
                onClick = ::createNewConversation,
                enabled = !generating && !historyLoading && !conversationLoading,
            ) { Text(stringResource(R.string.chat_new)) }
            TextButton(
                onClick = { showDeleteConfirmation = true },
                enabled = currentConversationId != null &&
                    !generating && !historyLoading && !conversationLoading,
            ) { Text(stringResource(R.string.action_delete)) }
            Spacer(modifier = Modifier.weight(1f))
            Column {
                TextButton(
                    onClick = { showHistory = true },
                    enabled = conversations.isNotEmpty() && !historyLoading && !generating && !conversationLoading,
                ) { Text(stringResource(R.string.chat_history, conversations.size)) }
                DropdownMenu(
                    expanded = showHistory,
                    onDismissRequest = { showHistory = false },
                ) {
                    conversations.forEach { conversation ->
                        DropdownMenuItem(
                            text = {
                                Text(
                                    text = conversation.title,
                                    maxLines = 1,
                                )
                            },
                            onClick = { selectConversation(conversation.id) },
                        )
                    }
                }
            }
        }

        if (showDeleteConfirmation) {
            AlertDialog(
                onDismissRequest = { showDeleteConfirmation = false },
                title = { Text(stringResource(R.string.chat_delete_title)) },
                text = {
                    Text(stringResource(R.string.chat_delete_body))
                },
                confirmButton = {
                    TextButton(onClick = ::deleteCurrentConversation) {
                        Text(stringResource(R.string.action_delete))
                    }
                },
                dismissButton = {
                    TextButton(onClick = { showDeleteConfirmation = false }) {
                        Text(stringResource(R.string.action_cancel))
                    }
                },
            )
        }

        Column(verticalArrangement = Arrangement.spacedBy(LolaSpacing.xs)) {
            Text(
                text = stringResource(R.string.chat_overline),
                color = MaterialTheme.colorScheme.primary,
                style = MaterialTheme.typography.labelSmall,
            )
            Text(
                text = currentConversationTitle,
                style = MaterialTheme.typography.titleLarge,
            )
            Text(
                text = model?.displayName ?: stringResource(R.string.chat_no_model),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Row(
                horizontalArrangement = Arrangement.spacedBy(LolaSpacing.sm),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val (dotColor, statusText) = when (runtimeState) {
                    LocalInferenceState.Unloaded -> if (installedFile == null) {
                        LolaStatus.Info to stringResource(R.string.chat_status_download_needed)
                    } else {
                        LolaStatus.Warning to stringResource(R.string.chat_status_installed)
                    }
                    LocalInferenceState.Loading -> LolaStatus.Warning to
                        stringResource(R.string.chat_status_loading)
                    is LocalInferenceState.Loaded -> if (sessionReady) {
                        LolaStatus.Success to stringResource(R.string.chat_status_active)
                    } else {
                        LolaStatus.Warning to stringResource(R.string.chat_status_session)
                    }
                    is LocalInferenceState.Error -> MaterialTheme.colorScheme.error to
                        stringResource(R.string.chat_status_failed)
                }
                Box(
                    modifier = Modifier
                        .size(8.dp)
                        .background(dotColor, CircleShape),
                )
                Text(
                    text = statusText,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }

        when (runtimeState) {
            LocalInferenceState.Unloaded -> {
                if (!catalogueAvailable || model == null) {
                    Text(stringResource(R.string.chat_no_model))
                } else if (installedFile == null) {
                    Card(
                        modifier = Modifier.fillMaxWidth(),
                        shape = MaterialTheme.shapes.medium,
                        colors = CardDefaults.cardColors(
                            containerColor = MaterialTheme.colorScheme.surfaceVariant,
                        ),
                    ) {
                        Column(
                            modifier = Modifier.padding(LolaSpacing.lg),
                            verticalArrangement = Arrangement.spacedBy(LolaSpacing.sm),
                        ) {
                            Text(stringResource(R.string.chat_not_installed_title))
                            Text(
                                stringResource(R.string.chat_not_installed_body),
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                style = MaterialTheme.typography.bodySmall,
                            )
                            Button(onClick = onOpenModels) {
                                Text(stringResource(R.string.chat_open_library))
                            }
                        }
                    }
                } else {
                    Button(
                        onClick = ::loadModel,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(stringResource(R.string.chat_load_model))
                    }
                }
            }
            LocalInferenceState.Loading -> {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(LolaSpacing.md),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(24.dp),
                        strokeWidth = 2.5.dp,
                    )
                    Text(stringResource(R.string.chat_loading_backend))
                }
            }
            is LocalInferenceState.Loaded -> {
                if (!sessionReady) {
                    Text(
                        text = localError ?: stringResource(R.string.chat_opening_session),
                        color = if (localError != null) {
                            MaterialTheme.colorScheme.error
                        } else {
                            MaterialTheme.colorScheme.onSurfaceVariant
                        },
                    )
                    if (localError != null) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(LolaSpacing.sm),
                        ) {
                            Button(
                                onClick = { scope.launch { ensureChatSession() } },
                            ) { Text(stringResource(R.string.chat_retry_session)) }
                            TextButton(onClick = ::unloadModel) {
                                Text(stringResource(R.string.chat_free_memory))
                            }
                        }
                    }
                }
            }
            is LocalInferenceState.Error -> {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = MaterialTheme.shapes.medium,
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.errorContainer,
                    ),
                ) {
                    Column(
                        modifier = Modifier.padding(LolaSpacing.lg),
                        verticalArrangement = Arrangement.spacedBy(LolaSpacing.sm),
                    ) {
                        Text(
                            text = (runtimeState as LocalInferenceState.Error).message,
                            color = MaterialTheme.colorScheme.onErrorContainer,
                        )
                        TextButton(onClick = ::loadModel) {
                            Text(stringResource(R.string.chat_retry_load))
                        }
                    }
                }
            }
        }

        localError?.let { error ->
            Text(
                text = error,
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodySmall,
            )
        }

        LazyColumn(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            state = listState,
            contentPadding = PaddingValues(vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            itemsIndexed(chatLines) { index, line ->
                ChatMessageBubble(
                    line = line,
                    isStreaming = generating && index == chatLines.lastIndex && line.role == "assistant",
                )
            }
            if (chatLines.isEmpty()) {
                item {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(top = LolaSpacing.xxl),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(LolaSpacing.sm),
                    ) {
                        Text(
                            text = stringResource(R.string.chat_privacy_note),
                            style = MaterialTheme.typography.titleMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            text = if (runtimeState is LocalInferenceState.Loaded) {
                                stringResource(R.string.chat_empty_ready)
                            } else {
                                stringResource(R.string.chat_empty_idle)
                            },
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            style = MaterialTheme.typography.bodyMedium,
                            textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                        )
                    }
                }
            }
        }

        if (runtimeState is LocalInferenceState.Loaded) {
            val sendLabel = stringResource(R.string.action_send)
            val stopLabel = stringResource(R.string.action_stop)
            latestMetrics?.let { metrics ->
                Text(
                    text = formatMetricsFootnote(metrics),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.labelSmall,
                )
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(LolaSpacing.sm),
                verticalAlignment = Alignment.Bottom,
            ) {
                OutlinedTextField(
                    value = input,
                    onValueChange = { input = it },
                    modifier = Modifier.weight(1f),
                    enabled = sessionReady && !generating && !conversationLoading && currentConversationId != null,
                    placeholder = { Text(stringResource(R.string.chat_input_hint)) },
                    maxLines = 4,
                    shape = RoundedCornerShape(24.dp),
                )
                if (generating) {
                    IconButton(
                        onClick = { backend.cancelGeneration(CHAT_SESSION_ID) },
                        modifier = Modifier
                            .size(48.dp)
                            .semantics { contentDescription = stopLabel },
                    ) {
                        Icon(
                            painter = painterResource(R.drawable.ic_stop),
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.error,
                        )
                    }
                } else {
                    IconButton(
                        onClick = ::sendMessage,
                        enabled = sessionReady && !generating && !conversationLoading &&
                            input.isNotBlank() && currentConversationId != null,
                        modifier = Modifier
                            .size(48.dp)
                            .semantics { contentDescription = sendLabel },
                    ) {
                        Icon(
                            painter = painterResource(R.drawable.ic_send),
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
            }
            Text(
                text = stringResource(R.string.chat_engine_footnote),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.labelSmall,
            )
        } else {
            Text(
                text = stringResource(R.string.chat_history_footnote),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

private fun formatMillisValue(millis: Long): String =
    if (millis >= 1_000) {
        String.format(java.util.Locale.ROOT, "%.1fs", millis / 1000.0)
    } else {
        "$millis ms"
    }

private fun formatMetricsFootnote(metrics: ai.lola.ace.inference.InferenceRunMetrics): String {
    val tokens = "${metrics.promptTokens} → ${metrics.generatedTokens} tok"
    val ttft = "TTFT ${formatMillisValue(metrics.timeToFirstTokenMillis)}"
    val decode = metrics.decodeTokensPerSecond
        ?.let { String.format(java.util.Locale.ROOT, "%.2f tok/s", it) }
        ?: "—"
    return "$tokens · $ttft · $decode"
}

@Composable
private fun ChatMessageBubble(line: ChatLine, isStreaming: Boolean) {
    val isUser = line.role == "user"
    val visible = remember { MutableTransitionState(false) }
    LaunchedEffect(Unit) { visible.targetState = true }

    Box(
        modifier = Modifier.fillMaxWidth(),
        contentAlignment = if (isUser) Alignment.CenterEnd else Alignment.CenterStart,
    ) {
        androidx.compose.animation.AnimatedVisibility(
            visibleState = visible,
            enter = fadeIn() + slideInVertically(
                initialOffsetY = { it / 6 },
                animationSpec = tween(180, easing = FastOutSlowInEasing),
            ),
        ) {
            Column(
                modifier = Modifier.widthIn(max = 312.dp),
                verticalArrangement = Arrangement.spacedBy(LolaSpacing.xs),
                horizontalAlignment = if (isUser) Alignment.End else Alignment.Start,
            ) {
                if (!isUser) {
                    Text(
                        text = stringResource(R.string.chat_role_lola),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.labelSmall,
                    )
                }
                Box(
                    modifier = Modifier
                        .background(
                            color = if (isUser) {
                                MaterialTheme.colorScheme.primaryContainer
                            } else {
                                MaterialTheme.colorScheme.surfaceVariant
                            },
                            shape = if (isUser) {
                                RoundedCornerShape(16.dp, 16.dp, 4.dp, 16.dp)
                            } else {
                                RoundedCornerShape(16.dp, 16.dp, 16.dp, 4.dp)
                            },
                        )
                        .padding(horizontal = LolaSpacing.lg, vertical = LolaSpacing.md),
                ) {
                    if (isUser) {
                        Text(
                            text = line.text,
                            color = MaterialTheme.colorScheme.onPrimaryContainer,
                            style = MaterialTheme.typography.bodyMedium,
                        )
                    } else {
                        AssistantBody(text = line.text, isStreaming = isStreaming)
                    }
                }
            }
        }
    }
}

/**
 * Splits the reference model's raw output at the literal `</think>` marker:
 * reasoning is collapsed behind a disclosure, the answer renders as the body.
 * While streaming with no marker yet, a typing indicator heads the bubble.
 */
@Composable
private fun AssistantBody(text: String, isStreaming: Boolean) {
    val marker = "</think>"
    val markerIndex = text.indexOf(marker)
    var expanded by rememberSaveable(text.isEmpty()) { mutableStateOf(false) }
    val reasoning: String
    val answer: String
    if (markerIndex >= 0) {
        reasoning = text.substring(0, markerIndex).trim()
        answer = text.substring(markerIndex + marker.length).trim()
    } else {
        reasoning = ""
        answer = text
    }
    val rotation by animateFloatAsState(
        targetValue = if (expanded) 180f else 0f,
        animationSpec = tween(150),
        label = "chevron",
    )

    Column(
        verticalArrangement = Arrangement.spacedBy(LolaSpacing.sm),
        modifier = Modifier.animateContentSize(),
    ) {
        if (reasoning.isNotEmpty()) {
            Row(
                modifier = Modifier.clickable { expanded = !expanded },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(LolaSpacing.xs),
            ) {
                Text(
                    text = stringResource(R.string.chat_reasoning),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.labelSmall,
                )
                Icon(
                    painter = painterResource(R.drawable.ic_expand),
                    contentDescription = null,
                    modifier = Modifier
                        .size(16.dp)
                        .rotate(rotation),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (expanded) {
                Text(
                    text = reasoning,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
        when {
            answer.isNotEmpty() -> Text(
                text = answer,
                color = MaterialTheme.colorScheme.onSurface,
                style = MaterialTheme.typography.bodyMedium,
            )
            isStreaming && reasoning.isEmpty() -> TypingDots()
            else -> Text(
                text = "…",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }
}

@Composable
private fun TypingDots() {
    val transition = rememberInfiniteTransition(label = "typing")
    val alpha by transition.animateFloat(
        initialValue = 0.25f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(500),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "dot",
    )
    Row(horizontalArrangement = Arrangement.spacedBy(LolaSpacing.xs)) {
        repeat(3) {
            Box(
                modifier = Modifier
                    .size(6.dp)
                    .graphicsLayer { this.alpha = alpha }
                    .background(
                        MaterialTheme.colorScheme.onSurfaceVariant,
                        CircleShape,
                    ),
            )
        }
    }
}
