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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import ai.lola.ace.data.ModelArtifactSummary
import ai.lola.ace.data.ModelTransferManager
import ai.lola.ace.inference.InferenceChatMessage
import ai.lola.ace.inference.LocalInferenceBackend
import ai.lola.ace.inference.LocalInferenceBackendProvider
import ai.lola.ace.inference.LocalInferenceState
import ai.lola.ace.inference.ReferenceSessionConfig
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

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
    onOpenModels: () -> Unit,
) {
    val backend = remember { LocalInferenceBackendProvider.get() }
    val runtimeState by backend.state.collectAsState()
    val scope = rememberCoroutineScope()
    val chatLines = remember { mutableStateListOf<ChatLine>() }
    val listState = rememberLazyListState()

    var transferVersion by remember { mutableIntStateOf(0) }
    var input by rememberSaveable { mutableStateOf("") }
    var sessionReady by remember { mutableStateOf(false) }
    var generating by remember { mutableStateOf(false) }
    var localError by remember { mutableStateOf<String?>(null) }

    DisposableEffect(transferManager) {
        val subscription = transferManager.subscribe { transferVersion++ }
        onDispose { subscription.close() }
    }

    val installedFile = remember(model, transferVersion) {
        model?.let(transferManager::installedModelFile)
    }

    LaunchedEffect(runtimeState, model?.id) {
        if (runtimeState is LocalInferenceState.Loaded && model != null) {
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
                transferManager.setModelInUse(model.id, true)
                localError = null
            } catch (error: Exception) {
                sessionReady = false
                localError = error.message ?: "Não foi possível abrir a sessão local."
            }
        } else {
            sessionReady = false
            if (runtimeState !is LocalInferenceState.Loaded) {
                model?.let { transferManager.setModelInUse(it.id, false) }
            }
        }
    }

    LaunchedEffect(chatLines.size) {
        if (chatLines.isNotEmpty()) {
            listState.animateScrollToItem(chatLines.lastIndex)
        }
    }

    fun loadModel() {
        val file = installedFile ?: return
        scope.launch {
            localError = null
            try {
                backend.loadModel(file)
            } catch (error: Exception) {
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
            } catch (error: Exception) {
                localError = error.message ?: "Não foi possível libertar o modelo."
            }
        }
    }

    fun sendMessage() {
        val text = input.trim()
        if (text.isEmpty() || !sessionReady || generating) return

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
            try {
                backend.generate(
                    sessionId = CHAT_SESSION_ID,
                    messages = history,
                    maxOutputTokens = MAX_OUTPUT_TOKENS,
                ).collect { delta ->
                    val last = chatLines.lastIndex
                    if (last >= 0 && chatLines[last].role == "assistant") {
                        chatLines[last] = chatLines[last].copy(text = chatLines[last].text + delta)
                    }
                }
            } catch (_: CancellationException) {
                // A user cancellation is a normal end to local generation.
            } catch (error: Exception) {
                localError = error.message ?: "Erro durante a geração local."
            } finally {
                generating = false
            }
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
            Text(
                text = "CONVERSA LOCAL",
                color = MaterialTheme.colorScheme.primary,
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = model?.displayName ?: "Nenhum modelo no catálogo",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
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
                    Text("A abrir sessão de conversa…")
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

        if (runtimeState is LocalInferenceState.Loaded) {
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
                            text = "O modelo está pronto. Envia uma mensagem para começar. A primeira resposta pode demorar enquanto o motor prepara o contexto.",
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            style = MaterialTheme.typography.bodyMedium,
                        )
                    }
                }
            }

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedTextField(
                    value = input,
                    onValueChange = { input = it },
                    modifier = Modifier.weight(1f),
                    enabled = sessionReady && !generating,
                    placeholder = { Text("Escreve uma mensagem…") },
                    maxLines = 4,
                )
                Button(
                    onClick = ::sendMessage,
                    enabled = sessionReady && !generating && input.isNotBlank(),
                ) {
                    Text("Enviar")
                }
            }
            Text(
                text = "Motor de referência llama.cpp · CPU · contexto 2K · máximo 256 tokens por resposta · conversa não persistida",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.labelSmall,
            )
        } else {
            Spacer(modifier = Modifier.height(2.dp))
            Text(
                text = "Ainda não há geração de texto disponível. A interface só activa o chat depois de o modelo estar instalado, carregado e com uma sessão válida.",
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
