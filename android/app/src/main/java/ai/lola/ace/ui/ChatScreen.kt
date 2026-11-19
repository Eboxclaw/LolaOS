package ai.lola.ace.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.weight
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import ai.lola.ace.data.ChatRepository
import ai.lola.ace.data.ChatRepositoryProvider
import ai.lola.ace.data.ModelArtifactSummary
import ai.lola.ace.data.ModelTransferManager
import ai.lola.ace.data.StoredConversation
import ai.lola.ace.inference.InferenceChatMessage
import ai.lola.ace.inference.LocalInferenceBackend
import ai.lola.ace.inference.LocalInferenceState
import ai.lola.ace.inference.ReferenceSessionConfig
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
    var historyLoading by remember { mutableStateOf(true) }

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
        } catch (error: Exception) {
            localError = error.message ?: "Não foi possível abrir o histórico local."
        } finally {
            historyLoading = false
        }
    }

    LaunchedEffect(currentConversationId) {
        val conversationId = currentConversationId ?: return@LaunchedEffect
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
        } catch (error: Exception) {
            localError = error.message ?: "Não foi possível carregar esta conversa."
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
        scope.launch {
            try {
                val newId = withContext(Dispatchers.IO) {
                    repository.createConversation()
                }
                currentConversationId = newId
                refreshConversations()
                showHistory = false
                localError = null
            } catch (error: Exception) {
                localError = error.message ?: "Não foi possível criar uma conversa."
            }
        }
    }

    fun selectConversation(conversationId: Long) {
        if (generating || currentConversationId == conversationId) {
            showHistory = false
            return
        }
        currentConversationId = conversationId
        showHistory = false
        localError = null
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
                chatLines.clear()
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
        if (text.isEmpty() || !sessionReady || generating || conversationId == null) return

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
            } catch (_: CancellationException) {
                // A user cancellation is a normal end to local generation.
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
                enabled = !generating && !historyLoading,
            ) { Text("Nova conversa") }
            Spacer(modifier = Modifier.weight(1f))
            Column {
                TextButton(
                    onClick = { showHistory = true },
                    enabled = conversations.isNotEmpty() && !historyLoading && !generating,
                ) { Text("Histórico (${conversations.size})") }
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

        Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
            Text(
                text = "CONVERSA LOCAL",
                color = MaterialTheme.colorScheme.primary,
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = currentConversationTitle,
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = model?.displayName ?: "Nenhum modelo no catálogo",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                text = when (runtimeState) {
                    LocalInferenceState.Unloaded -> if (installedFile == null) {
                        "Descarrega e verifica o modelo na biblioteca para começar."
                    } else {
                        "Modelo instalado e verificado. Ainda não está carregado na memória."
                    }
                    LocalInferenceState.Loading -> "A carregar o modelo na memória do dispositivo…"
                    is LocalInferenceState.Loaded -> if (sessionReady) {
                        "Motor de referência activo · CPU ARM64"
                    } else {
                        "Modelo carregado · a preparar a sessão"
                    }
                    is LocalInferenceState.Error -> "O motor não conseguiu carregar o modelo."
                },
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
        }

        when (runtimeState) {
            LocalInferenceState.Unloaded -> {
                if (!catalogueAvailable || model == null) {
                    Text("Não existe um modelo disponível no catálogo.")
                } else if (installedFile == null) {
                    Card(
                        modifier = Modifier.fillMaxWidth(),
                        colors = CardDefaults.cardColors(
                            containerColor = MaterialTheme.colorScheme.surfaceVariant,
                        ),
                    ) {
                        Column(
                            modifier = Modifier.padding(16.dp),
                            verticalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            Text("O LFM3 ainda não está instalado.")
                            Text(
                                "Abre a biblioteca de modelos, descarrega o GGUF e deixa a LOLA verificar o SHA-256.",
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                style = MaterialTheme.typography.bodySmall,
                            )
                            Button(onClick = onOpenModels) { Text("Abrir biblioteca") }
                        }
                    }
                } else {
                    Button(
                        onClick = ::loadModel,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Carregar LFM3 no dispositivo")
                    }
                }
            }
            LocalInferenceState.Loading -> {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    CircularProgressIndicator()
                    Text("A inicializar o backend CPU…")
                }
            }
            is LocalInferenceState.Loaded -> {
                if (sessionReady) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.End,
                    ) {
                        TextButton(
                            onClick = { backend.cancelGeneration(CHAT_SESSION_ID) },
                            enabled = generating,
                        ) { Text("Parar geração") }
                        TextButton(
                            onClick = ::unloadModel,
                            enabled = !generating,
                        ) { Text("Libertar memória") }
                    }
                } else {
                    Text(
                        text = localError ?: "A abrir sessão de conversa…",
                        color = if (localError != null) {
                            MaterialTheme.colorScheme.error
                        } else {
                            MaterialTheme.colorScheme.onSurfaceVariant
                        },
                    )
                    if (localError != null) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            Button(
                                onClick = { scope.launch { ensureChatSession() } },
                            ) { Text("Tentar abrir sessão") }
                            TextButton(onClick = ::unloadModel) {
                                Text("Libertar memória")
                            }
                        }
                    }
                }
            }
            is LocalInferenceState.Error -> {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.errorContainer,
                    ),
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(
                            text = (runtimeState as LocalInferenceState.Error).message,
                            color = MaterialTheme.colorScheme.onErrorContainer,
                        )
                        TextButton(onClick = ::loadModel) { Text("Tentar carregar novamente") }
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
            itemsIndexed(chatLines) { _, line ->
                ChatMessageBubble(line)
            }
            if (chatLines.isEmpty()) {
                item {
                    Text(
                        text = if (runtimeState is LocalInferenceState.Loaded) {
                            "O modelo está pronto. Envia uma mensagem para começar. A primeira resposta pode demorar enquanto o motor prepara o contexto."
                        } else {
                            "As conversas ficam guardadas neste dispositivo. Carrega o modelo para continuar a conversar."
                        },
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        }

        if (runtimeState is LocalInferenceState.Loaded) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedTextField(
                    value = input,
                    onValueChange = { input = it },
                    modifier = Modifier.weight(1f),
                    enabled = sessionReady && !generating && currentConversationId != null,
                    placeholder = { Text("Escreve uma mensagem…") },
                    maxLines = 4,
                )
                Button(
                    onClick = ::sendMessage,
                    enabled = sessionReady && !generating && input.isNotBlank() && currentConversationId != null,
                ) {
                    Text("Enviar")
                }
            }
            Text(
                text = "Motor de referência llama.cpp · CPU · contexto 2K · máximo 256 tokens por resposta · histórico guardado localmente",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.labelSmall,
            )
        } else {
            Text(
                text = "O histórico continua disponível sem o modelo carregado. Carrega o LFM3 para enviar novas mensagens.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
private fun ChatMessageBubble(line: ChatLine) {
    val isUser = line.role == "user"
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = if (isUser) {
                MaterialTheme.colorScheme.primaryContainer
            } else {
                MaterialTheme.colorScheme.surfaceVariant
            },
        ),
    ) {
        Column(
            modifier = Modifier.padding(13.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                text = if (isUser) "TU" else "LOLA",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = line.text.ifEmpty { "…" },
                color = if (isUser) {
                    MaterialTheme.colorScheme.onPrimaryContainer
                } else {
                    MaterialTheme.colorScheme.onSurface
                },
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }
}
