package ai.lola.ace.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import ai.lola.ace.R
import ai.lola.ace.data.ChatRepositoryProvider
import ai.lola.ace.data.StoredConversation
import ai.lola.ace.ui.theme.LolaSpacing
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

/**
 * The Chats tab's home: every conversation, grouped by last activity.
 * Browsing/opening/creating only — deletion stays inside a thread, where
 * the thread's own state owns the consequence.
 */
@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun ConversationsHomeScreen(
    currentConversationId: Long?,
    onOpenConversation: (Long) -> Unit,
    onNewConversation: () -> Unit,
) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val repository = remember(context) { ChatRepositoryProvider.get(context) }
    var conversations by remember { mutableStateOf<List<StoredConversation>>(emptyList()) }
    var loading by remember { mutableStateOf(true) }

    LaunchedEffect(Unit) {
        runCatching {
            conversations = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                repository.listConversations()
            }
        }
        loading = false
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = LolaSpacing.lg, vertical = LolaSpacing.md),
        verticalArrangement = Arrangement.spacedBy(LolaSpacing.md),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = stringResource(R.string.home_title),
                    style = MaterialTheme.typography.headlineSmall,
                )
                Text(
                    text = stringResource(R.string.home_subtitle),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            TextButton(onClick = onNewConversation) {
                Text(stringResource(R.string.chat_new))
            }
        }

        when {
            loading -> Text(
                text = stringResource(R.string.home_loading),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyMedium,
            )
            conversations.isEmpty() -> Text(
                text = stringResource(R.string.home_empty),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyMedium,
            )
            else -> ConversationGroups(
                conversations = conversations,
                currentConversationId = currentConversationId,
                onOpenConversation = onOpenConversation,
            )
        }
    }
}

@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
private fun ConversationGroups(
    conversations: List<StoredConversation>,
    currentConversationId: Long?,
    onOpenConversation: (Long) -> Unit,
) {
    val zone = remember { ZoneId.systemDefault() }
    val today = remember { LocalDate.now(zone) }
    val yesterday = remember { today.minusDays(1) }

    data class Group(val label: String, val items: List<StoredConversation>)

    val groups = listOf(
        Group(stringResource(R.string.home_group_today), conversations.filter {
            Instant.ofEpochMilli(it.updatedAtEpochMillis).atZone(zone).toLocalDate() == today
        }),
        Group(stringResource(R.string.home_group_yesterday), conversations.filter {
            Instant.ofEpochMilli(it.updatedAtEpochMillis).atZone(zone).toLocalDate() == yesterday
        }),
        Group(stringResource(R.string.home_group_earlier), conversations.filter {
            val date = Instant.ofEpochMilli(it.updatedAtEpochMillis).atZone(zone).toLocalDate()
            date.isBefore(yesterday)
        }),
    ).filter { it.items.isNotEmpty() }

    LazyColumn(
        contentPadding = PaddingValues(vertical = LolaSpacing.sm),
        verticalArrangement = Arrangement.spacedBy(LolaSpacing.sm),
    ) {
        groups.forEach { group ->
            item(key = "header-${group.label}") {
                Text(
                    text = group.label.uppercase(),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.labelSmall,
                )
            }
            items(group.items, key = { it.id }) { conversation ->
                Card(
                    onClick = { onOpenConversation(conversation.id) },
                    modifier = Modifier.fillMaxWidth(),
                    shape = MaterialTheme.shapes.medium,
                    colors = CardDefaults.cardColors(
                        containerColor = if (conversation.id == currentConversationId) {
                            MaterialTheme.colorScheme.primaryContainer
                        } else {
                            MaterialTheme.colorScheme.surfaceVariant
                        },
                    ),
                ) {
                    Column(
                        modifier = Modifier.padding(
                            horizontal = LolaSpacing.lg,
                            vertical = LolaSpacing.md,
                        ),
                        verticalArrangement = Arrangement.spacedBy(LolaSpacing.xs),
                    ) {
                        Text(
                            text = conversation.title,
                            style = MaterialTheme.typography.titleMedium,
                            color = if (conversation.id == currentConversationId) {
                                MaterialTheme.colorScheme.onPrimaryContainer
                            } else {
                                MaterialTheme.colorScheme.onSurface
                            },
                        )
                        Text(
                            text = formatDateLabel(conversation.updatedAtEpochMillis, zone),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            style = MaterialTheme.typography.labelSmall,
                        )
                    }
                }
            }
        }
    }
}

private fun formatDateLabel(epochMillis: Long, zone: ZoneId): String {
    val dateTime = Instant.ofEpochMilli(epochMillis).atZone(zone)
    return "${
        dateTime.dayOfWeek.getDisplayName(java.time.format.TextStyle.FULL, java.util.Locale.ROOT)
    } · ${
        dateTime.format(java.time.format.DateTimeFormatter.ofPattern("HH:mm"))
    }"
}
