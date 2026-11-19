package ai.lola.ace.data

import android.content.ContentValues
import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper

data class StoredConversation(
    val id: Long,
    val title: String,
    val updatedAtEpochMillis: Long,
)

data class StoredChatMessage(
    val id: Long,
    val conversationId: Long,
    val role: String,
    val content: String,
    val createdAtEpochMillis: Long,
)

class ChatRepository(context: Context) {
    private val helper = ChatDatabase(context.applicationContext)

    fun listConversations(limit: Int = DEFAULT_HISTORY_LIMIT): List<StoredConversation> {
        require(limit in 1..MAX_HISTORY_LIMIT)
        val result = mutableListOf<StoredConversation>()
        helper.readableDatabase.rawQuery(
            """
            SELECT _id, title, updated_at
            FROM conversations
            ORDER BY updated_at DESC, _id DESC
            LIMIT ?
            """.trimIndent(),
            arrayOf(limit.toString()),
        ).use { cursor ->
            while (cursor.moveToNext()) {
                result += StoredConversation(
                    id = cursor.getLong(0),
                    title = cursor.getString(1),
                    updatedAtEpochMillis = cursor.getLong(2),
                )
            }
        }
        return result
    }

    fun latestConversationId(): Long? =
        helper.readableDatabase.rawQuery(
            "SELECT _id FROM conversations ORDER BY updated_at DESC, _id DESC LIMIT 1",
            null,
        ).use { cursor ->
            if (cursor.moveToFirst()) cursor.getLong(0) else null
        }

    fun createConversation(nowEpochMillis: Long = System.currentTimeMillis()): Long {
        val values = ContentValues().apply {
            put("title", DEFAULT_CONVERSATION_TITLE)
            put("created_at", nowEpochMillis)
            put("updated_at", nowEpochMillis)
        }
        return helper.writableDatabase.insertOrThrow("conversations", null, values)
    }

    fun loadMessages(conversationId: Long): List<StoredChatMessage> {
        val result = mutableListOf<StoredChatMessage>()
        helper.readableDatabase.rawQuery(
            """
            SELECT _id, conversation_id, role, content, created_at
            FROM messages
            WHERE conversation_id = ?
            ORDER BY created_at ASC, _id ASC
            """.trimIndent(),
            arrayOf(conversationId.toString()),
        ).use { cursor ->
            while (cursor.moveToNext()) {
                result += StoredChatMessage(
                    id = cursor.getLong(0),
                    conversationId = cursor.getLong(1),
                    role = cursor.getString(2),
                    content = cursor.getString(3),
                    createdAtEpochMillis = cursor.getLong(4),
                )
            }
        }
        return result
    }

    fun appendMessage(
        conversationId: Long,
        role: String,
        content: String,
        nowEpochMillis: Long = System.currentTimeMillis(),
    ): Long {
        require(role in CHAT_ROLES) { "Unsupported chat message role" }
        require(content.isNotBlank()) { "Chat message content cannot be blank" }

        val database = helper.writableDatabase
        database.beginTransaction()
        try {
            val message = ContentValues().apply {
                put("conversation_id", conversationId)
                put("role", role)
                put("content", content)
                put("created_at", nowEpochMillis)
            }
            val messageId = database.insertOrThrow("messages", null, message)

            val conversationUpdate = ContentValues().apply {
                put("updated_at", nowEpochMillis)
            }
            database.update(
                "conversations",
                conversationUpdate,
                "_id = ?",
                arrayOf(conversationId.toString()),
            )

            if (role == "user") {
                val title = ConversationTitle.fromFirstMessage(content)
                val titleUpdate = ContentValues().apply { put("title", title) }
                database.update(
                    "conversations",
                    titleUpdate,
                    "_id = ? AND title = ?",
                    arrayOf(conversationId.toString(), DEFAULT_CONVERSATION_TITLE),
                )
            }

            database.setTransactionSuccessful()
            return messageId
        } finally {
            database.endTransaction()
        }
    }

    fun deleteConversation(conversationId: Long): Boolean {
        return helper.writableDatabase.delete(
            "conversations",
            "_id = ?",
            arrayOf(conversationId.toString()),
        ) > 0
    }

    fun close() {
        helper.close()
    }

    private class ChatDatabase(context: Context) :
        SQLiteOpenHelper(context, DATABASE_NAME, null, DATABASE_VERSION) {

        override fun onConfigure(database: SQLiteDatabase) {
            super.onConfigure(database)
            database.setForeignKeyConstraintsEnabled(true)
        }

        override fun onCreate(database: SQLiteDatabase) {
            database.execSQL(
                """
                CREATE TABLE conversations (
                    _id INTEGER PRIMARY KEY AUTOINCREMENT,
                    title TEXT NOT NULL DEFAULT '$DEFAULT_CONVERSATION_TITLE',
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                )
                """.trimIndent(),
            )
            database.execSQL(
                """
                CREATE TABLE messages (
                    _id INTEGER PRIMARY KEY AUTOINCREMENT,
                    conversation_id INTEGER NOT NULL,
                    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
                    content TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    FOREIGN KEY(conversation_id) REFERENCES conversations(_id) ON DELETE CASCADE
                )
                """.trimIndent(),
            )
            database.execSQL(
                "CREATE INDEX index_messages_conversation_time ON messages(conversation_id, created_at, _id)",
            )
        }

        override fun onUpgrade(database: SQLiteDatabase, oldVersion: Int, newVersion: Int) {
            // Schema changes must use explicit forward migrations; never silently discard chat history.
            throw IllegalStateException(
                "Missing explicit chat database migration from $oldVersion to $newVersion",
            )
        }
    }

    companion object {
        const val DEFAULT_CONVERSATION_TITLE = "Nova conversa"
        private const val DATABASE_NAME = "lola-chat.db"
        private const val DATABASE_VERSION = 1
        private const val DEFAULT_HISTORY_LIMIT = 30
        private const val MAX_HISTORY_LIMIT = 100
        private val CHAT_ROLES = setOf("user", "assistant", "system")
    }
}

object ConversationTitle {
    private const val MAX_TITLE_LENGTH = 44

    fun fromFirstMessage(message: String): String {
        val normalized = message.trim().replace(Regex("\\s+"), " ")
        if (normalized.isEmpty()) return ChatRepository.DEFAULT_CONVERSATION_TITLE
        return if (normalized.length <= MAX_TITLE_LENGTH) {
            normalized
        } else {
            normalized.take(MAX_TITLE_LENGTH - 1).trimEnd() + "…"
        }
    }
}

object ChatRepositoryProvider {
    @Volatile
    private var instance: ChatRepository? = null

    fun get(context: Context): ChatRepository =
        instance ?: synchronized(this) {
            instance ?: ChatRepository(context.applicationContext).also { instance = it }
        }
}
