package ai.lola.ace.data

import org.junit.Assert.assertEquals
import org.junit.Test

class ConversationTitleTest {
    @Test
    fun collapsesWhitespaceAndTrimsEdges() {
        assertEquals(
            "Olá, mundo",
            ConversationTitle.fromFirstMessage("  Olá,\n\t mundo  "),
        )
    }

    @Test
    fun usesDefaultForBlankText() {
        assertEquals(
            ChatRepository.DEFAULT_CONVERSATION_TITLE,
            ConversationTitle.fromFirstMessage("  \n "),
        )
    }

    @Test
    fun truncatesLongTitles() {
        val title = ConversationTitle.fromFirstMessage("a".repeat(100))

        assertEquals(44, title.length)
        assertEquals("…", title.takeLast(1))
    }

    @Test
    fun keepsShortUnicodeTitles() {
        assertEquals("Olá 👋", ConversationTitle.fromFirstMessage("Olá 👋"))
    }
}
